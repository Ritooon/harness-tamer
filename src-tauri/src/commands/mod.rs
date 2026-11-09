//! Commandes appelées par l'interface (IPC Tauri). Chaque commande renvoie
//! une erreur `{ code, message }` que l'interface traduit.

pub mod agents;
pub mod projects;
pub mod settings;
pub mod skills;
