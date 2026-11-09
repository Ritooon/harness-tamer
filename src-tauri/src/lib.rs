//! Application de bureau HarnessForge (Tauri 2).
//!
//! Rôle de cette couche : relier l'interface React au cœur `hf-engine`,
//! afficher les dialogues natifs (consentement réseau, confirmations,
//! sélecteurs de fichiers) et piloter `llama-server`. Aucune logique métier
//! ici, et aucun accès réseau hors de `hf-net`.

mod commands;
mod consent;
mod error;
mod runtime;
mod state;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use hf_engine::store::Store;
use hf_net::{GatedClient, GatedConfig, Journal};
use tauri::webview::NewWindowResponse;
use tauri::{App, Manager, RunEvent, Url, WebviewUrl, WebviewWindowBuilder};

use crate::consent::NativeConsent;
use crate::state::AppState;

/// Dossier de données : `HARNESSFORGE_DATA_DIR` (installation portable,
/// tests) sinon le dossier de données standard de l'OS.
fn data_dir(app: &App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(dir) = std::env::var_os("HARNESSFORGE_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(app.path().app_data_dir()?)
}

/// La fenêtre ne peut afficher que l'application elle-même : tout autre
/// chargement (lien cliqué, redirection) est bloqué. Les liens externes
/// passent par `open_external_link`, donc par le dialogue de consentement.
fn is_app_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "about" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            // Serveur de développement Vite, en mode debug uniquement.
            Some("localhost" | "127.0.0.1") => cfg!(debug_assertions) && url.port() == Some(1420),
            _ => false,
        },
        _ => false,
    }
}

fn create_main_window(app: &App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("HarnessForge")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 640.0)
        .on_navigation(is_app_url)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()?;
    Ok(())
}

fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(data_dir(app)?)?;
    // Réglages illisibles (écrits par une version plus récente…) : valeurs
    // par défaut, donc politique réseau « Demander ».
    let settings = store.load_settings().unwrap_or_default();
    let lang = Arc::new(RwLock::new(settings.ui_language));
    let journal = Arc::new(Journal::new(store.journal_path()));
    let consent = Arc::new(NativeConsent { app: app.handle().clone(), lang: Arc::clone(&lang) });
    let gated = Arc::new(GatedClient::new(settings.network_policy.into(), consent, journal, GatedConfig::default()));
    app.manage(AppState::new(store, gated, lang, settings));
    create_main_window(app)?;
    Ok(())
}

pub fn run() {
    use commands::{agents, projects, settings, skills};
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        // Les liens cliqués dans l'interface ne s'ouvrent pas tout seuls.
        .plugin(tauri_plugin_opener::Builder::new().open_js_links_on_click(false).build())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            projects::list_projects,
            projects::get_project,
            projects::create_project,
            projects::update_project,
            projects::set_project_status,
            projects::delete_project,
            projects::analyze_repo,
            projects::pick_folder,
            projects::import_brand_files,
            projects::remove_brand_file,
            agents::generate_agents,
            agents::cancel_operation,
            agents::save_agents,
            agents::read_version,
            agents::restore_version,
            agents::diff_versions,
            agents::check_agents,
            agents::export_project,
            agents::reveal_path,
            skills::list_skills,
            skills::get_skill,
            skills::blank_skill_template,
            skills::save_new_skill,
            skills::update_skill,
            skills::delete_skill,
            skills::propose_new_skill,
            skills::list_project_skills,
            skills::attach_skill,
            skills::detach_skill,
            skills::update_project_skill,
            skills::adapt_project_skill,
            skills::search_online_skills,
            skills::import_online_skill,
            settings::get_settings,
            settings::save_settings,
            settings::system_info,
            settings::list_models,
            settings::start_model_download,
            settings::cancel_model_download,
            settings::import_model_file,
            settings::forget_model,
            settings::runtime_status,
            settings::runtime_log,
            settings::start_runtime,
            settings::stop_runtime,
            settings::read_network_log,
            settings::open_external_link,
            settings::reveal_data_dir,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build HarnessForge")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<AppState>().runtime.stop_now();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_app_itself_can_be_loaded() {
        let ok = |u: &str| is_app_url(&Url::parse(u).unwrap());
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/"));
        assert!(!ok("https://example.org/"));
        assert!(!ok("http://localhost:8080/"));
        assert!(!ok("file:///etc/passwd"));
    }
}
