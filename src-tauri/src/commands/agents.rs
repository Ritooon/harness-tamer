//! AGENTS.md : génération, régénération partielle, édition, versions, export.

use hf_engine::diff::{DiffLine, line_diff};
use hf_engine::export::{ExportOptions, ExportReport, conflicts, export_files, export_to_dir, export_to_zip};
use hf_engine::generation::checks::Issue;
use hf_engine::generation::{GenEvent, GenerateOptions, GenerationResult, check_project, generate};
use hf_engine::model::{Lang, VersionMeta, VersionOrigin};
use hf_engine::store::VersionDraft;
use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::consent::{confirm, current_lang};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[tauri::command]
pub async fn generate_agents(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    options: GenerateOptions,
    on_event: Channel<GenEvent>,
) -> CmdResult<GenerationResult> {
    let settings = state.settings();
    let backend = state.runtime.backend(&app, &state.store, &settings).await?;
    let op = format!("generate:{id}");
    let cancel = state.begin_cancellable(&op);
    let send = |event: GenEvent| {
        let _ = on_event.send(event);
    };
    let result = generate(&state.store, backend.as_ref(), &id, options, &send, &cancel).await;
    state.end_cancellable(&op);
    Ok(result?)
}

/// Annule une opération longue (`generate:<id>`, `skill:propose`…).
#[tauri::command]
pub fn cancel_operation(state: State<'_, AppState>, op: String) {
    state.cancel(&op);
}

/// Enregistre une édition manuelle comme nouvelle version.
#[tauri::command]
pub fn save_agents(
    state: State<'_, AppState>,
    id: String,
    content: String,
    note: Option<String>,
) -> CmdResult<VersionMeta> {
    Ok(state.store.save_agents_version(
        &id,
        &content,
        VersionDraft {
            origin: Some(VersionOrigin::ManualEdit),
            note: note.filter(|n| !n.trim().is_empty()),
            ..Default::default()
        },
    )?)
}

#[tauri::command]
pub fn read_version(state: State<'_, AppState>, id: String, number: u32) -> CmdResult<String> {
    Ok(state.store.read_version(&id, number)?)
}

#[tauri::command]
pub fn restore_version(state: State<'_, AppState>, id: String, number: u32) -> CmdResult<VersionMeta> {
    Ok(state.store.restore_version(&id, number)?)
}

/// Différences entre une version et une autre (ou la version courante).
#[tauri::command]
pub fn diff_versions(state: State<'_, AppState>, id: String, from: u32, to: Option<u32>) -> CmdResult<Vec<DiffLine>> {
    let old = state.store.read_version(&id, from)?;
    let new = match to {
        Some(n) => state.store.read_version(&id, n)?,
        None => state.store.load_agents(&id)?.unwrap_or_default(),
    };
    Ok(line_diff(&old, &new))
}

#[tauri::command]
pub fn check_agents(state: State<'_, AppState>, id: String) -> CmdResult<Vec<Issue>> {
    Ok(check_project(&state.store, &id)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Folder,
    Zip,
}

/// Export vers un dossier (la racine d'un dépôt) ou une archive ZIP, choisis
/// dans un sélecteur natif. Si des fichiers existent déjà, une confirmation
/// native les liste avant de les remplacer. Rend `None` si l'utilisateur annule.
#[tauri::command]
pub async fn export_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    kind: ExportKind,
    mut options: ExportOptions,
) -> CmdResult<Option<ExportReport>> {
    let project = state.store.get_project(&id)?;
    let lang = current_lang(&state.lang);
    let default_dir = state.settings().default_export_dir;
    let picker = {
        let app = app.clone();
        let file_name = format!("{}-agents.zip", hf_engine::text::slugify(&project.name));
        tauri::async_runtime::spawn_blocking(move || {
            let mut dialog = app.dialog().file();
            if let Some(dir) = default_dir {
                dialog = dialog.set_directory(dir);
            }
            match kind {
                ExportKind::Folder => dialog.blocking_pick_folder(),
                ExportKind::Zip => dialog.add_filter("ZIP", &["zip"]).set_file_name(file_name).blocking_save_file(),
            }
        })
        .await?
    };
    let Some(target) = picker.and_then(|p| p.into_path().ok()) else { return Ok(None) };

    let is_zip = target.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let existing = if is_zip {
        if target.exists() { vec![target.display().to_string()] } else { vec![] }
    } else {
        conflicts(&target, &export_files(&state.store, &id, &options)?)
    };
    if !existing.is_empty() && !options.overwrite {
        let list = existing.iter().map(|f| format!("  • {f}")).collect::<Vec<_>>().join("\n");
        let (title, message, label) = match lang {
            Lang::Fr => (
                "Remplacer les fichiers ?",
                format!("Ces fichiers existent déjà et seront remplacés :\n{list}"),
                "Remplacer",
            ),
            Lang::En => {
                ("Replace files?", format!("These files already exist and will be replaced:\n{list}"), "Replace")
            }
        };
        let confirmed = {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || confirm(&app, lang, title, &message, label)).await?
        };
        if !confirmed {
            return Ok(None);
        }
        options.overwrite = true;
    }
    let report = if is_zip {
        export_to_zip(&state.store, &id, &target, &options)?
    } else {
        export_to_dir(&state.store, &id, &target, &options)?
    };
    Ok(Some(report))
}

/// Montre un fichier ou un dossier dans le gestionnaire de fichiers.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(&path).map_err(|e| CmdError::new("app", e.to_string()))
}
