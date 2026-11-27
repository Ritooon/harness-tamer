//! Tests de bout en bout du client Internet soumis au consentement, contre un
//! petit serveur HTTP local écrit pour l'occasion.

#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hf_net::{
    ActionKind, ConsentPrompt, Decision, GatedClient, GatedConfig, Journal, JournalDecision, JournalEvent, NetAction,
    NetError, NetworkPolicy,
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// Requête HTTP réduite à ce dont les tests ont besoin.
struct Req {
    path: String,
    headers: HashMap<String, String>,
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Resp {
    fn ok(body: &[u8]) -> Self {
        Self { status: 200, headers: vec![], body: body.to_vec() }
    }
}

type Handler = Arc<dyn Fn(&Req) -> Resp + Send + Sync>;

/// Démarre un serveur HTTP/1.1 minimal ; rend son adresse et un compteur de requêtes.
async fn serve(handler: Handler) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { break };
            let handler = Arc::clone(&handler);
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                counter.fetch_add(1, Ordering::SeqCst);
                let text = String::from_utf8_lossy(&raw).to_string();
                let mut lines = text.split("\r\n");
                let path = lines.next().unwrap_or("").split(' ').nth(1).unwrap_or("/").to_string();
                let headers = lines
                    .take_while(|l| !l.is_empty())
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let resp = handler(&Req { path, headers });
                let mut head = format!(
                    "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                    resp.status,
                    resp.body.len()
                );
                for (k, v) in &resp.headers {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&resp.body).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (addr, hits)
}

/// Dialogue simulé : répond toujours la même chose et compte les appels.
struct FakePrompt {
    answer: Decision,
    asked: AtomicUsize,
}

impl ConsentPrompt for FakePrompt {
    fn ask(&self, _action: &NetAction) -> Decision {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.answer
    }
}

struct Setup {
    client: GatedClient,
    prompt: Arc<FakePrompt>,
    journal: Arc<Journal>,
    _dir: tempfile::TempDir,
}

fn setup(policy: NetworkPolicy, answer: Decision) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let journal = Arc::new(Journal::new(dir.path().join("network.log.jsonl")));
    let prompt = Arc::new(FakePrompt { answer, asked: AtomicUsize::new(0) });
    let config = GatedConfig { require_https: false, use_system_proxy: false, ..GatedConfig::default() };
    let client = GatedClient::new(policy, prompt.clone(), Arc::clone(&journal), config);
    Setup { client, prompt, journal, _dir: dir }
}

fn action_for(addr: SocketAddr, path: &str) -> NetAction {
    let url = Url::parse(&format!("http://{addr}{path}")).unwrap();
    NetAction::new(ActionKind::SkillSearch, "Chercher des skills", vec![url])
}

#[tokio::test]
async fn policy_off_blocks_without_asking_and_is_logged() {
    let (addr, hits) = serve(Arc::new(|_| Resp::ok(b"x"))).await;
    let s = setup(NetworkPolicy::Off, Decision::Accepted);
    let err = s.client.begin(action_for(addr, "/a")).await.err().unwrap();
    assert!(matches!(err, NetError::BlockedByPolicy));
    assert_eq!(s.prompt.asked.load(Ordering::SeqCst), 0, "aucun dialogue en mode coupé");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let log = s.journal.read_recent(10).unwrap();
    assert_eq!(log[0].decision, Some(JournalDecision::BlockedByPolicy));
}

#[tokio::test]
async fn refusal_sends_nothing_and_is_logged() {
    let (addr, hits) = serve(Arc::new(|_| Resp::ok(b"x"))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Refused);
    let err = s.client.begin(action_for(addr, "/a")).await.err().unwrap();
    assert!(matches!(err, NetError::Refused));
    assert_eq!(s.prompt.asked.load(Ordering::SeqCst), 1);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let log = s.journal.read_recent(10).unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].decision, Some(JournalDecision::Refused));
    assert_eq!(log[0].hosts, vec!["127.0.0.1".to_string()]);
}

#[tokio::test]
async fn accepted_action_fetches_and_logs_each_request() {
    let (addr, hits) = serve(Arc::new(|r: &Req| Resp::ok(format!("{{\"path\":\"{}\"}}", r.path).as_bytes()))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let session = s.client.begin(action_for(addr, "/a")).await.unwrap();
    let value: serde_json::Value =
        session.get_json(&Url::parse(&format!("http://{addr}/a?token=secret")).unwrap()).await.unwrap();
    assert_eq!(value["path"], "/a?token=secret");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let log = s.journal.read_recent(10).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].event, JournalEvent::Decision);
    assert_eq!(log[0].event, JournalEvent::Request);
    assert_eq!(log[0].status, Some(200));
    let logged_url = log[0].url.as_deref().unwrap();
    assert!(!logged_url.contains("secret"), "les paramètres d'URL ne sont pas journalisés");
}

#[tokio::test]
async fn undeclared_host_is_refused_before_any_request() {
    let (addr, hits) = serve(Arc::new(|_| Resp::ok(b"x"))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let session = s.client.begin(action_for(addr, "/a")).await.unwrap();
    let other = Url::parse(&format!("http://localhost:{}/a", addr.port())).unwrap();
    let err = session.get_bytes(&other).await.err().unwrap();
    assert!(matches!(err, NetError::HostNotAllowed(_)));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert_eq!(s.journal.read_recent(1).unwrap()[0].event, JournalEvent::Blocked);
}

#[tokio::test]
async fn redirect_to_undeclared_host_is_blocked_and_logged() {
    let port_cell = Arc::new(AtomicUsize::new(0));
    let port_for_handler = Arc::clone(&port_cell);
    let (addr, hits) = serve(Arc::new(move |r: &Req| {
        if r.path == "/start" {
            let port = port_for_handler.load(Ordering::SeqCst);
            Resp {
                status: 302,
                headers: vec![("Location".into(), format!("http://localhost:{port}/next"))],
                body: vec![],
            }
        } else {
            Resp::ok(b"should not be reached")
        }
    }))
    .await;
    port_cell.store(addr.port() as usize, Ordering::SeqCst);
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let session = s.client.begin(action_for(addr, "/start")).await.unwrap();
    let result = session.get_bytes(&Url::parse(&format!("http://{addr}/start")).unwrap()).await;
    assert!(result.is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 1, "la cible de la redirection n'est jamais contactée");
    let log = s.journal.read_recent(10).unwrap();
    assert!(
        log.iter().any(|e| e.event == JournalEvent::Blocked && e.url.as_deref().unwrap_or("").contains("localhost"))
    );
}

fn sha256_hex(data: &[u8]) -> String {
    hf_net::to_hex(&Sha256::digest(data))
}

/// Serveur de fichier qui honore l'en-tête Range.
fn file_handler(content: Arc<Vec<u8>>) -> Handler {
    Arc::new(move |r: &Req| {
        if let Some(range) = r.headers.get("range") {
            let start: usize = range.trim_start_matches("bytes=").trim_end_matches('-').parse().unwrap();
            if start >= content.len() {
                return Resp { status: 416, headers: vec![], body: vec![] };
            }
            return Resp {
                status: 206,
                headers: vec![(
                    "Content-Range".into(),
                    format!("bytes {start}-{}/{}", content.len() - 1, content.len()),
                )],
                body: content[start..].to_vec(),
            };
        }
        Resp::ok(&content)
    })
}

#[tokio::test]
async fn download_verifies_checksum_and_resumes_partial_files() {
    let content: Arc<Vec<u8>> = Arc::new((0..200_000u32).map(|i| (i % 251) as u8).collect());
    let (addr, _) = serve(file_handler(Arc::clone(&content))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("models/model.gguf");

    // Un fichier partiel existe déjà : le téléchargement doit reprendre.
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(format!("{}.part", dest.display()), &content[..50_000]).unwrap();

    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    let report = session.download(&url, &dest, Some(&sha256_hex(&content)), &|_, _| {}).await.unwrap();
    assert!(report.resumed);
    assert_eq!(report.bytes, content.len() as u64);
    assert_eq!(std::fs::read(&dest).unwrap(), *content);
    assert!(!std::path::Path::new(&format!("{}.part", dest.display())).exists());
    assert_eq!(s.journal.read_recent(1).unwrap()[0].sha256_ok, Some(true));
}

#[tokio::test]
async fn download_with_wrong_checksum_leaves_no_file() {
    let content: Arc<Vec<u8>> = Arc::new(vec![7u8; 10_000]);
    let (addr, _) = serve(file_handler(Arc::clone(&content))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    let err = session.download(&url, &dest, Some(&"0".repeat(64)), &|_, _| {}).await.err().unwrap();
    assert!(matches!(err, NetError::ChecksumMismatch { .. }));
    assert!(!dest.exists());
    assert!(!std::path::Path::new(&format!("{}.part", dest.display())).exists());
    assert_eq!(s.journal.read_recent(1).unwrap()[0].sha256_ok, Some(false));
}

#[tokio::test]
async fn plain_http_is_refused_when_https_is_required() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Arc::new(Journal::new(dir.path().join("log.jsonl")));
    let prompt = Arc::new(FakePrompt { answer: Decision::Accepted, asked: AtomicUsize::new(0) });
    let client = GatedClient::new(NetworkPolicy::Ask, prompt.clone(), journal, GatedConfig::default());
    let action = NetAction::new(ActionKind::ModelDownload, "x", vec![Url::parse("http://example.org/a").unwrap()]);
    assert!(matches!(client.begin(action).await.err().unwrap(), NetError::InsecureUrl(_)));
    assert_eq!(prompt.asked.load(Ordering::SeqCst), 0);
}

/// Serveur qui envoie une réponse brute puis, au choix, ferme la connexion ou
/// la laisse ouverte sans plus rien envoyer (connexion « figée »).
async fn serve_raw(head_and_body: Vec<u8>, hang: bool) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { break };
            let raw = head_and_body.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(&raw).await;
                if hang {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
                let _ = socket.shutdown().await;
            });
        }
    });
    addr
}

fn setup_with_read_timeout(timeout: std::time::Duration) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let journal = Arc::new(Journal::new(dir.path().join("network.log.jsonl")));
    let prompt = Arc::new(FakePrompt { answer: Decision::Accepted, asked: AtomicUsize::new(0) });
    let config =
        GatedConfig { require_https: false, use_system_proxy: false, read_timeout: timeout, ..GatedConfig::default() };
    let client = GatedClient::new(NetworkPolicy::Ask, prompt.clone(), Arc::clone(&journal), config);
    Setup { client, prompt, journal, _dir: dir }
}

fn part_of(dest: &std::path::Path) -> std::path::PathBuf {
    hf_net::part_path(dest)
}

#[tokio::test]
async fn stalled_connection_fails_instead_of_hanging_and_keeps_the_partial_file() {
    let mut raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n".to_vec();
    raw.extend_from_slice(&[1u8; 30_000]);
    let addr = serve_raw(raw, true).await;
    let s = setup_with_read_timeout(std::time::Duration::from_millis(500));
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();

    let started = std::time::Instant::now();
    let err = session.download(&url, &dest, Some(&"0".repeat(64)), &|_, _| {}).await.err().unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "le délai d'inactivité coupe la connexion figée");
    assert!(matches!(err, NetError::Interrupted(_)), "{err:?}");
    assert_eq!(std::fs::metadata(part_of(&dest)).unwrap().len(), 30_000, "la partie reçue est gardée");
    assert!(!dest.exists());
    let last = &s.journal.read_recent(1).unwrap()[0];
    assert_eq!((last.event, last.error.as_deref()), (JournalEvent::Request, Some("net.interrupted")));
}

#[tokio::test]
async fn early_close_is_resumable_not_corrupt() {
    let mut raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n".to_vec();
    raw.extend_from_slice(&[1u8; 40_000]);
    let addr = serve_raw(raw, false).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    let err = session.download(&url, &dest, Some(&"0".repeat(64)), &|_, _| {}).await.err().unwrap();
    assert!(matches!(err, NetError::Interrupted(_)), "{err:?}");
    // Avant : fichier partiel jugé « corrompu » et supprimé, ou laissé sans explication.
    assert_eq!(std::fs::metadata(part_of(&dest)).unwrap().len(), 40_000);
}

#[tokio::test]
async fn complete_partial_file_is_verified_and_renamed() {
    let content: Arc<Vec<u8>> = Arc::new((0..120_000u32).map(|i| (i % 13) as u8).collect());
    let (addr, hits) = serve(file_handler(Arc::clone(&content))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    // Cas d'une application fermée pendant l'ancienne vérification finale.
    std::fs::write(part_of(&dest), &*content).unwrap();
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    let report = session.download(&url, &dest, Some(&sha256_hex(&content)), &|_, _| {}).await.unwrap();
    assert!(report.resumed);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), *content);
    assert!(!part_of(&dest).exists());
}

#[tokio::test]
async fn server_ignoring_range_restarts_from_zero() {
    let content: Arc<Vec<u8>> = Arc::new((0..80_000u32).map(|i| (i % 7) as u8).collect());
    let (addr, _) = serve(Arc::new({
        let content = Arc::clone(&content);
        move |_: &Req| Resp::ok(&content)
    }))
    .await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    std::fs::write(part_of(&dest), b"garbage that is not the start").unwrap();
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    let report = session.download(&url, &dest, Some(&sha256_hex(&content)), &|_, _| {}).await.unwrap();
    assert!(!report.resumed);
    assert_eq!(std::fs::read(&dest).unwrap(), *content);
}

#[tokio::test]
async fn resume_at_the_wrong_offset_is_refused() {
    let (addr, _) = serve(Arc::new(|_: &Req| Resp {
        status: 206,
        headers: vec![("Content-Range".into(), "bytes 0-9/10".into())],
        body: vec![0u8; 10],
    }))
    .await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    std::fs::write(part_of(&dest), [9u8; 5]).unwrap();
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    assert!(session.download(&url, &dest, None, &|_, _| {}).await.is_err());
    assert!(!dest.exists(), "aucun fichier mélangé n'est rendu visible");
    assert!(!part_of(&dest).exists(), "la prochaine tentative repart de zéro");
}

#[tokio::test]
async fn progress_ends_on_the_full_size() {
    let content: Arc<Vec<u8>> = Arc::new(vec![3u8; 300_000]);
    let (addr, _) = serve(file_handler(Arc::clone(&content))).await;
    let s = setup(NetworkPolicy::Ask, Decision::Accepted);
    let url = Url::parse(&format!("http://{addr}/model.gguf")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.gguf");
    let seen = std::sync::Mutex::new(Vec::new());
    let session = s.client.begin(action_for(addr, "/model.gguf")).await.unwrap();
    session
        .download(&url, &dest, Some(&sha256_hex(&content)), &|b, t| seen.lock().unwrap().push((b, t)))
        .await
        .unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&(0, Some(300_000))));
    assert_eq!(seen.last(), Some(&(300_000, Some(300_000))));
    assert!(seen.len() <= 4, "la progression est espacée, pas envoyée à chaque bloc : {}", seen.len());
}
