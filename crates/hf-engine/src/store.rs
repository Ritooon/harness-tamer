//! Stockage local, en fichiers lisibles (ADR 0002).
//!
//! ```text
//! <données>/
//! ├── settings.json
//! ├── network.log.jsonl
//! ├── models/            catalog.json + *.gguf
//! ├── runtimes/          builds optionnels (CUDA)
//! ├── skills/<slug>/     skill.json, SKILL.md, ressources
//! └── projects/<uuid>/
//!     ├── project.json
//!     ├── repo-profile.json
//!     ├── brand/         fichiers importés + brand.json
//!     ├── AGENTS.md      version courante, avec marqueurs de section
//!     ├── history/       index.jsonl + NNNN.md
//!     └── skills/<slug>/ skills rattachés, copiés puis adaptés
//! ```
//!
//! Règles : écritures atomiques (fichier temporaire puis renommage), schéma
//! versionné, suppression vers la corbeille du système.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::brand::BrandKit;
use crate::error::{EngineError, Result};
use crate::model::{
    BrandSettings, Project, ProjectInput, ProjectStatus, ProjectSummary, SCHEMA_VERSION, Settings, Skill, SkillFile,
    SkillMeta, SkillOrigin, VersionMeta, VersionOrigin,
};
use crate::scan::RepoProfile;
use crate::skills::skill_md;
use crate::text::slugify;

const PROJECT_FILE: &str = "project.json";
const AGENTS_FILE: &str = "AGENTS.md";
const HISTORY_DIR: &str = "history";
const HISTORY_INDEX: &str = "index.jsonl";
const SKILL_META: &str = "skill.json";
const SKILL_FILE: &str = "SKILL.md";

/// Suppression « douce » : la corbeille du système en production, un faux en test.
pub trait Trasher: Send + Sync {
    fn trash(&self, path: &Path) -> std::io::Result<()>;
}

/// Envoie les fichiers dans la corbeille de l'OS (récupérables).
pub struct SystemTrash;

impl Trasher for SystemTrash {
    fn trash(&self, path: &Path) -> std::io::Result<()> {
        trash::delete(path).map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// Informations jointes à une nouvelle version de l'AGENTS.md.
#[derive(Debug, Clone, Default)]
pub struct VersionDraft {
    pub origin: Option<VersionOrigin>,
    pub model: Option<String>,
    pub sections: Vec<String>,
    pub restored_from: Option<u32>,
    pub note: Option<String>,
}

/// Skill prêt à entrer dans la bibliothèque.
#[derive(Debug, Clone)]
pub struct SkillDraft {
    pub content: String,
    pub files: Vec<SkillFile>,
    pub origin: SkillOrigin,
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Ouvre (et crée si besoin) le dossier de données.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { root: root.into() };
        for dir in [store.projects_dir(), store.skills_dir(), store.root.join("models"), store.runtimes_dir()] {
            fs::create_dir_all(dir)?;
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn journal_path(&self) -> PathBuf {
        self.root.join("network.log.jsonl")
    }

    pub fn models_dir(&self, settings: &Settings) -> PathBuf {
        settings.models_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| self.root.join("models"))
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// Dossier d'un projet. L'identifiant doit être un UUID : cela interdit
    /// toute traversée de chemin (`../..`) depuis l'interface.
    pub fn project_dir(&self, id: &str) -> Result<PathBuf> {
        let uuid = Uuid::parse_str(id).map_err(|_| EngineError::ProjectNotFound(id.to_owned()))?;
        Ok(self.projects_dir().join(uuid.to_string()))
    }

    // ---------------------------------------------------------------- réglages

    pub fn load_settings(&self) -> Result<Settings> {
        match read_json_versioned::<Settings>(&self.settings_path())? {
            Some(s) => Ok(s),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_json(&self.settings_path(), settings)
    }

    // ---------------------------------------------------------------- projets

    pub fn create_project(&self, input: ProjectInput) -> Result<Project> {
        let input = validate_input(input)?;
        let now = Utc::now();
        let project = Project {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            name: input.name,
            description: input.description,
            status: ProjectStatus::Active,
            platform: input.platform,
            target_os: input.target_os,
            imposed_tech: input.imposed_tech,
            existing_code: input.existing_code,
            brand: BrandSettings { enabled: input.brand_enabled, files: Vec::new() },
            constraints: input.constraints,
            output_language: input.output_language,
            created_at: now,
            updated_at: now,
        };
        let dir = self.project_dir(&project.id)?;
        fs::create_dir_all(dir.join(HISTORY_DIR))?;
        write_json(&dir.join(PROJECT_FILE), &project)?;
        Ok(project)
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        let path = self.project_dir(id)?.join(PROJECT_FILE);
        read_json_versioned(&path)?.ok_or_else(|| EngineError::ProjectNotFound(id.to_owned()))
    }

    /// Met à jour le formulaire d'un projet ; statut et fichiers de charte sont conservés.
    pub fn update_project(&self, id: &str, input: ProjectInput) -> Result<Project> {
        let input = validate_input(input)?;
        let mut project = self.get_project(id)?;
        project.name = input.name;
        project.description = input.description;
        project.platform = input.platform;
        project.target_os = input.target_os;
        project.imposed_tech = input.imposed_tech;
        project.existing_code = input.existing_code;
        project.brand.enabled = input.brand_enabled;
        project.constraints = input.constraints;
        project.output_language = input.output_language;
        self.save_project(&mut project)?;
        Ok(project)
    }

    fn save_project(&self, project: &mut Project) -> Result<()> {
        project.updated_at = Utc::now();
        write_json(&self.project_dir(&project.id)?.join(PROJECT_FILE), project)
    }

    /// Archive ou restaure : seul le statut change, le dossier ne bouge pas.
    pub fn set_status(&self, id: &str, status: ProjectStatus) -> Result<Project> {
        let mut project = self.get_project(id)?;
        project.status = status;
        self.save_project(&mut project)?;
        Ok(project)
    }

    /// Envoie le dossier du projet à la corbeille. La confirmation est demandée
    /// par l'application avant l'appel.
    pub fn delete_project(&self, id: &str, trasher: &dyn Trasher) -> Result<()> {
        let dir = self.project_dir(id)?;
        if !dir.join(PROJECT_FILE).exists() {
            return Err(EngineError::ProjectNotFound(id.to_owned()));
        }
        trasher.trash(&dir)?;
        Ok(())
    }

    /// Tous les projets lisibles. Un dossier corrompu est ignoré plutôt que de
    /// bloquer l'accueil.
    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.projects_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let Ok(Some(project)) = read_json_versioned::<Project>(&dir.join(PROJECT_FILE)) else {
                continue;
            };
            out.push(ProjectSummary {
                has_agents: dir.join(AGENTS_FILE).exists(),
                id: project.id,
                name: project.name,
                description: project.description,
                status: project.status,
                platform: project.platform,
                imposed_tech: project.imposed_tech,
                created_at: project.created_at,
                updated_at: project.updated_at,
            });
        }
        Ok(out)
    }

    // ------------------------------------------------- analyse et charte

    pub fn save_repo_profile(&self, id: &str, profile: &RepoProfile) -> Result<()> {
        write_json(&self.project_dir(id)?.join("repo-profile.json"), profile)
    }

    pub fn load_repo_profile(&self, id: &str) -> Result<Option<RepoProfile>> {
        read_json_versioned(&self.project_dir(id)?.join("repo-profile.json"))
    }

    pub fn brand_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.project_dir(id)?.join("brand"))
    }

    /// Copie un fichier de charte dans le projet et le déclare dans `project.json`.
    /// Rend le nom retenu (suffixé si un fichier du même nom existe déjà).
    pub fn import_brand_file(&self, id: &str, source: &Path) -> Result<String> {
        let mut project = self.get_project(id)?;
        let dir = self.brand_dir(id)?;
        fs::create_dir_all(&dir)?;
        let original = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| EngineError::Invalid("brand file name".into()))?;
        if original == "brand.json" {
            return Err(EngineError::Invalid("brand.json is reserved".into()));
        }
        let name = unique_name(&dir, original);
        fs::copy(source, dir.join(&name))?;
        project.brand.files.push(name.clone());
        project.brand.enabled = true;
        self.save_project(&mut project)?;
        Ok(name)
    }

    pub fn remove_brand_file(&self, id: &str, name: &str) -> Result<()> {
        let mut project = self.get_project(id)?;
        let path = self.brand_dir(id)?.join(safe_relative(name)?);
        if path.exists() {
            fs::remove_file(path)?;
        }
        project.brand.files.retain(|f| f != name);
        self.save_project(&mut project)
    }

    pub fn save_brand_kit(&self, id: &str, kit: &BrandKit) -> Result<()> {
        write_json(&self.brand_dir(id)?.join("brand.json"), kit)
    }

    pub fn load_brand_kit(&self, id: &str) -> Result<Option<BrandKit>> {
        read_json_versioned(&self.brand_dir(id)?.join("brand.json"))
    }

    // ------------------------------------------------- AGENTS.md et versions

    pub fn load_agents(&self, id: &str) -> Result<Option<String>> {
        let path = self.project_dir(id)?.join(AGENTS_FILE);
        match fs::read_to_string(path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Enregistre une nouvelle version complète et en fait la version courante.
    /// Ordre : copie de la version, version courante, puis index — un arrêt
    /// brutal laisse au pire une copie orpheline, jamais un index incohérent.
    pub fn save_agents_version(&self, id: &str, content: &str, draft: VersionDraft) -> Result<VersionMeta> {
        let mut project = self.get_project(id)?;
        let dir = self.project_dir(id)?;
        let history = dir.join(HISTORY_DIR);
        fs::create_dir_all(&history)?;
        let number = self.list_versions(id)?.first().map(|v| v.number + 1).unwrap_or(1);
        let meta = VersionMeta {
            number,
            created_at: Utc::now(),
            origin: draft.origin.unwrap_or(VersionOrigin::ManualEdit),
            model: draft.model,
            sections: draft.sections,
            restored_from: draft.restored_from,
            note: draft.note,
        };
        write_atomic(&history.join(format!("{number:04}.md")), content.as_bytes())?;
        write_atomic(&dir.join(AGENTS_FILE), content.as_bytes())?;
        let mut index = fs::OpenOptions::new().create(true).append(true).open(history.join(HISTORY_INDEX))?;
        writeln!(index, "{}", serde_json::to_string(&meta)?)?;
        index.sync_all()?;
        self.save_project(&mut project)?;
        Ok(meta)
    }

    /// Versions, de la plus récente à la plus ancienne.
    pub fn list_versions(&self, id: &str) -> Result<Vec<VersionMeta>> {
        let path = self.project_dir(id)?.join(HISTORY_DIR).join(HISTORY_INDEX);
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut versions: Vec<VersionMeta> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        versions.sort_by_key(|v| std::cmp::Reverse(v.number));
        Ok(versions)
    }

    pub fn read_version(&self, id: &str, number: u32) -> Result<String> {
        let path = self.project_dir(id)?.join(HISTORY_DIR).join(format!("{number:04}.md"));
        fs::read_to_string(path).map_err(|_| EngineError::VersionNotFound(number))
    }

    /// Restaurer crée une nouvelle version : rien n'est écrasé.
    pub fn restore_version(&self, id: &str, number: u32) -> Result<VersionMeta> {
        let content = self.read_version(id, number)?;
        self.save_agents_version(
            id,
            &content,
            VersionDraft { origin: Some(VersionOrigin::Restore), restored_from: Some(number), ..Default::default() },
        )
    }

    // ------------------------------------------------- bibliothèque de skills

    pub fn list_skills(&self) -> Result<Vec<SkillMeta>> {
        list_skill_metas(&self.skills_dir())
    }

    pub fn get_skill(&self, slug: &str) -> Result<Skill> {
        read_skill(&self.skills_dir().join(safe_slug(slug)?), slug)
    }

    /// Ajoute un skill à la bibliothèque. Le nom et la description sont lus
    /// dans l'en-tête du SKILL.md ; le slug est rendu unique.
    pub fn create_skill(&self, draft: SkillDraft) -> Result<Skill> {
        let doc = skill_md::parse(&draft.content)?;
        let base = slugify(&doc.name);
        let base = if base.is_empty() { "skill".to_owned() } else { base };
        let slug = unique_name(&self.skills_dir(), &base);
        let now = Utc::now();
        let skill = Skill {
            meta: SkillMeta {
                schema_version: SCHEMA_VERSION,
                slug: slug.clone(),
                name: doc.name,
                description: doc.description,
                origin: draft.origin,
                version: 1,
                created_at: now,
                updated_at: now,
            },
            content: draft.content,
            files: draft.files,
        };
        write_skill(&self.skills_dir().join(&slug), &skill)?;
        Ok(skill)
    }

    pub fn update_skill(&self, slug: &str, content: String, files: Vec<SkillFile>) -> Result<Skill> {
        let dir = self.skills_dir().join(safe_slug(slug)?);
        let mut skill = read_skill(&dir, slug)?;
        bump_skill(&mut skill, content, files)?;
        write_skill(&dir, &skill)?;
        Ok(skill)
    }

    pub fn delete_skill(&self, slug: &str, trasher: &dyn Trasher) -> Result<()> {
        let dir = self.skills_dir().join(safe_slug(slug)?);
        if !dir.join(SKILL_META).exists() {
            return Err(EngineError::SkillNotFound(slug.to_owned()));
        }
        trasher.trash(&dir)?;
        Ok(())
    }

    // ------------------------------------------------- skills d'un projet

    fn project_skills_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.project_dir(id)?.join("skills"))
    }

    pub fn list_project_skills(&self, id: &str) -> Result<Vec<Skill>> {
        let dir = self.project_skills_dir(id)?;
        list_skill_metas(&dir)?.into_iter().map(|m| read_skill(&dir.join(&m.slug), &m.slug)).collect()
    }

    pub fn get_project_skill(&self, id: &str, slug: &str) -> Result<Skill> {
        read_skill(&self.project_skills_dir(id)?.join(safe_slug(slug)?), slug)
    }

    /// Rattache un skill : il est **copié** dans le projet, pour que son
    /// adaptation au projet ne modifie pas la bibliothèque.
    pub fn attach_skill(&self, id: &str, slug: &str) -> Result<Skill> {
        self.get_project(id)?;
        let skill = self.get_skill(slug)?;
        let target = self.project_skills_dir(id)?.join(&skill.meta.slug);
        if target.join(SKILL_META).exists() {
            return Err(EngineError::Invalid(format!("skill already attached: {slug}")));
        }
        write_skill(&target, &skill)?;
        Ok(skill)
    }

    pub fn update_project_skill(
        &self,
        id: &str,
        slug: &str,
        content: String,
        files: Vec<SkillFile>,
        origin: Option<SkillOrigin>,
    ) -> Result<Skill> {
        let dir = self.project_skills_dir(id)?.join(safe_slug(slug)?);
        let mut skill = read_skill(&dir, slug)?;
        bump_skill(&mut skill, content, files)?;
        if let Some(origin) = origin {
            skill.meta.origin = origin;
        }
        write_skill(&dir, &skill)?;
        Ok(skill)
    }

    pub fn detach_skill(&self, id: &str, slug: &str) -> Result<()> {
        let dir = self.project_skills_dir(id)?.join(safe_slug(slug)?);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------- utilitaires

fn validate_input(mut input: ProjectInput) -> Result<ProjectInput> {
    input.name = input.name.trim().to_owned();
    if input.name.is_empty() {
        return Err(EngineError::Invalid("name is required".into()));
    }
    if input.name.chars().count() > 120 {
        return Err(EngineError::Invalid("name is longer than 120 characters".into()));
    }
    if input.description.chars().count() > 4000 {
        return Err(EngineError::Invalid("description is longer than 4000 characters".into()));
    }
    input.imposed_tech = input.imposed_tech.iter().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
    input.imposed_tech.dedup();
    input.target_os.sort_by_key(|o| *o as u8);
    input.target_os.dedup();
    if input.existing_code.enabled {
        let path = input.existing_code.repo_path.as_deref().map(str::trim).unwrap_or("");
        if path.is_empty() || !Path::new(path).is_dir() {
            return Err(EngineError::RepoNotFound(path.to_owned()));
        }
        input.existing_code.repo_path = Some(path.to_owned());
    } else {
        input.existing_code.repo_path = None;
    }
    Ok(input)
}

fn bump_skill(skill: &mut Skill, content: String, files: Vec<SkillFile>) -> Result<()> {
    let doc = skill_md::parse(&content)?;
    for f in &files {
        safe_relative(&f.path)?;
    }
    skill.meta.name = doc.name;
    skill.meta.description = doc.description;
    skill.meta.version += 1;
    skill.meta.updated_at = Utc::now();
    skill.content = content;
    skill.files = files;
    Ok(())
}

fn list_skill_metas(dir: &Path) -> Result<Vec<SkillMeta>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if let Ok(Some(meta)) = read_json_versioned::<SkillMeta>(&entry.path().join(SKILL_META)) {
            out.push(meta);
        }
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    Ok(out)
}

fn read_skill(dir: &Path, slug: &str) -> Result<Skill> {
    let meta: SkillMeta =
        read_json_versioned(&dir.join(SKILL_META))?.ok_or_else(|| EngineError::SkillNotFound(slug.to_owned()))?;
    let content = fs::read_to_string(dir.join(SKILL_FILE))?;
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Skill { meta, content, files })
}

/// Ressources textuelles d'un skill (tout sauf skill.json et SKILL.md).
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<SkillFile>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if rel != SKILL_META
            && rel != SKILL_FILE
            && let Ok(content) = fs::read_to_string(&path)
        {
            out.push(SkillFile { path: rel, content });
        }
    }
    Ok(())
}

fn write_skill(dir: &Path, skill: &Skill) -> Result<()> {
    fs::create_dir_all(dir)?;
    // Les ressources retirées du skill disparaissent aussi du disque.
    let mut existing = Vec::new();
    if dir.exists() {
        collect_files(dir, dir, &mut existing)?;
    }
    for old in existing {
        if !skill.files.iter().any(|f| f.path == old.path) {
            let _ = fs::remove_file(dir.join(&old.path));
        }
    }
    for file in &skill.files {
        let rel = safe_relative(&file.path)?;
        write_atomic(&dir.join(rel), file.content.as_bytes())?;
    }
    write_atomic(&dir.join(SKILL_FILE), skill.content.as_bytes())?;
    write_json(&dir.join(SKILL_META), &skill.meta)
}

/// Refuse les chemins absolus ou qui remontent l'arborescence.
pub(crate) fn safe_relative(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    let ok = !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)));
    if ok { Ok(p.to_path_buf()) } else { Err(EngineError::Invalid(format!("unsafe path: {path}"))) }
}

fn safe_slug(slug: &str) -> Result<&str> {
    let ok = !slug.is_empty() && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok { Ok(slug) } else { Err(EngineError::SkillNotFound(slug.to_owned())) }
}

/// `nom`, ou `nom-2`, `nom-3`… si le nom est déjà pris dans `dir`.
fn unique_name(dir: &Path, base: &str) -> String {
    if !dir.join(base).exists() {
        return base.to_owned();
    }
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (base.to_owned(), String::new()),
    };
    (2..)
        .map(|i| format!("{stem}-{i}{ext}"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("an unused name always exists")
}

/// Écrit un fichier sans jamais laisser de version à moitié écrite.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| EngineError::Invalid("path without parent".into()))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    write_atomic(path, &data)
}

/// Lit un JSON versionné. `None` si le fichier n'existe pas ; erreur si le
/// fichier vient d'une version plus récente de HarnessForge.
pub(crate) fn read_json_versioned<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let version = value.get("schema_version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > SCHEMA_VERSION {
        return Err(EngineError::Invalid(format!(
            "{} was written by a newer HarnessForge (schema {version})",
            path.display()
        )));
    }
    migrate(&mut value, version);
    Ok(Some(serde_json::from_value(value)?))
}

/// Point d'entrée des migrations de schéma. Version 1 : rien à migrer, on
/// complète seulement le numéro de version des fichiers qui n'en ont pas.
fn migrate(value: &mut serde_json::Value, from: u32) {
    if from < SCHEMA_VERSION
        && let Some(obj) = value.as_object_mut()
    {
        obj.insert("schema_version".into(), SCHEMA_VERSION.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExistingCode, Lang, Platform, TargetOs};

    struct FakeTrash(PathBuf);

    impl Trasher for FakeTrash {
        fn trash(&self, path: &Path) -> std::io::Result<()> {
            fs::create_dir_all(&self.0)?;
            fs::rename(path, self.0.join(path.file_name().unwrap()))
        }
    }

    fn input(name: &str) -> ProjectInput {
        ProjectInput {
            name: name.into(),
            description: "App de click & collect".into(),
            platform: Platform::Mobile,
            target_os: vec![TargetOs::Ios, TargetOs::Android, TargetOs::Ios],
            imposed_tech: vec![" React Native ".into(), "".into(), "TypeScript".into()],
            existing_code: ExistingCode::default(),
            brand_enabled: false,
            constraints: "Accessibilité AA".into(),
            output_language: Lang::Fr,
        }
    }

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Store::open(dir.path().join("data")).unwrap(), dir)
    }

    #[test]
    fn create_get_list_and_normalise_a_project() {
        let (store, _dir) = store();
        let p = store.create_project(input("  Boutique  ")).unwrap();
        assert_eq!(p.name, "Boutique");
        assert_eq!(p.imposed_tech, vec!["React Native", "TypeScript"]);
        assert_eq!(p.target_os, vec![TargetOs::Android, TargetOs::Ios]);
        assert_eq!(store.get_project(&p.id).unwrap(), p);
        let list = store.list_projects().unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].has_agents);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let (store, _dir) = store();
        assert!(matches!(store.create_project(input("   ")), Err(EngineError::Invalid(_))));
        let mut with_repo = input("x");
        with_repo.existing_code = ExistingCode { enabled: true, repo_path: Some("/does/not/exist".into()) };
        assert!(matches!(store.create_project(with_repo), Err(EngineError::RepoNotFound(_))));
    }

    #[test]
    fn ids_cannot_escape_the_projects_folder() {
        let (store, _dir) = store();
        assert!(matches!(store.get_project("../../etc"), Err(EngineError::ProjectNotFound(_))));
    }

    #[test]
    fn archive_restore_and_delete_to_trash() {
        let (store, dir) = store();
        let p = store.create_project(input("Boutique")).unwrap();
        assert_eq!(store.set_status(&p.id, ProjectStatus::Archived).unwrap().status, ProjectStatus::Archived);
        assert_eq!(store.set_status(&p.id, ProjectStatus::Active).unwrap().status, ProjectStatus::Active);
        let trash = FakeTrash(dir.path().join("trash"));
        store.delete_project(&p.id, &trash).unwrap();
        assert!(store.list_projects().unwrap().is_empty());
        assert!(dir.path().join("trash").join(&p.id).join("project.json").exists(), "récupérable");
    }

    #[test]
    fn versions_are_numbered_listed_and_restorable() {
        let (store, _dir) = store();
        let p = store.create_project(input("Boutique")).unwrap();
        let v1 = store
            .save_agents_version(
                &p.id,
                "# v1",
                VersionDraft {
                    origin: Some(VersionOrigin::Generation),
                    model: Some("qwen".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let v2 = store.save_agents_version(&p.id, "# v2", VersionDraft::default()).unwrap();
        assert_eq!((v1.number, v2.number), (1, 2));
        assert_eq!(store.load_agents(&p.id).unwrap().as_deref(), Some("# v2"));
        let v3 = store.restore_version(&p.id, 1).unwrap();
        assert_eq!(v3.origin, VersionOrigin::Restore);
        assert_eq!(v3.restored_from, Some(1));
        assert_eq!(store.load_agents(&p.id).unwrap().as_deref(), Some("# v1"));
        let numbers: Vec<u32> = store.list_versions(&p.id).unwrap().iter().map(|v| v.number).collect();
        assert_eq!(numbers, vec![3, 2, 1]);
        assert!(store.list_projects().unwrap()[0].has_agents);
    }

    #[test]
    fn settings_default_then_roundtrip() {
        let (store, _dir) = store();
        let mut s = store.load_settings().unwrap();
        assert_eq!(s, Settings::default());
        s.ui_language = Lang::En;
        store.save_settings(&s).unwrap();
        assert_eq!(store.load_settings().unwrap().ui_language, Lang::En);
    }

    #[test]
    fn newer_schema_is_refused() {
        let (store, _dir) = store();
        fs::write(store.settings_path(), r#"{"schema_version": 99}"#).unwrap();
        assert!(matches!(store.load_settings(), Err(EngineError::Invalid(_))));
    }

    #[test]
    fn brand_files_are_copied_with_unique_names() {
        let (store, dir) = store();
        let p = store.create_project(input("Boutique")).unwrap();
        let src = dir.path().join("logo.svg");
        fs::write(&src, "<svg/>").unwrap();
        assert_eq!(store.import_brand_file(&p.id, &src).unwrap(), "logo.svg");
        assert_eq!(store.import_brand_file(&p.id, &src).unwrap(), "logo-2.svg");
        let project = store.get_project(&p.id).unwrap();
        assert!(project.brand.enabled);
        assert_eq!(project.brand.files, vec!["logo.svg", "logo-2.svg"]);
        store.remove_brand_file(&p.id, "logo.svg").unwrap();
        assert_eq!(store.get_project(&p.id).unwrap().brand.files, vec!["logo-2.svg"]);
    }

    fn skill_md(name: &str) -> String {
        format!("---\nname: {name}\ndescription: Relit une PR React.\n---\n\n# Revue\n")
    }

    #[test]
    fn skills_library_and_project_copies_are_independent() {
        let (store, dir) = store();
        let draft = SkillDraft {
            content: skill_md("revue-react"),
            files: vec![SkillFile { path: "scripts/check.sh".into(), content: "echo ok".into() }],
            origin: SkillOrigin::Created,
        };
        let skill = store.create_skill(draft.clone()).unwrap();
        assert_eq!(skill.meta.slug, "revue-react");
        assert_eq!(store.create_skill(draft).unwrap().meta.slug, "revue-react-2");

        let p = store.create_project(input("Boutique")).unwrap();
        store.attach_skill(&p.id, "revue-react").unwrap();
        assert!(store.attach_skill(&p.id, "revue-react").is_err());
        store
            .update_project_skill(
                &p.id,
                "revue-react",
                skill_md("revue-react").replace("PR React", "PR mobile"),
                vec![],
                None,
            )
            .unwrap();
        assert!(store.get_skill("revue-react").unwrap().content.contains("PR React"), "bibliothèque intacte");
        let attached = store.get_project_skill(&p.id, "revue-react").unwrap();
        assert!(attached.content.contains("PR mobile"));
        assert!(attached.files.is_empty(), "la ressource retirée a disparu");
        assert_eq!(attached.meta.version, 2);

        store.detach_skill(&p.id, "revue-react").unwrap();
        assert!(store.list_project_skills(&p.id).unwrap().is_empty());
        store.delete_skill("revue-react", &FakeTrash(dir.path().join("trash"))).unwrap();
        assert_eq!(store.list_skills().unwrap().len(), 1);
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        assert!(safe_relative("../x").is_err());
        assert!(safe_relative("/etc/passwd").is_err());
        assert!(safe_relative("scripts/run.sh").is_ok());
        assert!(safe_slug("../x").is_err());
    }
}
