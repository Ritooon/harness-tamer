//! Erreurs du cœur. Chaque variante expose un code stable (`code()`) que
//! l'interface traduit ; le message anglais sert de repli et de diagnostic.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("skill not found: {0}")]
    SkillNotFound(String),
    #[error("version not found: {0}")]
    VersionNotFound(u32),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("repository path not found: {0}")]
    RepoNotFound(String),
    #[error("export target already contains HarnessForge files: {0}")]
    ExportTargetExists(String),
    #[error("no model is ready; download or import one in the settings")]
    ModelNotReady,
    #[error("model runtime error: {0}")]
    Runtime(String),
    #[error("could not install the llama.cpp engine: {0}")]
    RuntimeInstall(String),
    #[error("the model returned an unusable answer: {0}")]
    BadModelOutput(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error(transparent)]
    Net(#[from] hf_net::NetError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::ProjectNotFound(_) => "project.not_found",
            EngineError::SkillNotFound(_) => "skill.not_found",
            EngineError::VersionNotFound(_) => "version.not_found",
            EngineError::Invalid(_) => "input.invalid",
            EngineError::RepoNotFound(_) => "repo.not_found",
            EngineError::ExportTargetExists(_) => "export.target_exists",
            EngineError::ModelNotReady => "model.not_ready",
            EngineError::Runtime(_) => "model.runtime",
            EngineError::RuntimeInstall(_) => "runtime.install",
            EngineError::BadModelOutput(_) => "model.bad_output",
            EngineError::Cancelled => "cancelled",
            EngineError::Net(e) => e.code(),
            EngineError::Io(_) => "io",
            EngineError::Json(_) => "json",
        }
    }
}

pub type Result<T, E = EngineError> = std::result::Result<T, E>;
