//! Moteur factice : répond par une fonction Rust, sans modèle ni réseau.
//! Sert aux tests de la génération et des skills.

use std::sync::Mutex;

use futures_util::future::BoxFuture;

use super::{Cancel, ChatBackend, ChatOutput, ChatRequest, EventSink, StreamEvent};
use crate::error::{EngineError, Result};

type Responder = dyn Fn(&ChatRequest) -> Result<String> + Send + Sync;

pub struct ScriptedBackend {
    responder: Box<Responder>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedBackend {
    pub fn new(responder: impl Fn(&ChatRequest) -> Result<String> + Send + Sync + 'static) -> Self {
        Self { responder: Box::new(responder), requests: Mutex::new(Vec::new()) }
    }

    /// Requêtes reçues, dans l'ordre.
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("lock").clone()
    }
}

impl ChatBackend for ScriptedBackend {
    fn model_label(&self) -> String {
        "scripted".into()
    }

    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_event: EventSink<'a>,
        cancel: &'a Cancel,
    ) -> BoxFuture<'a, Result<ChatOutput>> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            let answer = (self.responder)(&request)?;
            self.requests.lock().expect("lock").push(request);
            for word in answer.split_inclusive(' ') {
                on_event(StreamEvent::Content(word));
            }
            Ok(ChatOutput {
                content: answer.trim().to_owned(),
                finish_reason: Some("stop".into()),
                ..Default::default()
            })
        })
    }
}
