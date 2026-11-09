//! Client Internet soumis au consentement de l'utilisateur.
//!
//! Déroulé d'une action :
//! 1. l'appelant décrit l'action ([`NetAction`]) : but, URLs, hôtes, taille ;
//! 2. [`GatedClient::begin`] applique la politique, ouvre le dialogue natif et
//!    journalise la décision ;
//! 3. si l'utilisateur autorise, une [`NetSession`] est rendue. Elle ne peut
//!    contacter que les hôtes déclarés (redirections comprises) et journalise
//!    chaque requête.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::RANGE;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use url::Url;
use uuid::Uuid;

use crate::consent::{ActionKind, ConsentPrompt, Decision, NetAction, host_allowed};
use crate::error::NetError;
use crate::journal::{Journal, JournalDecision, JournalEntry, JournalEvent};
use crate::policy::NetworkPolicy;
use crate::to_hex;

/// Réglages techniques du client Internet.
#[derive(Debug, Clone)]
pub struct GatedConfig {
    /// Refuser toute URL qui n'est pas en HTTPS (désactivé seulement dans les tests).
    pub require_https: bool,
    /// Utiliser le proxy déclaré par le système (`HTTPS_PROXY`…), utile en entreprise.
    pub use_system_proxy: bool,
    pub user_agent: String,
    pub connect_timeout: Duration,
    pub max_redirects: usize,
}

impl Default for GatedConfig {
    fn default() -> Self {
        Self {
            require_https: true,
            use_system_proxy: true,
            user_agent: format!("HarnessForge/{}", env!("CARGO_PKG_VERSION")),
            connect_timeout: Duration::from_secs(30),
            max_redirects: 10,
        }
    }
}

/// Point d'entrée unique vers Internet.
pub struct GatedClient {
    policy: RwLock<NetworkPolicy>,
    prompt: Arc<dyn ConsentPrompt>,
    journal: Arc<Journal>,
    config: GatedConfig,
}

impl GatedClient {
    pub fn new(
        policy: NetworkPolicy,
        prompt: Arc<dyn ConsentPrompt>,
        journal: Arc<Journal>,
        config: GatedConfig,
    ) -> Self {
        Self { policy: RwLock::new(policy), prompt, journal, config }
    }

    pub fn policy(&self) -> NetworkPolicy {
        *self.policy.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_policy(&self, policy: NetworkPolicy) {
        *self.policy.write().unwrap_or_else(|p| p.into_inner()) = policy;
    }

    pub fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }

    /// Demande l'autorisation pour une action et, si elle est accordée, rend
    /// une session limitée aux hôtes déclarés.
    pub async fn begin(&self, action: NetAction) -> Result<NetSession, NetError> {
        for url in &action.urls {
            check_scheme(url, self.config.require_https)?;
        }
        let action_id = Uuid::new_v4().to_string();
        let declared = action.hosts();
        let mut entry = JournalEntry::new(&action_id, action.kind, JournalEvent::Decision);
        entry.purpose = Some(action.purpose.clone());
        entry.hosts = declared.clone();

        if self.policy() == NetworkPolicy::Off {
            entry.decision = Some(JournalDecision::BlockedByPolicy);
            self.journal.append(&entry)?;
            return Err(NetError::BlockedByPolicy);
        }

        // Le dialogue natif est bloquant : on l'exécute hors du runtime async.
        let prompt = Arc::clone(&self.prompt);
        let shown = action.clone();
        let decision =
            tokio::task::spawn_blocking(move || prompt.ask(&shown)).await.map_err(|_| NetError::PromptUnavailable)?;

        entry.decision = Some(match decision {
            Decision::Accepted => JournalDecision::Accepted,
            Decision::Refused => JournalDecision::Refused,
        });
        self.journal.append(&entry)?;
        if decision == Decision::Refused {
            return Err(NetError::Refused);
        }

        let declared = Arc::new(declared);
        let blocked = Arc::new(Mutex::new(Vec::<String>::new()));
        let redirect_policy = {
            let declared = Arc::clone(&declared);
            let blocked = Arc::clone(&blocked);
            let require_https = self.config.require_https;
            let max = self.config.max_redirects;
            reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= max {
                    return attempt.error("too many redirects");
                }
                let next = attempt.url();
                let scheme_ok = !require_https || next.scheme() == "https";
                let host_ok = next.host_str().is_some_and(|h| host_allowed(&declared, h));
                if scheme_ok && host_ok {
                    attempt.follow()
                } else {
                    blocked.lock().unwrap_or_else(|p| p.into_inner()).push(redact(next));
                    attempt.error("redirect to an undeclared host")
                }
            })
        };
        let mut builder = reqwest::Client::builder()
            .redirect(redirect_policy)
            .user_agent(self.config.user_agent.clone())
            .connect_timeout(self.config.connect_timeout);
        if !self.config.use_system_proxy {
            builder = builder.no_proxy();
        }
        let client = builder.build()?;

        Ok(NetSession {
            action_id,
            kind: action.kind,
            declared,
            client,
            journal: Arc::clone(&self.journal),
            blocked,
            require_https: self.config.require_https,
        })
    }
}

/// Résultat d'un téléchargement terminé et vérifié.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadReport {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    /// Vrai si le téléchargement a repris un fichier partiel existant.
    pub resumed: bool,
}

/// Session ouverte après un « Autoriser ». Elle vit le temps de l'action.
pub struct NetSession {
    action_id: String,
    kind: ActionKind,
    declared: Arc<Vec<String>>,
    client: reqwest::Client,
    journal: Arc<Journal>,
    blocked: Arc<Mutex<Vec<String>>>,
    require_https: bool,
}

impl NetSession {
    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn declared_hosts(&self) -> &[String] {
        &self.declared
    }

    /// Vérifie qu'une URL appartient bien aux hôtes déclarés pour l'action.
    fn check(&self, url: &Url) -> Result<(), NetError> {
        check_scheme(url, self.require_https)?;
        let host = url.host_str().ok_or_else(|| NetError::InvalidUrl(redact(url)))?;
        if !host_allowed(&self.declared, host) {
            let mut e = JournalEntry::new(&self.action_id, self.kind, JournalEvent::Blocked);
            e.url = Some(redact(url));
            e.error = Some("host not declared".into());
            self.journal.append(&e)?;
            return Err(NetError::HostNotAllowed(host.to_owned()));
        }
        Ok(())
    }

    fn log_request(
        &self,
        url: &Url,
        status: Option<u16>,
        bytes: Option<u64>,
        sha256_ok: Option<bool>,
        error: Option<String>,
    ) -> Result<(), NetError> {
        // Les redirections refusées pendant la requête sont journalisées d'abord.
        let refused: Vec<String> = std::mem::take(&mut *self.blocked.lock().unwrap_or_else(|p| p.into_inner()));
        for target in refused {
            let mut e = JournalEntry::new(&self.action_id, self.kind, JournalEvent::Blocked);
            e.url = Some(target);
            e.error = Some("redirect to an undeclared host".into());
            self.journal.append(&e)?;
        }
        let mut e = JournalEntry::new(&self.action_id, self.kind, JournalEvent::Request);
        e.method = Some("GET".into());
        e.url = Some(redact(url));
        e.status = status;
        e.bytes = bytes;
        e.sha256_ok = sha256_ok;
        e.error = error;
        self.journal.append(&e)
    }

    /// GET et lecture complète du corps (réponses petites : JSON, SKILL.md…).
    pub async fn get_bytes(&self, url: &Url) -> Result<Vec<u8>, NetError> {
        self.check(url)?;
        let response = match self.client.get(url.clone()).send().await {
            Ok(r) => r,
            Err(err) => {
                let err = NetError::from(err);
                self.log_request(url, None, None, None, Some(err.code().into()))?;
                return Err(err);
            }
        };
        let status = response.status().as_u16();
        if !response.status().is_success() {
            self.log_request(url, Some(status), None, None, None)?;
            return Err(NetError::Status { status, url: redact(url) });
        }
        let body = response.bytes().await.map_err(NetError::from)?;
        self.log_request(url, Some(status), Some(body.len() as u64), None, None)?;
        Ok(body.to_vec())
    }

    pub async fn get_text(&self, url: &Url) -> Result<String, NetError> {
        Ok(String::from_utf8_lossy(&self.get_bytes(url).await?).into_owned())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &Url) -> Result<T, NetError> {
        serde_json::from_slice(&self.get_bytes(url).await?).map_err(|e| NetError::Json(e.to_string()))
    }

    /// Télécharge un fichier volumineux vers `dest`, en reprenant un éventuel
    /// fichier partiel (`<dest>.part`) et en vérifiant l'empreinte SHA-256.
    /// Le fichier final n'apparaît qu'une fois vérifié.
    pub async fn download(
        &self,
        url: &Url,
        dest: &Path,
        expected_sha256: Option<&str>,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<DownloadReport, NetError> {
        self.check(url)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = PathBuf::from(format!("{}.part", dest.display()));
        let existing = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);

        let mut request = self.client.get(url.clone());
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(err) => {
                let err = NetError::from(err);
                self.log_request(url, None, None, None, Some(err.code().into()))?;
                return Err(err);
            }
        };
        let status = response.status().as_u16();

        let mut resumed = false;
        let mut downloaded: u64;
        if status == 416 && existing > 0 {
            // Le fichier partiel est déjà complet : il reste à le vérifier.
            downloaded = existing;
            resumed = true;
        } else if response.status().is_success() {
            let mut file = if status == 206 && existing > 0 {
                resumed = true;
                downloaded = existing;
                tokio::fs::OpenOptions::new().append(true).open(&part).await?
            } else {
                downloaded = 0;
                tokio::fs::File::create(&part).await?
            };
            let total = response.content_length().map(|len| len + downloaded);
            progress(downloaded, total);
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(err) => {
                        let err = NetError::from(err);
                        file.flush().await?;
                        self.log_request(url, Some(status), Some(downloaded), None, Some(err.code().into()))?;
                        return Err(err);
                    }
                };
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                progress(downloaded, total);
            }
            file.flush().await?;
        } else {
            self.log_request(url, Some(status), None, None, None)?;
            return Err(NetError::Status { status, url: redact(url) });
        }

        let actual = sha256_file(part.clone()).await?;
        if let Some(expected) = expected_sha256
            && !actual.eq_ignore_ascii_case(expected.trim())
        {
            let _ = tokio::fs::remove_file(&part).await;
            self.log_request(url, Some(status), Some(downloaded), Some(false), None)?;
            return Err(NetError::ChecksumMismatch { expected: expected.to_owned(), actual });
        }
        tokio::fs::rename(&part, dest).await?;
        self.log_request(url, Some(status), Some(downloaded), expected_sha256.map(|_| true), None)?;
        Ok(DownloadReport { path: dest.to_path_buf(), bytes: downloaded, sha256: actual, resumed })
    }
}

/// Empreinte SHA-256 d'un fichier, calculée par blocs hors du runtime async.
pub async fn sha256_file(path: PathBuf) -> Result<String, NetError> {
    tokio::task::spawn_blocking(move || -> Result<String, NetError> {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(to_hex(&hasher.finalize()))
    })
    .await
    .map_err(|_| NetError::Http("hashing task failed".into()))?
}

fn check_scheme(url: &Url, require_https: bool) -> Result<(), NetError> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if !require_https => Ok(()),
        "http" => Err(NetError::InsecureUrl(redact(url))),
        _ => Err(NetError::InvalidUrl(redact(url))),
    }
}

/// URL sans paramètres ni fragment : les URLs signées des CDN restent lisibles
/// dans le journal sans y recopier leurs jetons.
pub fn redact(url: &Url) -> String {
    let mut clean = url.clone();
    clean.set_query(None);
    clean.set_fragment(None);
    clean.to_string()
}
