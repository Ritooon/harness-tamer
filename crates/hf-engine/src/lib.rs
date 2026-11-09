//! # hf-engine
//!
//! Cœur de HarnessForge, indépendant de l'interface : modèle de données,
//! stockage local, analyse de dépôt, charte graphique, dialogue avec le
//! modèle local, génération de l'AGENTS.md, skills et export.
//!
//! Ce crate n'ouvre **aucune** connexion lui-même : tout accès réseau passe
//! par `hf-net` (Internet sous consentement, ou boucle locale vers
//! `llama-server`). Le script `scripts/check-network-deps.mjs` le vérifie.

pub mod brand;
pub mod error;
pub mod export;
pub mod generation;
pub mod index;
pub mod llm;
pub mod model;
pub mod scan;
pub mod skills;
pub mod store;
pub mod text;

pub use error::{EngineError, Result};
