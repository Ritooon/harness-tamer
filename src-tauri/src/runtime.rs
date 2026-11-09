//! Cycle de vie du moteur d'inférence : `llama-server` livré avec
//! l'application (ou déposé par l'utilisateur dans `runtimes/llama/`), ou
//! Ollama déjà lancé sur la machine.
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
use hf_engine::llm::runtime::{OLLAMA_BASE, SERVER_BINARY, new_api_key, plan_launch, wait_until_ready};
use hf_engine::llm::{Cancel, ChatBackend};
use hf_engine::model::{RuntimeBackend, Settings};
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
    Failed {
        message: String,
    },
}

/// Configuration qui, si elle change, impose de relancer le moteur.
#[derive(Debug, Clone, PartialEq)]
struct RunningKey {
    backend: RuntimeBackend,
    model: String,
    context_size: u32,
    gpu_layers: Option<u32>,
    cpu_moe_layers: Option<u32>,
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

    /// Moteur prêt pour les réglages courants ; le (re)lance si besoin.
    pub async fn backend<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        store: &Store,
        settings: &Settings,
    ) -> CmdResult<Arc<dyn ChatBackend>> {
        let key = RunningKey::from(settings).ok_or_else(|| CmdError::from(hf_engine::EngineError::ModelNotReady))?;
        let mut running = self.running.lock().await;
        if running.key.as_ref() == Some(&key)
            && let Some(backend) = &running.backend
        {
            return Ok(Arc::clone(backend));
        }
        stop_locked(&mut running);
        self.set_status(RuntimeStatus::Starting { model: key.model.clone() });
        let result = match key.backend {
            RuntimeBackend::Embedded => self.start_embedded(app, store, settings, &mut running).await,
            RuntimeBackend::Ollama => start_ollama(&key.model).await,
        };
        match result {
            Ok(backend) => {
                running.backend = Some(Arc::clone(&backend));
                running.key = Some(key.clone());
                self.set_status(RuntimeStatus::Ready { model: key.model, backend: key.backend });
                Ok(backend)
            }
            Err(err) => {
                stop_locked(&mut running);
                self.set_status(RuntimeStatus::Failed { message: err.message.clone() });
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
        let server = server_path(app, store).ok_or_else(|| {
            CmdError::new("runtime.missing", "llama-server was not found in the application resources")
        })?;
        let port = hf_net::free_loopback_port()?;
        let plan = plan_launch(&model, &settings.runtime, port, new_api_key());

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
                            *s = RuntimeStatus::Failed { message: message.clone() };
                        }
                        message
                    }
                    _ => continue,
                };
                let mut log = log.lock().unwrap_or_else(|p| p.into_inner());
                if log.len() >= LOG_LINES {
                    log.pop_front();
                }
                log.push_back(line);
            }
        });

        let http = LoopbackClient::new(plan.base_url.clone(), Some(plan.api_key.clone()))?;
        wait_until_ready(&http, READY_TIMEOUT, &exited).await.map_err(|e| match e {
            hf_engine::EngineError::Cancelled => {
                let tail = self.log_tail();
                let last = tail.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join("\n");
                CmdError::new("runtime.exited", format!("llama-server stopped during startup:\n{last}"))
            }
            other => CmdError::from(other),
        })?;
        Ok(Arc::new(OpenAiCompatClient::new(http, model.id.clone(), model.label.clone(), model.thinking.clone())))
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

/// `runtimes/llama/llama-server` (build fourni par l'utilisateur, CUDA par
/// exemple) sinon le binaire livré dans les ressources de l'application.
fn server_path<R: Runtime>(app: &AppHandle<R>, store: &Store) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{SERVER_BINARY}.exe") } else { SERVER_BINARY.to_owned() };
    let custom = store.runtimes_dir().join("llama").join(&exe);
    if custom.is_file() {
        return Some(custom);
    }
    let bundled = app.path().resource_dir().ok()?.join("llama").join(&exe);
    bundled.is_file().then_some(bundled)
}
