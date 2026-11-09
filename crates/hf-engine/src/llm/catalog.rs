//! Catalogue des modèles proposés (embarqué dans l'application) et registre
//! des modèles installés localement (`models/catalog.json`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{EngineError, Result};
use crate::model::{Lang, SCHEMA_VERSION};
use crate::store::{read_json_versioned, write_json};

/// Catalogue intégré : empreintes SHA-256 relevées sur Hugging Face.
const BUILTIN: &str = include_str!("../../../../resources/catalog/models.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Default,
    Light,
    Alternative,
    Small,
}

/// Façon d'activer ou non le mode réflexion du modèle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingControl {
    /// Qwen : `chat_template_kwargs: { enable_thinking }`.
    ChatTemplateKwargs,
    /// Gemma 4 : jeton placé au début du message système pour activer la réflexion.
    SystemToken { token: String },
    /// Le modèle réfléchit toujours (Ministral Reasoning).
    AlwaysOn,
    /// Pas de mode réflexion connu (modèle importé).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LocalizedText {
    pub fr: String,
    pub en: String,
}

impl LocalizedText {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::Fr => &self.fr,
            Lang::En => &self.en,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CatalogModel {
    pub id: String,
    pub label: String,
    pub family: String,
    pub role: ModelRole,
    pub license: String,
    pub parameters: String,
    pub repo: String,
    pub file: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    pub sha256: String,
    /// Mémoire (RAM + VRAM) conseillée, en Go.
    pub min_memory_gb: u32,
    pub moe: bool,
    pub thinking: ThinkingControl,
    pub notes: LocalizedText,
}

impl CatalogModel {
    /// `https://huggingface.co/<dépôt>/resolve/main/<fichier>`.
    pub fn download_url(&self) -> Result<Url> {
        Url::parse(&format!("https://huggingface.co/{}/resolve/main/{}", self.repo, self.file))
            .map_err(|e| EngineError::Invalid(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Catalog {
    pub schema_version: u32,
    /// Hôtes contactés par un téléchargement (redirections vers le CDN comprises).
    pub download_hosts: Vec<String>,
    pub models: Vec<CatalogModel>,
}

impl Catalog {
    pub fn builtin() -> Self {
        serde_json::from_str(BUILTIN).expect("resources/catalog/models.json is valid")
    }

    pub fn get(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn by_role(&self, role: ModelRole) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.role == role)
    }
}

/// Modèle importé par l'utilisateur (fichier GGUF déjà présent sur le disque).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ImportedModel {
    pub id: String,
    pub label: String,
    /// Chemin absolu du fichier (il n'est pas copié).
    pub path: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    pub thinking: ThinkingControl,
}

/// Contenu de `models/catalog.json` : les modèles importés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalRegistry {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub imported: Vec<ImportedModel>,
}

/// État d'un modèle du point de vue de l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    NotDownloaded,
    /// Téléchargement interrompu, reprenable.
    Partial,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub role: Option<ModelRole>,
    pub state: ModelState,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes_on_disk: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    pub path: String,
    pub imported: bool,
    pub min_memory_gb: Option<u32>,
    pub notes: Option<LocalizedText>,
    pub license: Option<String>,
}

/// Modèle prêt à être lancé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub thinking: ThinkingControl,
    pub moe: bool,
}

pub struct ModelLibrary {
    dir: PathBuf,
    catalog: Catalog,
}

impl ModelLibrary {
    pub fn new(dir: impl Into<PathBuf>, catalog: Catalog) -> Self {
        Self { dir: dir.into(), catalog }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    fn registry_path(&self) -> PathBuf {
        self.dir.join("catalog.json")
    }

    pub fn registry(&self) -> Result<LocalRegistry> {
        Ok(read_json_versioned(&self.registry_path())?.unwrap_or_default())
    }

    /// Chemin de destination d'un modèle du catalogue.
    pub fn file_path(&self, model: &CatalogModel) -> PathBuf {
        self.dir.join(&model.file)
    }

    pub fn statuses(&self) -> Result<Vec<ModelStatus>> {
        let mut out = Vec::new();
        for m in &self.catalog.models {
            let path = self.file_path(m);
            let full = std::fs::metadata(&path).map(|md| md.len()).ok();
            let part = std::fs::metadata(format!("{}.part", path.display())).map(|md| md.len()).ok();
            let (state, bytes) = match (full, part) {
                // Le fichier final n'existe qu'après vérification de l'empreinte.
                (Some(n), _) => (ModelState::Ready, n),
                (None, Some(n)) => (ModelState::Partial, n),
                (None, None) => (ModelState::NotDownloaded, 0),
            };
            out.push(ModelStatus {
                id: m.id.clone(),
                label: m.label.clone(),
                role: Some(m.role),
                state,
                bytes_on_disk: bytes,
                size_bytes: m.size_bytes,
                path: path.display().to_string(),
                imported: false,
                min_memory_gb: Some(m.min_memory_gb),
                notes: Some(m.notes.clone()),
                license: Some(m.license.clone()),
            });
        }
        for m in self.registry()?.imported {
            let exists = Path::new(&m.path).is_file();
            out.push(ModelStatus {
                id: m.id,
                label: m.label,
                role: None,
                state: if exists { ModelState::Ready } else { ModelState::NotDownloaded },
                bytes_on_disk: if exists { m.size_bytes } else { 0 },
                size_bytes: m.size_bytes,
                path: m.path,
                imported: true,
                min_memory_gb: None,
                notes: None,
                license: None,
            });
        }
        Ok(out)
    }

    /// Modèle prêt correspondant à `id`, ou erreur `ModelNotReady`.
    pub fn resolve(&self, id: &str) -> Result<ResolvedModel> {
        if let Some(m) = self.catalog.get(id) {
            let path = self.file_path(m);
            if path.is_file() {
                return Ok(ResolvedModel {
                    id: m.id.clone(),
                    label: m.label.clone(),
                    path,
                    thinking: m.thinking.clone(),
                    moe: m.moe,
                });
            }
            return Err(EngineError::ModelNotReady);
        }
        let imported = self.registry()?.imported.into_iter().find(|m| m.id == id).ok_or(EngineError::ModelNotReady)?;
        let path = PathBuf::from(&imported.path);
        if !path.is_file() {
            return Err(EngineError::ModelNotReady);
        }
        Ok(ResolvedModel { id: imported.id, label: imported.label, path, thinking: imported.thinking, moe: false })
    }

    /// Enregistre un fichier GGUF local (vérifie l'en-tête `GGUF`). Le
    /// mode réflexion est deviné d'après le nom du fichier.
    pub fn import(&self, path: &Path) -> Result<ImportedModel> {
        use std::io::Read;
        let mut magic = [0u8; 4];
        std::fs::File::open(path)?.read_exact(&mut magic)?;
        if &magic != b"GGUF" {
            return Err(EngineError::Invalid(format!("not a GGUF file: {}", path.display())));
        }
        let size_bytes = std::fs::metadata(path)?.len();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model").to_owned();
        let lower = stem.to_lowercase();
        let thinking = if lower.contains("qwen3") {
            ThinkingControl::ChatTemplateKwargs
        } else if lower.contains("gemma-4") || lower.contains("gemma4") {
            ThinkingControl::SystemToken { token: "<|think|>".into() }
        } else if lower.contains("reasoning") {
            ThinkingControl::AlwaysOn
        } else {
            ThinkingControl::Unknown
        };
        let mut registry = self.registry()?;
        let id = format!("local-{}", crate::text::slugify(&stem));
        registry.imported.retain(|m| m.id != id);
        let model = ImportedModel { id, label: stem, path: path.display().to_string(), size_bytes, thinking };
        registry.imported.push(model.clone());
        registry.schema_version = SCHEMA_VERSION;
        write_json(&self.registry_path(), &registry)?;
        Ok(model)
    }

    /// Retire un modèle importé du registre (le fichier n'est pas touché).
    pub fn forget(&self, id: &str) -> Result<()> {
        let mut registry = self.registry()?;
        registry.imported.retain(|m| m.id != id);
        write_json(&self.registry_path(), &registry)
    }
}

/// Télécharge un modèle du catalogue, après le dialogue de consentement
/// (déclenché par `GatedClient::begin`). Reprend un fichier partiel et
/// vérifie l'empreinte SHA-256 avant de rendre le fichier visible.
pub async fn download_model(
    gated: &hf_net::GatedClient,
    library: &ModelLibrary,
    model_id: &str,
    lang: Lang,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<PathBuf> {
    let model =
        library.catalog.get(model_id).ok_or_else(|| EngineError::Invalid(format!("unknown model: {model_id}")))?;
    let url = model.download_url()?;
    let size = hf_net::human_size(model.size_bytes, lang.code());
    let purpose = match lang {
        Lang::Fr => format!("Télécharger le modèle {} ({size}) depuis Hugging Face.", model.label),
        Lang::En => format!("Download the {} model ({size}) from Hugging Face.", model.label),
    };
    let action = hf_net::NetAction::new(hf_net::ActionKind::ModelDownload, purpose, vec![url.clone()])
        .with_extra_hosts(library.catalog.download_hosts.clone())
        .with_estimated_bytes(Some(model.size_bytes));
    let session = gated.begin(action).await?;
    let dest = library.file_path(model);
    let report = session.download(&url, &dest, Some(&model.sha256), progress).await?;
    Ok(report.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_consistent() {
        let c = Catalog::builtin();
        assert_eq!(c.by_role(ModelRole::Default).unwrap().id, "qwen3.6-35b-a3b-q4km");
        assert_eq!(c.by_role(ModelRole::Light).unwrap().id, "qwen3.5-9b-q4km");
        for m in &c.models {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.file.ends_with(".gguf"));
            let url = m.download_url().unwrap();
            assert!(hf_net::host_allowed(&c.download_hosts, url.host_str().unwrap()));
        }
    }

    #[test]
    fn statuses_import_and_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let lib = ModelLibrary::new(dir.path().join("models"), Catalog::builtin());
        std::fs::create_dir_all(lib.dir()).unwrap();
        let light = lib.catalog().by_role(ModelRole::Light).unwrap().clone();
        std::fs::write(format!("{}.part", lib.file_path(&light).display()), b"123").unwrap();
        let st = lib.statuses().unwrap();
        let s = st.iter().find(|s| s.id == light.id).unwrap();
        assert_eq!((s.state, s.bytes_on_disk), (ModelState::Partial, 3));
        assert!(matches!(lib.resolve(&light.id), Err(EngineError::ModelNotReady)));

        let gguf = dir.path().join("Qwen3-Coder-Q4.gguf");
        std::fs::write(&gguf, b"GGUF\x03\0\0\0").unwrap();
        let imported = lib.import(&gguf).unwrap();
        assert_eq!(imported.thinking, ThinkingControl::ChatTemplateKwargs);
        assert_eq!(lib.resolve(&imported.id).unwrap().path, gguf);

        let not_gguf = dir.path().join("x.gguf");
        std::fs::write(&not_gguf, b"nope").unwrap();
        assert!(lib.import(&not_gguf).is_err());
        lib.forget(&imported.id).unwrap();
        assert!(lib.resolve(&imported.id).is_err());
    }
}
