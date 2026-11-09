//! Projets : liste, formulaire, archivage, suppression, analyse du dépôt et charte.

use hf_engine::brand::BrandKit;
use hf_engine::generation::{ensure_brand_kit, ensure_repo_profile};
use hf_engine::index::{ProjectQuery, query_projects};
use hf_engine::model::{Lang, Project, ProjectInput, ProjectStatus, ProjectSummary, SkillMeta, VersionMeta};
use hf_engine::scan::RepoProfile;
use hf_engine::store::SystemTrash;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::consent::{confirm, current_lang};
use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ProjectDetail {
    pub project: Project,
    pub agents: Option<String>,
    pub versions: Vec<VersionMeta>,
    pub skills: Vec<SkillMeta>,
    pub repo_profile: Option<RepoProfile>,
    pub brand_kit: Option<BrandKit>,
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>, query: ProjectQuery) -> CmdResult<Vec<ProjectSummary>> {
    Ok(query_projects(state.store.list_projects()?, &query))
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>, id: String) -> CmdResult<ProjectDetail> {
    let store = &state.store;
    Ok(ProjectDetail {
        project: store.get_project(&id)?,
        agents: store.load_agents(&id)?,
        versions: store.list_versions(&id)?,
        skills: store.list_project_skills(&id)?.into_iter().map(|s| s.meta).collect(),
        repo_profile: store.load_repo_profile(&id)?,
        brand_kit: store.load_brand_kit(&id)?,
    })
}

#[tauri::command]
pub fn create_project(state: State<'_, AppState>, input: ProjectInput) -> CmdResult<Project> {
    Ok(state.store.create_project(input)?)
}

#[tauri::command]
pub fn update_project(state: State<'_, AppState>, id: String, input: ProjectInput) -> CmdResult<Project> {
    Ok(state.store.update_project(&id, input)?)
}

/// Archiver ou restaurer (réversible, sans confirmation).
#[tauri::command]
pub fn set_project_status(state: State<'_, AppState>, id: String, status: ProjectStatus) -> CmdResult<Project> {
    Ok(state.store.set_status(&id, status)?)
}

/// Suppression vers la corbeille, après confirmation native. Rend `false`
/// si l'utilisateur annule.
#[tauri::command]
pub async fn delete_project(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    let project = state.store.get_project(&id)?;
    let lang = current_lang(&state.lang);
    let (title, message, label) = match lang {
        Lang::Fr => (
            "Supprimer le projet ?",
            format!(
                "Le projet « {} », son AGENTS.md, son historique et ses skills rattachés seront envoyés à la corbeille du système.",
                project.name
            ),
            "Supprimer",
        ),
        Lang::En => (
            "Delete the project?",
            format!(
                "The \"{}\" project, its AGENTS.md, its history and its attached skills will be moved to the system trash.",
                project.name
            ),
            "Delete",
        ),
    };
    let confirmed = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || confirm(&app, lang, title, &message, label)).await?
    };
    if !confirmed {
        return Ok(false);
    }
    state.store.delete_project(&id, &SystemTrash)?;
    Ok(true)
}

/// (Re)lance l'analyse locale du dépôt du projet.
#[tauri::command]
pub async fn analyze_repo(state: State<'_, AppState>, id: String) -> CmdResult<Option<RepoProfile>> {
    let project = state.store.get_project(&id)?;
    Ok(ensure_repo_profile(&state.store, &project, true).await?)
}

/// Sélecteur de dossier natif (dépôt existant, dossier d'export…).
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder()).await?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
}

/// Importe des fichiers de charte choisis dans un sélecteur natif.
#[tauri::command]
pub async fn import_brand_files(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<ProjectDetail> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(
                "Charte / Brand",
                &[
                    "css", "scss", "sass", "less", "json", "svg", "png", "jpg", "jpeg", "webp", "md", "txt", "pdf",
                    "ttf", "otf", "woff", "woff2",
                ],
            )
            .blocking_pick_files()
    })
    .await?;
    for file in picked.unwrap_or_default() {
        if let Ok(path) = file.into_path() {
            state.store.import_brand_file(&id, &path)?;
        }
    }
    let project = state.store.get_project(&id)?;
    ensure_brand_kit(&state.store, &project, true)?;
    get_project(state, id)
}

#[tauri::command]
pub fn remove_brand_file(state: State<'_, AppState>, id: String, name: String) -> CmdResult<ProjectDetail> {
    state.store.remove_brand_file(&id, &name)?;
    let project = state.store.get_project(&id)?;
    if project.brand.files.is_empty() {
        // Plus de fichiers : la charte extraite n'a plus de raison d'être utilisée.
        let _ = std::fs::remove_file(state.store.brand_dir(&id)?.join("brand.json"));
    } else {
        ensure_brand_kit(&state.store, &project, true)?;
    }
    get_project(state, id)
}
