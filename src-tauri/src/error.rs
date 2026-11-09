//! Erreur renvoyée à l'interface : un code stable (traduit côté React) et
//! un message anglais de diagnostic.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
}

impl CmdError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

impl From<hf_engine::EngineError> for CmdError {
    fn from(e: hf_engine::EngineError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<hf_net::NetError> for CmdError {
    fn from(e: hf_net::NetError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e.to_string())
    }
}

impl From<tauri::Error> for CmdError {
    fn from(e: tauri::Error) -> Self {
        Self::new("app", e.to_string())
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
