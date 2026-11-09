//! Politique réseau choisie par l'utilisateur dans les paramètres.

use serde::{Deserialize, Serialize};

/// Politique d'accès à Internet.
///
/// Il n'existe volontairement pas de mode « toujours autoriser » : la
/// spécification exige une validation à chaque accès.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Chaque action réseau demandée par l'utilisateur ouvre un dialogue natif.
    #[default]
    Ask,
    /// Tout accès à Internet est refusé, sans dialogue.
    Off,
}
