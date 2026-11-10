//! Cycle de vie du moteur d'inférence : `llama-server` (livré avec
//! l'application, installé par elle sur demande, déposé par l'utilisateur ou
//! déjà présent sur le système), ou Ollama déjà lancé sur la machine.
//!
//! Chaque tentative de démarrage est notée dans le journal du moteur, échec
//! compris, pour que l'écran des réglages puisse toujours dire ce qui s'est
//! passé.
//!
//! `llama-server` est lancé depuis Rust uniquement (le plugin shell n'est
//! accessible à aucune permission de l'interface), sur 127.0.0.1, avec une
//! clé éphémère et `--offline` (voir `hf_engine::llm::runtime`).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use hf_engine::llm::catalog::{Catalog, ModelLibrary, ThinkingControl};
use hf_engine::llm::client::OpenAiCompatClient;
use hf_engine::llm::runtime::{OLLAMA_BASE, ServerCandidate, ServerSearch, new_api_key, plan_launch, wait_until_ready};
use hf_engine::llm::{Cancel, ChatBackend};
use hf_engine::model::{Lang, RuntimeBackend, Settings};
use hf_engine::store::Store;
use hf_net::LoopbackClient;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

use crate::error::{CmdError, CmdResult};

/// Chargement d'un gros modèle depuis un disque lent : jusqu'à 10 minutes.
const READY_TIMEOUT: Duration = Duration::from_secs(600);
const LOG_LINES: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RuntimeStatus {
    #[default]
    Stopped,
    Starting {
        model: String,
    },
    Ready {
        model: String,
        backend: RuntimeBackend,
    },
    /// `code` est le code stable de l'erreur, que l'interface traduit.
    Failed {
        code: String,
        message: String,
    },
}

/// Où se trouve `llama-server`, pour l'écran des réglages.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeLocation {
    pub found: Option<ServerCandidate>,
    /// Chemins essayés, dans l'ordre, quand rien n'a été trouvé.
    pub searched: Vec<String>,
    /// Cible llama.cpp de cette machine, si une installation en un clic est possible.
    pub install_target: Option<String>,
}

/// Configuration qui, si elle change, impose de relancer le moteur.
#[derive(Debug, Clone, PartialEq)]
struct RunningKey {
    backend: RuntimeBackend,
    model: String,
    context_size: u32,
    gpu_layers: Option<u32>,
    cpu_moe_layers: Option<u32>,
    server_path: Option<String>,
}

impl RunningKey {
    fn from(settings: &Settings) -> Option<Self> {
        let model = match settings.runtime.backend {
            RuntimeBackend::Embedded => settings.active_model.clone()?,
            RuntimeBackend::Ollama => settings.runtime.ollama_model.clone()?,
        };
        Some(Self {
            backend: settings.runtime.backend,
            model,
            context_size: settings.runtime.context_size,
            gpu_layers: settings.runtime.gpu_layers,
            cpu_moe_layers: settings.runtime.cpu_moe_layers,
            server_path: settings.runtime.server_path.clone(),
        })
    }
}

#[derive(Default)]
struct Running {
    key: Option<RunningKey>,
    child: Option<CommandChild>,
    backend: Option<Arc<dyn ChatBackend>>,
}

#[derive(Default)]
pub struct RuntimeManager {
    running: tokio::sync::Mutex<Running>,
    status: Arc<RwLock<RuntimeStatus>>,
    log: Arc<Mutex<VecDeque<String>>>,
}

impl RuntimeManager {
    pub fn status(&self) -> RuntimeStatus {
        self.status.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set_status(&self, status: RuntimeStatus) {
        *self.status.write().unwrap_or_else(|p| p.into_inner()) = status;
    }

    /// Dernières lignes écrites par `llama-server` (diagnostic local).
    pub fn log_tail(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect()
    }

    /// Ajoute une ligne de l'application au journal du moteur.
    fn note(&self, line: impl Into<String>) {
        push_log(&self.log, format!("[HarnessForge] {}", line.into()));
    }

    /// Moteur prêt pour les réglages courants ; le (re)lance si besoin.
    pub async fn backend<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        store: &Store,
        settings: &Settings,
    ) -> CmdResult<Arc<dyn ChatBackend>> {
        let fr = settings.ui_language == Lang::Fr;
        let Some(key) = RunningKey::from(settings) else {
            let err = CmdError::from(hf_engine::EngineError::ModelNotReady);
            self.note(if fr {
                "Démarrage impossible : aucun modèle n'est choisi dans les réglages."
            } else {
                "Cannot start: no model is selected in the settings."
            });
            self.set_status(RuntimeStatus::Failed { code: err.code.clone(), message: err.message.clone() });
            return Err(err);
        };
        let mut running = self.running.lock().await;
        if running.key.as_ref() == Some(&key)
            && let Some(backend) = &running.backend
        {
            return Ok(Arc::clone(backend));
        }
        stop_locked(&mut running);
        self.set_status(RuntimeStatus::Starting { model: key.model.clone() });
        self.note(match (fr, key.backend) {
            (true, RuntimeBackend::Embedded) => format!("Démarrage de llama.cpp avec le modèle {}.", key.model),
            (false, RuntimeBackend::Embedded) => format!("Starting llama.cpp with model {}.", key.model),
            (true, RuntimeBackend::Ollama) => format!("Connexion à Ollama (modèle {}).", key.model),
            (false, RuntimeBackend::Ollama) => format!("Connecting to Ollama (model {}).", key.model),
        });
        let result = match key.backend {
            RuntimeBackend::Embedded => self.start_embedded(app, store, settings, &mut running).await,
            RuntimeBackend::Ollama => start_ollama(&key.model).await,
        };
        match result {
            Ok(backend) => {
                running.backend = Some(Arc::clone(&backend));
                running.key = Some(key.clone());
                self.note(if fr { "Moteur prêt." } else { "Engine ready." });
                self.set_status(RuntimeStatus::Ready { model: key.model, backend: key.backend });
                Ok(backend)
            }
            Err(err) => {
                stop_locked(&mut running);
                self.note(match (err.code.as_str(), fr) {
                    ("runtime.missing", true) => {
                        format!("Échec : llama-server est introuvable. Emplacements consultés :\n{}", err.message)
                    }
                    ("runtime.missing", false) => {
                        format!("Failed: llama-server was not found. Locations checked:\n{}", err.message)
                    }
                    (code, true) => format!("Échec [{code}] : {}", err.message),
                    (code, false) => format!("Failed [{code}]: {}", err.message),
                });
                self.set_status(RuntimeStatus::Failed { code: err.code.clone(), message: err.message.clone() });
                Err(err)
            }
        }
    }

    async fn start_embedded<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        store: &Store,
        settings: &Settings,
        running: &mut Running,
    ) -> CmdResult<Arc<dyn ChatBackend>> {
        let model_id = settings.active_model.as_deref().ok_or(hf_engine::EngineError::ModelNotReady)?;
        let library = ModelLibrary::new(store.models_dir(settings), Catalog::builtin());
        let model = library.resolve(model_id)?;
        let server = match locate_server(app, store, settings) {
            Ok(found) => found.path,
            // Le message porte les chemins essayés : l'interface les affiche.
            Err(searched) => return Err(CmdError::new("runtime.missing", searched.join("\n"))),
        };
        let port = hf_net::free_loopback_port()?;
        let plan = plan_launch(&model, &settings.runtime, port, new_api_key());
        // La clé d'API passe par l'environnement : elle n'apparaît pas ici.
        self.note(format!("{} {}", server.display(), plan.args.join(" ")));

        let dir = server.parent().map(PathBuf::from).unwrap_or_default();
        let mut command = app.shell().command(&server).args(&plan.args).envs(plan.env.clone()).current_dir(&dir);
        // Bibliothèques partagées de llama.cpp, livrées à côté du binaire.
        if cfg!(target_os = "linux") {
            command = command.env("LD_LIBRARY_PATH", &dir);
        } else if cfg!(target_os = "macos") {
            command = command.env("DYLD_LIBRARY_PATH", &dir);
        }
        let (mut events, child) =
            command.kill_on_drop(true).spawn().map_err(|e| CmdError::new("runtime.spawn", e.to_string()))?;
        running.child = Some(child);

        // Le processus s'arrête avant d'être prêt : on cesse d'attendre.
        let exited = Cancel::new();
        let log = Arc::clone(&self.log);
        let status = Arc::clone(&self.status);
        let exited_signal = exited.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                let line = match event {
                    CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                        String::from_utf8_lossy(&bytes).trim_end().to_owned()
                    }
                    CommandEvent::Error(e) => e,
                    CommandEvent::Terminated(payload) => {
                        exited_signal.cancel();
                        let message = format!("llama-server exited (code {:?})", payload.code);
                        let mut s = status.write().unwrap_or_else(|p| p.into_inner());
                        if !matches!(*s, RuntimeStatus::Stopped) {
                            *s = RuntimeStatus::Failed { code: "runtime.exited".into(), message: message.clone() };
                        }
                        message
                    }
                    _ => continue,
                };
                push_log(&log, line);
            }
        });

        let http = LoopbackClient::new(plan.base_url.clone(), Some(plan.api_key.clone()))?;
        wait_until_ready(&http, READY_TIMEOUT, &exited).await.map_err(|e| match e {
            hf_engine::EngineError::Cancelled => {
                // Les dernières lignes de llama-server disent presque toujours pourquoi.
                let tail = self.log_tail();
                let own: Vec<&String> = tail.iter().filter(|l| !l.starts_with("[HarnessForge]")).collect();
                let last = own[own.len().saturating_sub(6)..].iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n");
                CmdError::new("runtime.exited", last)
            }
            other => CmdError::from(other),
        })?;
        Ok(Arc::new(OpenAiCompatClient::new(http, model.id.clone(), model.label.clone(), model.thinking.clone())))
    }

    /// Efface un état d'échec devenu sans objet (moteur installé entre-temps).
    pub fn clear_failure(&self) {
        let mut status = self.status.write().unwrap_or_else(|p| p.into_inner());
        if matches!(*status, RuntimeStatus::Failed { .. }) {
            *status = RuntimeStatus::Stopped;
        }
    }

    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        stop_locked(&mut running);
        self.set_status(RuntimeStatus::Stopped);
    }

    /// Arrêt synchrone, à la fermeture de l'application.
    pub fn stop_now(&self) {
        if let Ok(mut running) = self.running.try_lock() {
            stop_locked(&mut running);
        }
        self.set_status(RuntimeStatus::Stopped);
    }
}

fn stop_locked(running: &mut Running) {
    if let Some(child) = running.child.take() {
        let _ = child.kill();
    }
    running.backend = None;
    running.key = None;
}

async fn start_ollama(model: &str) -> CmdResult<Arc<dyn ChatBackend>> {
    let base = url::Url::parse(OLLAMA_BASE).map_err(|e| CmdError::new("runtime.ollama", e.to_string()))?;
    let http = LoopbackClient::new(base, None)?;
    match http.get_status("/api/version").await {
        Ok(200) => {}
        _ => return Err(CmdError::new("runtime.ollama", "Ollama is not running on 127.0.0.1:11434")),
    }
    Ok(Arc::new(OpenAiCompatClient::new(http, model, format!("Ollama · {model}"), ThinkingControl::Unknown)))
}

/// Ajoute une entrée au journal, ligne par ligne.
fn push_log(log: &Mutex<VecDeque<String>>, line: String) {
    let mut log = log.lock().unwrap_or_else(|p| p.into_inner());
    for part in line.lines() {
        if log.len() >= LOG_LINES {
            log.pop_front();
        }
        log.push_back(part.to_owned());
    }
}

/// Dossier géré par l'installation en un clic.
pub fn downloaded_dir(store: &Store) -> PathBuf {
    store.runtimes_dir().join("llama-cpp")
}

/// Cherche `llama-server` : réglage, `runtimes/llama/` (build de
/// l'utilisateur), ressources de l'application, installation en un clic, puis
/// système (Homebrew, PATH). Rend les chemins essayés si rien n'existe.
pub fn locate_server<R: Runtime>(
    app: &AppHandle<R>,
    store: &Store,
    settings: &Settings,
) -> Result<ServerCandidate, Vec<String>> {
    let user_dir = store.runtimes_dir().join("llama");
    let bundled = app.path().resource_dir().ok().map(|d| d.join("llama"));
    let downloaded = downloaded_dir(store);
    let path_env = std::env::var_os("PATH");
    let search = ServerSearch {
        configured: settings.runtime.server_path.as_deref().map(std::path::Path::new),
        user_dir: &user_dir,
        bundled_dir: bundled.as_deref(),
        downloaded_dir: Some(&downloaded),
        path_env: path_env.as_deref(),
        os: std::env::consts::OS,
    };
    search.locate().map_err(|tried| tried.iter().map(|p| p.display().to_string()).collect())
}

pub fn location<R: Runtime>(app: &AppHandle<R>, store: &Store, settings: &Settings) -> RuntimeLocation {
    let install_target = hf_engine::llm::install::platform_key(std::env::consts::OS, std::env::consts::ARCH);
    match locate_server(app, store, settings) {
        Ok(found) => RuntimeLocation { found: Some(found), searched: Vec::new(), install_target },
        Err(searched) => RuntimeLocation { found: None, searched, install_target },
    }
}
