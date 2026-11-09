//! # hf-net
//!
//! Le **seul** crate de HarnessForge autorisé à ouvrir une connexion réseau
//! (ADR 0002). Tout le reste du code passe par l'un de ses deux clients :
//!
//! - [`LoopbackClient`] : parle uniquement à `127.0.0.1` / `::1`, c'est-à-dire au
//!   moteur LLM local (`llama-server`). Aucun consentement n'est requis, car rien
//!   ne quitte la machine ; toute autre adresse est refusée.
//! - [`GatedClient`] : accès à Internet. Chaque action de l'utilisateur ouvre un
//!   dialogue natif ([`ConsentPrompt`]) qui liste les hôtes contactés. Sans
//!   « Autoriser », aucune requête ne part. Chaque décision et chaque requête est
//!   inscrite dans le [`Journal`].
//!
//! La politique [`NetworkPolicy::Off`] coupe tout accès Internet sans même
//! afficher de dialogue.

// Ce crate est l'unique exception aux interdictions de `clippy.toml` :
// c'est précisément son rôle d'ouvrir des sockets.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

pub mod consent;
pub mod error;
pub mod gated;
pub mod journal;
pub mod loopback;
pub mod policy;

pub use consent::{
    ActionKind, ConsentPrompt, ConsentText, Decision, NetAction, host_allowed, human_size, render_consent,
};
pub use error::NetError;
pub use gated::{DownloadReport, GatedClient, GatedConfig, NetSession, sha256_file};
pub use journal::{Journal, JournalDecision, JournalEntry, JournalEvent};
pub use loopback::{LoopbackClient, free_loopback_port};
pub use policy::NetworkPolicy;

/// Encode des octets en hexadécimal minuscule (empreintes SHA-256).
pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_encoding() {
        assert_eq!(super::to_hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
