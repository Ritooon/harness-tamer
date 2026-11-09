//! Réglages, modèles, moteur d'inférence, journal réseau et liens externes.

use hf_engine::llm::catalog::{ImportedModel, ModelStatus, download_model};
use hf_engine::llm::hardware::{HardwareInfo, Recommendation, recommend};
use hf_engine::model::Settings;
use hf_net::{ActionKind, JournalEntry, NetAction};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::runtime::RuntimeStatus;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    state.save_settings(settings)
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub hardware: HardwareInfo,
    pub recommendation: Recommendation,
    pub data_dir: String,
    pub models_dir: String,
    pub version: &'static str,
}

#[tauri::command]
pub fn system_info(state: State<'_, AppState>) -> SystemInfo {
    let hardware = HardwareInfo::detect();
    let library = state.library();
    SystemInfo {
        recommendation: recommend(&hardware, library.catalog()),
        hardware,
        data_dir: state.store.root().display().to_string(),
        models_dir: library.dir().display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> CmdResult<Vec<ModelStatus>> {
    Ok(state.library().statuses()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub bytes: u64,
    pub total: Option<u64>,
}

/// Télécharge un modèle du catalogue : dialogue de consentement natif,
/// reprise d'un fichier partiel, vérification SHA-256.
#[tauri::command]
pub async fn start_model_download(
    state: State<'_, AppState>,
    model_id: String,
    on_progress: Channel<DownloadProgress>,
) -> CmdResult<String> {
    let gated = state.gated.clone();
    let library = state.library();
    let lang = state.settings().ui_language;
    let id = model_id.clone();
    let task = tokio::spawn(async move {
        let progress = |bytes: u64, total: Option<u64>| {
            let _ = on_progress.send(DownloadProgress { model_id: id.clone(), bytes, total });
        };
        download_model(&gated, &library, &id, lang, &progress).await
    });
    // La poignée d'annulation est conservée le temps du téléchargement.
    state.downloads.lock().unwrap_or_else(|p| p.into_inner()).insert(model_id.clone(), task.abort_handle());
    let outcome = task.await;
    state.downloads.lock().unwrap_or_else(|p| p.into_inner()).remove(&model_id);
    match outcome {
        Ok(Ok(path)) => Ok(path.display().to_string()),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(CmdError::new("cancelled", "download cancelled; the partial file is kept for resuming")),
    }
}

/// Interrompt un téléchargement ; le fichier `.part` permet de reprendre.
#[tauri::command]
pub fn cancel_model_download(state: State<'_, AppState>, model_id: String) {
    if let Some(handle) = state.downloads.lock().unwrap_or_else(|p| p.into_inner()).remove(&model_id) {
        handle.abort();
    }
}

/// Enregistre un fichier GGUF déjà présent sur le disque (aucun réseau).
#[tauri::command]
pub async fn import_model_file(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<ImportedModel>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("GGUF", &["gguf"]).blocking_pick_file()
    })
    .await?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    Ok(Some(state.library().import(&path)?))
}

#[tauri::command]
pub fn forget_model(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.library().forget(&model_id)?)
}

#[tauri::command]
pub fn runtime_status(state: State<'_, AppState>) -> RuntimeStatus {
    state.runtime.status()
}

#[tauri::command]
pub fn runtime_log(state: State<'_, AppState>) -> Vec<String> {
    state.runtime.log_tail()
}

/// Démarre le moteur avec les réglages courants (chargement du modèle).
#[tauri::command]
pub async fn start_runtime(app: AppHandle, state: State<'_, AppState>) -> CmdResult<RuntimeStatus> {
    state.runtime.backend(&app, &state.store, &state.settings()).await?;
    Ok(state.runtime.status())
}

#[tauri::command]
pub async fn stop_runtime(state: State<'_, AppState>) -> CmdResult<RuntimeStatus> {
    state.runtime.stop().await;
    Ok(state.runtime.status())
}

#[tauri::command]
pub fn read_network_log(state: State<'_, AppState>, limit: Option<usize>) -> CmdResult<Vec<JournalEntry>> {
    Ok(state.gated.journal().read_recent(limit.unwrap_or(200))?)
}

/// Ouvre un lien dans le navigateur : c'est un accès à Internet, donc
/// soumis au même dialogue de consentement et au même journal.
#[tauri::command]
pub async fn open_external_link(app: AppHandle, state: State<'_, AppState>, url: String) -> CmdResult<bool> {
    let parsed = url::Url::parse(&url).map_err(|e| CmdError::new("net.invalid_url", e.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(CmdError::new("net.insecure_url", url));
    }
    let purpose = match state.settings().ui_language {
        hf_engine::model::Lang::Fr => "Ouvrir ce lien dans votre navigateur.".to_owned(),
        hf_engine::model::Lang::En => "Open this link in your browser.".to_owned(),
    };
    match state.gated.begin(NetAction::new(ActionKind::OpenLink, purpose, vec![parsed.clone()])).await {
        Ok(_) => {
            app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| CmdError::new("app", e.to_string()))?;
            Ok(true)
        }
        Err(hf_net::NetError::Refused) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[tauri::command]
pub fn reveal_data_dir(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(state.store.settings_path()).map_err(|e| CmdError::new("app", e.to_string()))
}
