//! Erreurs du module réseau. Chaque variante expose un code stable que
//! l'interface traduit (voir `src/i18n`).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("network access is disabled by the network policy")]
    BlockedByPolicy,
    #[error("network access was refused by the user")]
    Refused,
    #[error("host not declared for this action: {0}")]
    HostNotAllowed(String),
    #[error("non-loopback address refused: {0}")]
    NotLoopback(String),
    #[error("HTTPS is required: {0}")]
    InsecureUrl(String),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("HTTP status {status} for {url}")]
    Status { status: u16, url: String },
    #[error("SHA-256 mismatch (expected {expected}, got {actual})")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("consent dialog unavailable")]
    PromptUnavailable,
}

impl NetError {
    /// Code stable, utilisé par l'interface pour afficher un message traduit.
    pub fn code(&self) -> &'static str {
        match self {
            NetError::BlockedByPolicy => "net.blocked_by_policy",
            NetError::Refused => "net.refused",
            NetError::HostNotAllowed(_) => "net.host_not_allowed",
            NetError::NotLoopback(_) => "net.not_loopback",
            NetError::InsecureUrl(_) => "net.insecure_url",
            NetError::InvalidUrl(_) => "net.invalid_url",
            NetError::Status { .. } => "net.http_status",
            NetError::ChecksumMismatch { .. } => "net.checksum_mismatch",
            NetError::Http(_) => "net.http",
            NetError::Io(_) => "net.io",
            NetError::Json(_) => "net.json",
            NetError::PromptUnavailable => "net.prompt_unavailable",
        }
    }
}

impl From<reqwest::Error> for NetError {
    fn from(e: reqwest::Error) -> Self {
        // On n'expose pas la chaîne complète (elle peut contenir l'URL avec
        // une clé d'API locale) : seulement la nature de l'erreur.
        if let Some(status) = e.status() {
            return NetError::Status {
                status: status.as_u16(),
                url: e.url().map(|u| u.as_str().to_owned()).unwrap_or_default(),
            };
        }
        let kind = if e.is_timeout() {
            "timeout"
        } else if e.is_connect() {
            "connection failed"
        } else if e.is_redirect() {
            "redirect refused"
        } else if e.is_body() || e.is_decode() {
            "invalid response body"
        } else {
            "request failed"
        };
        NetError::Http(kind.to_owned())
    }
}
