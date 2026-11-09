//! Client de l'API compatible OpenAI exposée par `llama-server` (ou Ollama),
//! sur la boucle locale uniquement (`hf_net::LoopbackClient` refuse tout
//! autre hôte). Les réponses sont lues en flux (Server-Sent Events).

use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use hf_net::LoopbackClient;
use serde_json::{Value, json};

use super::catalog::ThinkingControl;
use super::{Cancel, ChatBackend, ChatOutput, ChatRequest, EventSink, Role, StreamEvent, split_inline_thinking};
use crate::error::{EngineError, Result};

/// Délai maximal avant le premier fragment (lecture du prompt sur CPU).
const FIRST_CHUNK_TIMEOUT: Duration = Duration::from_secs(900);
/// Délai maximal entre deux fragments.
const NEXT_CHUNK_TIMEOUT: Duration = Duration::from_secs(180);

pub struct OpenAiCompatClient {
    http: LoopbackClient,
    /// Valeur du champ `model` (alias côté `llama-server`, nom côté Ollama).
    model: String,
    label: String,
    thinking: ThinkingControl,
}

impl OpenAiCompatClient {
    pub fn new(
        http: LoopbackClient,
        model: impl Into<String>,
        label: impl Into<String>,
        thinking: ThinkingControl,
    ) -> Self {
        Self { http, model: model.into(), label: label.into(), thinking }
    }
}

/// Corps de la requête `/v1/chat/completions`, selon le mode réflexion du modèle.
pub fn request_body(model: &str, request: &ChatRequest, thinking: &ThinkingControl) -> Value {
    let mut messages: Vec<Value> = request
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({ "role": role, "content": m.content })
        })
        .collect();
    let mut body = json!({
        "model": model,
        "stream": true,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
    });
    match thinking {
        ThinkingControl::ChatTemplateKwargs => {
            body["chat_template_kwargs"] = json!({ "enable_thinking": request.thinking });
        }
        ThinkingControl::SystemToken { token } if request.thinking => {
            match messages.iter_mut().find(|m| m["role"] == "system") {
                Some(system) => {
                    let content = system["content"].as_str().unwrap_or("").to_owned();
                    system["content"] = Value::String(format!("{token}\n{content}"));
                }
                None => messages.insert(0, json!({ "role": "system", "content": token })),
            }
        }
        _ => {}
    }
    body["messages"] = Value::Array(messages);
    body
}

/// Découpe un flux SSE en charges utiles `data:`.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some((end, sep)) = find_event_end(&self.buffer) {
            let event: Vec<u8> = self.buffer.drain(..end + sep).take(end).collect();
            let text = String::from_utf8_lossy(&event);
            let data: Vec<&str> = text
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            if !data.is_empty() {
                out.push(data.join("\n"));
            }
        }
        out
    }
}

fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|p| (p, 2));
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| (p, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// Accumulateur des fragments d'une réponse.
#[derive(Debug, Default)]
pub struct Accumulator {
    pub content: String,
    pub reasoning: String,
    pub finish_reason: Option<String>,
    pub completion_tokens: Option<u32>,
}

impl Accumulator {
    /// Intègre une charge utile JSON ; renvoie `true` à la fin du flux.
    pub fn apply(&mut self, payload: &str, on_event: EventSink<'_>) -> Result<bool> {
        if payload.trim() == "[DONE]" {
            return Ok(true);
        }
        let value: Value =
            serde_json::from_str(payload).map_err(|e| EngineError::Runtime(format!("bad stream chunk: {e}")))?;
        if let Some(error) = value.get("error") {
            let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
            return Err(EngineError::Runtime(message.to_owned()));
        }
        if let Some(n) = value.pointer("/usage/completion_tokens").and_then(Value::as_u64) {
            self.completion_tokens = Some(n as u32);
        }
        if let Some(choice) = value.pointer("/choices/0") {
            let delta = &choice["delta"];
            // `reasoning_content` (llama.cpp) ou `reasoning` (Ollama).
            for key in ["reasoning_content", "reasoning"] {
                if let Some(r) = delta.get(key).and_then(Value::as_str).filter(|r| !r.is_empty()) {
                    self.reasoning.push_str(r);
                    on_event(StreamEvent::Reasoning(r));
                }
            }
            if let Some(c) = delta.get("content").and_then(Value::as_str).filter(|c| !c.is_empty()) {
                self.content.push_str(c);
                on_event(StreamEvent::Content(c));
            }
            if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
                self.finish_reason = Some(f.to_owned());
            }
        }
        Ok(false)
    }

    pub fn finish(self) -> ChatOutput {
        let (content, inline) = split_inline_thinking(&self.content);
        let reasoning = if self.reasoning.trim().is_empty() { inline } else { Some(self.reasoning) };
        ChatOutput {
            content: content.trim().to_owned(),
            reasoning,
            finish_reason: self.finish_reason,
            completion_tokens: self.completion_tokens,
        }
    }
}

impl ChatBackend for OpenAiCompatClient {
    fn model_label(&self) -> String {
        self.label.clone()
    }

    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_event: EventSink<'a>,
        cancel: &'a Cancel,
    ) -> BoxFuture<'a, Result<ChatOutput>> {
        Box::pin(async move {
            let body = request_body(&self.model, &request, &self.thinking);
            let mut stream = self.http.post_stream("/v1/chat/completions", &body).await?;
            let mut decoder = SseDecoder::default();
            let mut acc = Accumulator::default();
            let mut first = true;
            loop {
                if cancel.is_cancelled() {
                    // Abandonner le flux ferme la connexion : llama-server arrête de générer.
                    return Err(EngineError::Cancelled);
                }
                let wait = if first { FIRST_CHUNK_TIMEOUT } else { NEXT_CHUNK_TIMEOUT };
                let next = tokio::time::timeout(wait, stream.next())
                    .await
                    .map_err(|_| EngineError::Runtime("the model stopped responding".into()))?;
                first = false;
                let Some(chunk) = next else { break };
                for payload in decoder.push(&chunk?) {
                    if acc.apply(&payload, on_event)? {
                        return Ok(acc.finish());
                    }
                }
            }
            Ok(acc.finish())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::llm::ChatMessage;

    fn req(thinking: bool) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::system("Tu es précis."), ChatMessage::user("Bonjour")],
            max_tokens: 100,
            temperature: 0.3,
            thinking,
        }
    }

    #[test]
    fn qwen_uses_chat_template_kwargs() {
        let body = request_body("m", &req(false), &ThinkingControl::ChatTemplateKwargs);
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(body["messages"][0]["content"], "Tu es précis.");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn gemma_token_is_prepended_only_when_thinking() {
        let control = ThinkingControl::SystemToken { token: "<|think|>".into() };
        let on = request_body("m", &req(true), &control);
        assert_eq!(on["messages"][0]["content"], "<|think|>\nTu es précis.");
        let off = request_body("m", &req(false), &control);
        assert_eq!(off["messages"][0]["content"], "Tu es précis.");
        assert!(off.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn sse_events_split_across_chunks() {
        let mut d = SseDecoder::default();
        assert!(d.push(b"data: {\"a\":").is_empty());
        assert_eq!(d.push(b"1}\n\ndata: [DONE]\r\n\r\n: comment\n\n"), vec!["{\"a\":1}", "[DONE]"]);
    }

    #[test]
    fn accumulator_separates_reasoning_and_content() {
        let events = Mutex::new(Vec::new());
        let sink = |e: StreamEvent<'_>| {
            events.lock().unwrap().push(match e {
                StreamEvent::Reasoning(r) => format!("R:{r}"),
                StreamEvent::Content(c) => format!("C:{c}"),
            })
        };
        let mut acc = Accumulator::default();
        for p in [
            r#"{"choices":[{"delta":{"reasoning_content":"je pense"}}]}"#,
            r###"{"choices":[{"delta":{"content":"## Titre"}}]}"###,
            r#"{"choices":[{"delta":{"content":"\nTexte"},"finish_reason":"stop"}],"usage":{"completion_tokens":7}}"#,
        ] {
            assert!(!acc.apply(p, &sink).unwrap());
        }
        assert!(acc.apply("[DONE]", &sink).unwrap());
        let out = acc.finish();
        assert_eq!(out.content, "## Titre\nTexte");
        assert_eq!(out.reasoning.as_deref(), Some("je pense"));
        assert_eq!(out.completion_tokens, Some(7));
        assert!(!out.truncated());
        assert_eq!(events.lock().unwrap().len(), 3);
    }

    #[test]
    fn stream_errors_are_reported() {
        let mut acc = Accumulator::default();
        let err = acc.apply(r#"{"error":{"message":"context full"}}"#, &|_| {}).unwrap_err();
        assert!(matches!(err, EngineError::Runtime(m) if m == "context full"));
    }
}
