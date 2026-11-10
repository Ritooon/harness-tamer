//! Types du domaine, partagés avec l'interface (types TypeScript générés par
//! ts-rs quand la fonctionnalité `ts` est active).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Version courante des fichiers JSON écrits par HarnessForge.
pub const SCHEMA_VERSION: u32 = 1;

/// Langue d'un document généré ou de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub fn code(&self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

/// Plateforme cible du projet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Web,
    Mobile,
    Desktop,
    Cli,
    Api,
    Embedded,
    Library,
    Other,
}

impl Platform {
    pub fn label(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Platform::Web, _) => "Web",
            (Platform::Mobile, _) => "Mobile",
            (Platform::Desktop, _) => "Desktop",
            (Platform::Cli, _) => "CLI",
            (Platform::Api, _) => "API",
            (Platform::Embedded, Lang::Fr) => "Embarqué",
            (Platform::Embedded, Lang::En) => "Embedded",
            (Platform::Library, Lang::Fr) => "Bibliothèque",
            (Platform::Library, Lang::En) => "Library",
            (Platform::Other, Lang::Fr) => "Autre",
            (Platform::Other, Lang::En) => "Other",
        }
    }
}

/// Système cible (multi-sélection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum TargetOs {
    Windows,
    Linux,
    Macos,
    Android,
    Ios,
    Browser,
    Other,
}

impl TargetOs {
    pub fn label(&self) -> &'static str {
        match self {
            TargetOs::Windows => "Windows",
            TargetOs::Linux => "Linux",
            TargetOs::Macos => "macOS",
            TargetOs::Android => "Android",
            TargetOs::Ios => "iOS",
            TargetOs::Browser => "Browser",
            TargetOs::Other => "Other",
        }
    }
}

/// Code existant à respecter.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExistingCode {
    pub enabled: bool,
    /// Chemin absolu du dépôt local à analyser.
    pub repo_path: Option<String>,
}

/// Charte graphique fournie par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BrandSettings {
    pub enabled: bool,
    /// Noms des fichiers importés dans le dossier `brand/` du projet.
    pub files: Vec<String>,
}

/// Contenu de `project.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    pub platform: Platform,
    pub target_os: Vec<TargetOs>,
    pub imposed_tech: Vec<String>,
    pub existing_code: ExistingCode,
    pub brand: BrandSettings,
    pub constraints: String,
    pub output_language: Lang,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

/// Données du formulaire « Nouveau projet » (et de sa modification).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProjectInput {
    pub name: String,
    pub description: String,
    pub platform: Platform,
    pub target_os: Vec<TargetOs>,
    pub imposed_tech: Vec<String>,
    pub existing_code: ExistingCode,
    pub brand_enabled: bool,
    pub constraints: String,
    pub output_language: Lang,
}

/// Ligne de la liste de l'accueil.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    pub platform: Platform,
    pub imposed_tech: Vec<String>,
    pub has_agents: bool,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

/// Origine d'une version de l'AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VersionOrigin {
    Generation,
    SectionRegeneration,
    ManualEdit,
    Restore,
}

/// Ligne de `history/index.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VersionMeta {
    pub number: u32,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    pub origin: VersionOrigin,
    /// Modèle qui a produit la version, s'il y en a un.
    pub model: Option<String>,
    /// Sections touchées (identifiants), vide pour une édition libre.
    pub sections: Vec<String>,
    pub restored_from: Option<u32>,
    pub note: Option<String>,
}

/// Politique réseau, telle que l'interface la manipule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicySetting {
    #[default]
    Ask,
    Off,
}

impl From<NetworkPolicySetting> for hf_net::NetworkPolicy {
    fn from(value: NetworkPolicySetting) -> Self {
        match value {
            NetworkPolicySetting::Ask => hf_net::NetworkPolicy::Ask,
            NetworkPolicySetting::Off => hf_net::NetworkPolicy::Off,
        }
    }
}

/// Moteur d'inférence utilisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackend {
    /// `llama-server` livré avec l'application (par défaut).
    #[default]
    Embedded,
    /// Ollama déjà installé et lancé par l'utilisateur sur 127.0.0.1:11434.
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct RuntimeSettings {
    pub backend: RuntimeBackend,
    /// Taille du contexte, en tokens.
    pub context_size: u32,
    /// Couches confiées au GPU ; `None` = toutes (llama.cpp ajuste).
    pub gpu_layers: Option<u32>,
    /// Couches dont les experts MoE restent en RAM (`--n-cpu-moe`).
    pub cpu_moe_layers: Option<u32>,
    /// Nom du modèle côté Ollama, si ce moteur est choisi.
    pub ollama_model: Option<String>,
    /// Programme `llama-server` choisi par l'utilisateur ; `None` = recherche automatique.
    pub server_path: Option<String>,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            backend: RuntimeBackend::Embedded,
            context_size: 16_384,
            gpu_layers: None,
            cpu_moe_layers: None,
            ollama_model: None,
            server_path: None,
        }
    }
}

/// Source de recherche de skills en ligne.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SkillSource {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub kind: SkillSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillSourceKind {
    /// Dépôt GitHub public : les dossiers contenant un `SKILL.md` sont des skills.
    GithubRepo { owner: String, repo: String, branch: String, path_prefix: Option<String> },
    /// Registre : un fichier JSON `{ "skills": [{ name, description, url }] }`.
    Registry { index_url: String },
}

/// Contenu de `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub ui_language: Lang,
    pub default_output_language: Lang,
    pub network_policy: NetworkPolicySetting,
    /// Identifiant du modèle actif dans le catalogue.
    pub active_model: Option<String>,
    /// Dossier des modèles, s'il diffère de `<données>/models`.
    pub models_dir: Option<String>,
    /// Dossier proposé par défaut pour les exports.
    pub default_export_dir: Option<String>,
    pub runtime: RuntimeSettings,
    pub skill_sources: Vec<SkillSource>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ui_language: Lang::Fr,
            default_output_language: Lang::Fr,
            network_policy: NetworkPolicySetting::Ask,
            active_model: None,
            models_dir: None,
            default_export_dir: None,
            runtime: RuntimeSettings::default(),
            skill_sources: vec![SkillSource {
                id: "anthropics-skills".into(),
                label: "anthropics/skills (GitHub)".into(),
                enabled: true,
                kind: SkillSourceKind::GithubRepo {
                    owner: "anthropics".into(),
                    repo: "skills".into(),
                    branch: "main".into(),
                    path_prefix: Some("skills/".into()),
                },
            }],
        }
    }
}

/// Origine d'un skill de la bibliothèque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillOrigin {
    Created,
    Imported { source_id: String, url: String },
    Adapted { from: String },
}

/// Contenu de `skill.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SkillMeta {
    pub schema_version: u32,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub origin: SkillOrigin,
    pub version: u32,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

/// Fichier de ressource d'un skill (script, gabarit…), stocké en texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SkillFile {
    /// Chemin relatif au dossier du skill, sans `..`.
    pub path: String,
    pub content: String,
}

/// Un skill complet : métadonnées, SKILL.md et ressources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Skill {
    pub meta: SkillMeta,
    pub content: String,
    pub files: Vec<SkillFile>,
}
