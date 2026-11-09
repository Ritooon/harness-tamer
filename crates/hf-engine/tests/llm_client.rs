//! Client du modèle contre un faux `llama-server` sur 127.0.0.1.
//!
//! Le serveur de test utilise `std::net` : c'est la seule exception au lint
//! qui interdit les sockets hors de `hf-net`, limitée à ce fichier de test.
#![allow(clippy::disallowed_types)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use hf_engine::llm::catalog::ThinkingControl;
use hf_engine::llm::client::OpenAiCompatClient;
use hf_engine::llm::{Cancel, ChatBackend, ChatMessage, ChatRequest, StreamEvent};
use hf_net::LoopbackClient;
use url::Url;

/// Accepte une connexion, renvoie (en-têtes, corps) de la requête et répond `response`.
fn serve_once(response: String) -> (Url, mpsc::Receiver<(String, String)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap();
            }
            if line == "\r\n" {
                break;
            }
            headers.push_str(&line);
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        tx.send((headers, String::from_utf8(body).unwrap())).unwrap();
        let mut stream = stream;
        stream.write_all(response.as_bytes()).unwrap();
    });
    (url, rx)
}

fn sse(events: &[&str]) -> String {
    let mut out = String::from("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
    for e in events {
        out.push_str(&format!("data: {e}\n\n"));
    }
    out
}

fn request() -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage::system("Système"), ChatMessage::user("Écris la section.")],
        max_tokens: 64,
        temperature: 0.2,
        thinking: true,
    }
}

#[tokio::test]
async fn streams_reasoning_then_content_with_the_api_key() {
    let (url, rx) = serve_once(sse(&[
        r#"{"choices":[{"delta":{"reasoning_content":"Analyse…"}}]}"#,
        r#"{"choices":[{"delta":{"content":"Bonjour"}}]}"#,
        r#"{"choices":[{"delta":{"content":" !"},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ]));
    let http = LoopbackClient::new(url, Some("secret-key".into())).unwrap();
    let client = OpenAiCompatClient::new(http, "qwen", "Qwen", ThinkingControl::ChatTemplateKwargs);
    let seen = std::sync::Mutex::new(Vec::new());
    let sink = |e: StreamEvent<'_>| seen.lock().unwrap().push(format!("{e:?}"));
    let out = client.chat(request(), &sink, &Cancel::new()).await.unwrap();

    assert_eq!(out.content, "Bonjour !");
    assert_eq!(out.reasoning.as_deref(), Some("Analyse…"));
    assert_eq!(seen.lock().unwrap().len(), 3);
    let (headers, body) = rx.recv().unwrap();
    assert!(headers.starts_with("POST /v1/chat/completions "));
    assert!(headers.to_ascii_lowercase().contains("authorization: bearer secret-key"));
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["model"], "qwen");
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
}

#[tokio::test]
async fn http_errors_and_cancellation_are_reported() {
    let (url, _rx) =
        serve_once("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string());
    let client = OpenAiCompatClient::new(
        LoopbackClient::new(url, Some("wrong".into())).unwrap(),
        "m",
        "M",
        ThinkingControl::Unknown,
    );
    assert!(client.chat(request(), &|_| {}, &Cancel::new()).await.is_err());

    let (url, _rx) = serve_once(sse(&[r#"{"choices":[{"delta":{"content":"x"}}]}"#]));
    let client = OpenAiCompatClient::new(LoopbackClient::new(url, None).unwrap(), "m", "M", ThinkingControl::Unknown);
    let cancel = Cancel::new();
    cancel.cancel();
    let err = client.chat(request(), &|_| {}, &cancel).await.unwrap_err();
    assert_eq!(err.code(), "cancelled");
}
