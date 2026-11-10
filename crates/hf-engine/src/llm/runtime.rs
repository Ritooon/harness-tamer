//! Lancement de `llama-server` : arguments, environnement, attente du
//! démarrage. Le processus lui-même est lancé par l'application Tauri
//! (sidecar) ; ce module ne fait que préparer et vérifier.
//!
//! Verrou n°5 de l'ADR 0002 : écoute sur 127.0.0.1 uniquement, port
//! aléatoire, clé d'API éphémère passée par variable d'environnement (elle
//! n'apparaît donc pas dans la liste des processus), `--offline` (aucun
//! téléchargement possible) et `--no-webui`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hf_net::LoopbackClient;
use url::Url;

use super::Cancel;
use super::catalog::ResolvedModel;
use crate::error::{EngineError, Result};
use crate::model::RuntimeSettings;

/// Nom du programme de llama.cpp qui sert le modèle.
pub const SERVER_BINARY: &str = "llama-server";

/// D'où vient le `llama-server` utilisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ServerSource {
    /// Choisi dans les réglages.
    Settings,
    /// Déposé dans `<données>/runtimes/llama/` (build CUDA, Vulkan…).
    UserRuntime,
    /// Livré avec l'application.
    Bundled,
    /// Téléchargé par l'application, sur demande (`<données>/runtimes/llama-cpp/`).
    Downloaded,
    /// Installé sur le système (Homebrew, gestionnaire de paquets, PATH).
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ServerCandidate {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub path: PathBuf,
    pub source: ServerSource,
}

/// Emplacements où chercher `llama-server`, du plus prioritaire au moins
/// prioritaire. Aucune commande n'est lancée : seuls des chemins sont lus.
pub struct ServerSearch<'a> {
    /// Chemin choisi dans les réglages.
    pub configured: Option<&'a Path>,
    /// `<données>/runtimes/llama`.
    pub user_dir: &'a Path,
    /// Dossier `llama` des ressources de l'application.
    pub bundled_dir: Option<&'a Path>,
    /// Dossier géré par l'installation en un clic (voir `llm::install`).
    pub downloaded_dir: Option<&'a Path>,
    /// Valeur de la variable `PATH`.
    pub path_env: Option<&'a OsStr>,
    /// `std::env::consts::OS`.
    pub os: &'a str,
}

impl ServerSearch<'_> {
    pub fn candidates(&self) -> Vec<ServerCandidate> {
        let exe = if self.os == "windows" { format!("{SERVER_BINARY}.exe") } else { SERVER_BINARY.to_owned() };
        let mut out = Vec::new();
        let mut push = |path: PathBuf, source| {
            if !out.iter().any(|c: &ServerCandidate| c.path == path) {
                out.push(ServerCandidate { path, source });
            }
        };
        if let Some(p) = self.configured {
            push(p.to_path_buf(), ServerSource::Settings);
        }
        push(self.user_dir.join(&exe), ServerSource::UserRuntime);
        if let Some(dir) = self.bundled_dir {
            push(dir.join(&exe), ServerSource::Bundled);
        }
        if let Some(dir) = self.downloaded_dir {
            push(dir.join(&exe), ServerSource::Downloaded);
        }
        // Une application lancée depuis le Finder n'hérite pas du PATH du
        // terminal : les emplacements usuels sont ajoutés explicitement.
        let extra: &[&str] = match self.os {
            "macos" => &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"],
            "linux" => &["/usr/local/bin", "/usr/bin", "/snap/bin"],
            _ => &[],
        };
        let path_dirs = self.path_env.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default();
        for dir in path_dirs.into_iter().chain(extra.iter().map(PathBuf::from)) {
            if dir.is_absolute() {
                push(dir.join(&exe), ServerSource::System);
            }
        }
        out
    }

    /// Premier `llama-server` existant, ou la liste des chemins essayés.
    pub fn locate(&self) -> std::result::Result<ServerCandidate, Vec<PathBuf>> {
        let candidates = self.candidates();
        // Un chemin choisi à la main qui n'existe plus est une erreur, pas un
        // prétexte pour en prendre un autre en silence.
        if let Some(first) = candidates.first().filter(|c| c.source == ServerSource::Settings) {
            return if first.path.is_file() { Ok(first.clone()) } else { Err(vec![first.path.clone()]) };
        }
        match candidates.iter().find(|c| c.path.is_file()) {
            Some(found) => Ok(found.clone()),
            None => Err(candidates.into_iter().map(|c| c.path).collect()),
        }
    }
}

/// Adresse locale d'Ollama, s'il est utilisé à la place du moteur embarqué.
pub const OLLAMA_BASE: &str = "http://127.0.0.1:11434/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub base_url: Url,
    pub api_key: String,
}

/// Clé aléatoire de 256 bits, régénérée à chaque lancement.
pub fn new_api_key() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn plan_launch(model: &ResolvedModel, settings: &RuntimeSettings, port: u16, api_key: String) -> LaunchPlan {
    let mut args: Vec<String> = vec![
        "--model".into(),
        path_arg(&model.path),
        "--alias".into(),
        model.id.clone(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--ctx-size".into(),
        settings.context_size.to_string(),
        "--parallel".into(),
        "1".into(),
        "--jinja".into(),
        "--reasoning-format".into(),
        "deepseek".into(),
        "--offline".into(),
        "--no-webui".into(),
    ];
    if let Some(n) = settings.gpu_layers {
        args.extend(["--n-gpu-layers".into(), n.to_string()]);
    }
    if let Some(n) = settings.cpu_moe_layers.filter(|_| model.moe) {
        args.extend(["--n-cpu-moe".into(), n.to_string()]);
    }
    let base_url = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("valid loopback URL");
    LaunchPlan { args, env: vec![("LLAMA_API_KEY".into(), api_key.clone())], base_url, api_key }
}

fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

/// Attend que `/health` réponde 200 (503 pendant le chargement du modèle).
pub async fn wait_until_ready(client: &LoopbackClient, timeout: Duration, cancel: &Cancel) -> Result<()> {
    let start = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        if let Ok(200) = client.get_status("/health").await {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(EngineError::Runtime("llama-server did not become ready in time".into()));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::llm::catalog::ThinkingControl;

    fn model(moe: bool) -> ResolvedModel {
        ResolvedModel {
            id: "qwen".into(),
            label: "Qwen".into(),
            path: PathBuf::from("/m/q.gguf"),
            thinking: ThinkingControl::ChatTemplateKwargs,
            moe,
        }
    }

    fn search<'a>(
        configured: Option<&'a Path>,
        user: &'a Path,
        bundled: &'a Path,
        path_env: &'a OsStr,
    ) -> ServerSearch<'a> {
        ServerSearch {
            configured,
            user_dir: user,
            bundled_dir: Some(bundled),
            downloaded_dir: None,
            path_env: Some(path_env),
            os: "linux",
        }
    }

    #[test]
    fn server_search_order_and_fallbacks() {
        let tmp = tempfile::tempdir().unwrap();
        let (user, bundled, system) = (tmp.path().join("user"), tmp.path().join("bundled"), tmp.path().join("bin"));
        for dir in [&user, &bundled, &system] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let path_env = std::env::join_paths([system.clone(), PathBuf::from("relatif")]).unwrap();

        // Rien d'installé : l'erreur liste les chemins essayés, dans l'ordre.
        let tried = search(None, &user, &bundled, &path_env).locate().unwrap_err();
        assert_eq!(tried[..3], [user.join("llama-server"), bundled.join("llama-server"), system.join("llama-server")]);
        assert!(!tried.contains(&PathBuf::from("/opt/homebrew/bin/llama-server")), "linux : pas de Homebrew");
        assert!(!tried.iter().any(|p| p.starts_with("relatif")), "un PATH relatif est ignoré");

        // Installation système trouvée, puis supplantée par la build livrée.
        std::fs::write(system.join("llama-server"), "").unwrap();
        assert_eq!(search(None, &user, &bundled, &path_env).locate().unwrap().source, ServerSource::System);
        std::fs::write(bundled.join("llama-server"), "").unwrap();
        assert_eq!(search(None, &user, &bundled, &path_env).locate().unwrap().source, ServerSource::Bundled);

        // Un chemin choisi à la main passe en premier, et n'est jamais remplacé en silence.
        let chosen = tmp.path().join("chosen-llama-server");
        assert_eq!(search(Some(&chosen), &user, &bundled, &path_env).locate().unwrap_err(), vec![chosen.clone()]);
        std::fs::write(&chosen, "").unwrap();
        assert_eq!(search(Some(&chosen), &user, &bundled, &path_env).locate().unwrap().source, ServerSource::Settings);
    }

    #[test]
    fn server_search_knows_homebrew_and_windows_names() {
        let user = Path::new("/u");
        let downloaded = Path::new("/d");
        let base = ServerSearch {
            configured: None,
            user_dir: user,
            bundled_dir: None,
            downloaded_dir: Some(downloaded),
            path_env: None,
            os: "macos",
        };
        let mac = base.candidates();
        assert_eq!(mac[1], ServerCandidate { path: downloaded.join("llama-server"), source: ServerSource::Downloaded });
        assert!(mac.iter().any(|c| c.path == Path::new("/opt/homebrew/bin/llama-server")));
        let win = ServerSearch { os: "windows", ..base };
        assert_eq!(win.candidates()[0].path, user.join("llama-server.exe"));
    }

    #[test]
    fn launch_is_local_offline_and_keyed_by_env() {
        let settings = RuntimeSettings { cpu_moe_layers: Some(10), ..Default::default() };
        let plan = plan_launch(&model(true), &settings, 41234, "k".into());
        let joined = plan.args.join(" ");
        assert!(joined.contains("--host 127.0.0.1 --port 41234"));
        assert!(joined.contains("--offline") && joined.contains("--no-webui"));
        assert!(joined.contains("--n-cpu-moe 10"));
        assert!(!plan.args.iter().any(|a| a == "k"), "la clé n'apparaît pas dans les arguments");
        assert_eq!(plan.env, vec![("LLAMA_API_KEY".to_string(), "k".to_string())]);
        assert_eq!(plan.base_url.as_str(), "http://127.0.0.1:41234/");

        let dense = plan_launch(&model(false), &settings, 1, "k".into());
        assert!(!dense.args.iter().any(|a| a == "--n-cpu-moe"), "réservé aux modèles MoE");
    }

    #[test]
    fn api_keys_are_long_and_unique() {
        let (a, b) = (new_api_key(), new_api_key());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
