//! État partagé par les commandes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use hf_engine::llm::Cancel;
use hf_engine::llm::catalog::{Catalog, ModelLibrary};
use hf_engine::model::Settings;
use hf_engine::store::Store;
use hf_net::GatedClient;

use crate::consent::SharedLang;
use crate::error::CmdResult;
use crate::runtime::RuntimeManager;

pub struct AppState {
    pub store: Store,
    pub gated: Arc<GatedClient>,
    pub lang: SharedLang,
    settings: RwLock<Settings>,
    pub runtime: RuntimeManager,
    /// Opérations longues annulables (génération, création de skill…), par identifiant.
    pub cancels: Mutex<HashMap<String, Cancel>>,
    /// Téléchargements de modèles en cours, par identifiant de modèle.
    pub downloads: Mutex<HashMap<String, tokio::task::AbortHandle>>,
}

impl AppState {
    pub fn new(store: Store, gated: Arc<GatedClient>, lang: SharedLang, settings: Settings) -> Self {
        Self {
            store,
            gated,
            lang,
            settings: RwLock::new(settings),
            runtime: RuntimeManager::default(),
            cancels: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Enregistre les réglages et applique ceux qui ont un effet immédiat.
    pub fn save_settings(&self, settings: Settings) -> CmdResult<Settings> {
        self.store.save_settings(&settings)?;
        self.gated.set_policy(settings.network_policy.into());
        *self.lang.write().unwrap_or_else(|p| p.into_inner()) = settings.ui_language;
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = settings.clone();
        Ok(settings)
    }

    pub fn library(&self) -> ModelLibrary {
        ModelLibrary::new(self.store.models_dir(&self.settings()), Catalog::builtin())
    }

    /// Jeton d'annulation d'une opération longue, créé à son lancement.
    pub fn begin_cancellable(&self, op: &str) -> Cancel {
        let cancel = Cancel::new();
        self.cancels.lock().unwrap_or_else(|p| p.into_inner()).insert(op.to_owned(), cancel.clone());
        cancel
    }

    pub fn end_cancellable(&self, op: &str) {
        self.cancels.lock().unwrap_or_else(|p| p.into_inner()).remove(op);
    }

    pub fn cancel(&self, op: &str) {
        if let Some(c) = self.cancels.lock().unwrap_or_else(|p| p.into_inner()).get(op) {
            c.cancel();
        }
    }
}
