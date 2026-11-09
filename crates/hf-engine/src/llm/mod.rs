//! Dialogue avec le modèle local.
//!
//! - [`catalog`] : modèles proposés et modèles installés ;
//! - [`hardware`] : description de la machine et recommandation ;
//! - [`runtime`] : arguments de lancement de `llama-server` et attente du démarrage ;
//! - [`client`] : client de l'API compatible OpenAI (`llama-server` ou Ollama),
//!   uniquement sur la boucle locale, en flux ;
//! - [`scripted`] : moteur factice et déterministe pour les tests.

pub mod catalog;
pub mod client;
pub mod hardware;
pub mod runtime;
pub mod scripted;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Active le mode réflexion si le modèle le permet.
    pub thinking: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatOutput {
    pub content: String,
    pub reasoning: Option<String>,
    pub finish_reason: Option<String>,
    pub completion_tokens: Option<u32>,
}

impl ChatOutput {
    /// Vrai si la réponse a été coupée par la limite de tokens.
    pub fn truncated(&self) -> bool {
        self.finish_reason.as_deref() == Some("length")
    }
}

/// Fragment reçu pendant la génération, relayé à l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEvent<'a> {
    Reasoning(&'a str),
    Content(&'a str),
}

pub type EventSink<'a> = &'a (dyn Fn(StreamEvent<'_>) + Send + Sync);

/// Jeton d'annulation partagé entre l'interface et une génération en cours.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Un moteur de chat : `llama-server`, Ollama, ou le moteur factice des tests.
pub trait ChatBackend: Send + Sync {
    /// Nom du modèle, enregistré dans l'historique des versions.
    fn model_label(&self) -> String;

    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_event: EventSink<'a>,
        cancel: &'a Cancel,
    ) -> BoxFuture<'a, Result<ChatOutput>>;
}

/// Sépare un éventuel bloc `<think>…</think>` laissé dans le contenu (moteurs
/// qui ne séparent pas la réflexion) : `(contenu, réflexion)`.
pub fn split_inline_thinking(content: &str) -> (String, Option<String>) {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("<think>") {
        if let Some((thinking, answer)) = rest.split_once("</think>") {
            let thinking = thinking.trim();
            return (answer.trim_start().to_owned(), (!thinking.is_empty()).then(|| thinking.to_owned()));
        }
        // Réflexion jamais refermée : la réponse est vide.
        return (String::new(), Some(rest.trim().to_owned()));
    }
    (content.to_owned(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_thinking_is_split() {
        assert_eq!(split_inline_thinking("<think>a\nb</think>\n\n## Titre"), ("## Titre".into(), Some("a\nb".into())));
        assert_eq!(split_inline_thinking("<think></think>Texte"), ("Texte".into(), None));
        assert_eq!(split_inline_thinking("Texte <think>x</think>"), ("Texte <think>x</think>".into(), None));
        assert_eq!(split_inline_thinking("<think>sans fin"), (String::new(), Some("sans fin".into())));
    }

    #[test]
    fn cancel_is_shared_between_clones() {
        let c = Cancel::new();
        let d = c.clone();
        d.cancel();
        assert!(c.is_cancelled());
    }
}
