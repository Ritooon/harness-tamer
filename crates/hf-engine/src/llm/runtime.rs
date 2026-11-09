//! Lancement de `llama-server` : arguments, environnement, attente du
//! démarrage. Le processus lui-même est lancé par l'application Tauri
//! (sidecar) ; ce module ne fait que préparer et vérifier.
//!
//! Verrou n°5 de l'ADR 0002 : écoute sur 127.0.0.1 uniquement, port
//! aléatoire, clé d'API éphémère passée par variable d'environnement (elle
//! n'apparaît donc pas dans la liste des processus), `--offline` (aucun
//! téléchargement possible) et `--no-webui`.

use std::path::Path;
use std::time::{Duration, Instant};

use hf_net::LoopbackClient;
use url::Url;

use super::Cancel;
use super::catalog::ResolvedModel;
use crate::error::{EngineError, Result};
use crate::model::RuntimeSettings;

/// Nom du binaire embarqué (sidecar Tauri, suffixé par la cible à la compilation).
pub const SERVER_BINARY: &str = "llama-server";

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
