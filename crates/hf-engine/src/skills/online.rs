//! Recherche et import de skills en ligne, **uniquement à la demande** :
//! chaque recherche et chaque import est une action réseau distincte, qui
//! ouvre le dialogue de consentement (via `hf_net::GatedClient::begin`) et
//! est inscrite au journal. Aucune donnée du projet n'est envoyée : seules
//! des lectures publiques (GET) sont faites.
//!
//! Sources gérées :
//! - dépôt GitHub public : l'arborescence est lue par l'API
//!   (`api.github.com`), les fichiers par `raw.githubusercontent.com` ;
//! - registre JSON : `{ "skills": [{ "name", "description", "url" }] }`.

use serde::{Deserialize, Serialize};
use url::Url;

use super::skill_md;
use crate::error::{EngineError, Result};
use crate::model::{Lang, SkillFile, SkillOrigin, SkillSource, SkillSourceKind};
use crate::text::{fold, truncate_chars};

const GITHUB_API: &str = "api.github.com";
const GITHUB_RAW: &str = "raw.githubusercontent.com";
/// Nombre maximal de SKILL.md lus pendant une recherche.
const MAX_SKILLS_PER_SOURCE: usize = 60;
/// Ressources importées : nombre et taille maximum, texte seulement.
const MAX_RESOURCE_FILES: usize = 30;
const MAX_RESOURCE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OnlineSkill {
    pub source_id: String,
    pub source_label: String,
    pub name: String,
    pub description: String,
    /// Adresse du SKILL.md (affichée et enregistrée comme origine).
    pub url: String,
    /// Ressources disponibles à côté du SKILL.md (chemins relatifs).
    pub resources: Vec<String>,
    /// Contenu du SKILL.md, pour l'aperçu sans nouvelle requête.
    pub preview: String,
}

/// Skill téléchargé, prêt à entrer dans la bibliothèque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ImportedSkill {
    pub content: String,
    pub files: Vec<SkillFile>,
    pub origin: SkillOrigin,
    /// Ressources ignorées (binaires, trop grosses, en surnombre).
    pub skipped: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GitTree {
    tree: Vec<GitTreeEntry>,
}

#[derive(Debug, Deserialize)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Registry {
    skills: Vec<RegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    name: String,
    #[serde(default)]
    description: String,
    url: String,
}

fn url(s: &str) -> Result<Url> {
    Url::parse(s).map_err(|e| EngineError::Invalid(format!("{s}: {e}")))
}

fn tree_url(owner: &str, repo: &str, branch: &str) -> Result<Url> {
    url(&format!("https://{GITHUB_API}/repos/{owner}/{repo}/git/trees/{branch}?recursive=1"))
}

fn raw_url(owner: &str, repo: &str, branch: &str, path: &str) -> Result<Url> {
    url(&format!("https://{GITHUB_RAW}/{owner}/{repo}/{branch}/{path}"))
}

/// Dossiers contenant un SKILL.md, sous le préfixe éventuel.
fn skill_dirs(tree: &GitTree, prefix: Option<&str>) -> Vec<String> {
    let prefix = prefix.unwrap_or("").trim_start_matches('/');
    let mut dirs: Vec<String> = tree
        .tree
        .iter()
        .filter(|e| e.kind == "blob" && e.path.starts_with(prefix))
        .filter_map(|e| e.path.strip_suffix("/SKILL.md").or_else(|| (e.path == "SKILL.md").then_some("")))
        .map(str::to_owned)
        .collect();
    dirs.sort();
    dirs
}

/// Fichiers d'un dossier de skill (hors SKILL.md), avec leur taille.
fn resources_of<'a>(tree: &'a GitTree, dir: &str) -> Vec<(&'a str, u64)> {
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    tree.tree
        .iter()
        .filter(|e| e.kind == "blob" && e.path.starts_with(&prefix))
        .map(|e| (&e.path[prefix.len()..], e.size.unwrap_or(0)))
        .filter(|(rel, _)| *rel != "SKILL.md")
        .collect()
}

/// Ressources importables : texte uniquement (scripts, gabarits, documentation).
pub fn is_text_resource(path: &str) -> bool {
    const TEXT: &[&str] = &[
        "md", "txt", "py", "sh", "bash", "ps1", "js", "mjs", "cjs", "ts", "json", "yaml", "yml", "toml", "xml", "html",
        "css", "csv", "sql", "rb", "go", "rs", "java", "kt", "swift", "ini", "cfg", "j2", "jinja", "tmpl",
    ];
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    TEXT.contains(&ext) || matches!(lower.rsplit('/').next(), Some("license" | "makefile" | "dockerfile"))
}

/// Filtre local (aucune requête) : tous les mots doivent apparaître.
pub fn matches_query(skill: &OnlineSkill, query: &str) -> bool {
    let haystack = fold(&format!("{} {} {}", skill.name, skill.description, skill.url));
    fold(query).split_whitespace().all(|w| haystack.contains(w))
}

fn purpose(lang: Lang, search: bool, labels: &str) -> String {
    match (lang, search) {
        (Lang::Fr, true) => format!("Rechercher des skills dans : {labels}. Seule la liste publique est lue."),
        (Lang::En, true) => format!("Search skills in: {labels}. Only the public listing is read."),
        (Lang::Fr, false) => format!("Importer le skill « {labels} » et ses fichiers."),
        (Lang::En, false) => format!("Import the \"{labels}\" skill and its files."),
    }
}

/// Recherche dans les sources activées : **une** action réseau, donc un
/// seul dialogue, qui liste tous les hôtes contactés.
pub async fn search(
    gated: &hf_net::GatedClient,
    sources: &[SkillSource],
    query: &str,
    lang: Lang,
) -> Result<Vec<OnlineSkill>> {
    let sources: Vec<&SkillSource> = sources.iter().filter(|s| s.enabled).collect();
    if sources.is_empty() {
        return Ok(vec![]);
    }
    let mut urls = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    for s in &sources {
        match &s.kind {
            SkillSourceKind::GithubRepo { owner, repo, branch, .. } => {
                urls.push(tree_url(owner, repo, branch)?);
                hosts.push(GITHUB_RAW.into());
            }
            SkillSourceKind::Registry { index_url } => urls.push(url(index_url)?),
        }
    }
    let labels = sources.iter().map(|s| s.label.as_str()).collect::<Vec<_>>().join(", ");
    let action = hf_net::NetAction::new(hf_net::ActionKind::SkillSearch, purpose(lang, true, &labels), urls)
        .with_extra_hosts(hosts);
    let session = gated.begin(action).await?;

    let mut results = Vec::new();
    for source in sources {
        match &source.kind {
            SkillSourceKind::GithubRepo { owner, repo, branch, path_prefix } => {
                let tree: GitTree = session.get_json(&tree_url(owner, repo, branch)?).await?;
                for dir in skill_dirs(&tree, path_prefix.as_deref()).into_iter().take(MAX_SKILLS_PER_SOURCE) {
                    let path = if dir.is_empty() { "SKILL.md".to_owned() } else { format!("{dir}/SKILL.md") };
                    let skill_url = raw_url(owner, repo, branch, &path)?;
                    let Ok(text) = session.get_text(&skill_url).await else { continue };
                    let Ok(doc) = skill_md::parse(&text) else { continue };
                    results.push(OnlineSkill {
                        source_id: source.id.clone(),
                        source_label: source.label.clone(),
                        name: doc.name,
                        description: truncate_chars(&doc.description, 1_024),
                        url: skill_url.to_string(),
                        resources: resources_of(&tree, &dir).into_iter().map(|(p, _)| p.to_owned()).collect(),
                        preview: text,
                    });
                }
            }
            SkillSourceKind::Registry { index_url } => {
                let registry: Registry = session.get_json(&url(index_url)?).await?;
                for entry in registry.skills.into_iter().take(MAX_SKILLS_PER_SOURCE) {
                    results.push(OnlineSkill {
                        source_id: source.id.clone(),
                        source_label: source.label.clone(),
                        name: entry.name,
                        description: truncate_chars(&entry.description, 1_024),
                        url: entry.url,
                        resources: vec![],
                        preview: String::new(),
                    });
                }
            }
        }
    }
    results.retain(|s| query.trim().is_empty() || matches_query(s, query));
    Ok(results)
}

/// Importe un skill trouvé par [`search`] : SKILL.md et ressources texte.
pub async fn import(
    gated: &hf_net::GatedClient,
    sources: &[SkillSource],
    found: &OnlineSkill,
    lang: Lang,
) -> Result<ImportedSkill> {
    let source = sources
        .iter()
        .find(|s| s.id == found.source_id)
        .ok_or_else(|| EngineError::Invalid(format!("unknown source: {}", found.source_id)))?;
    let skill_url = url(&found.url)?;
    let mut extra_hosts = Vec::new();
    let mut urls = vec![skill_url.clone()];
    if let SkillSourceKind::GithubRepo { owner, repo, branch, .. } = &source.kind {
        urls.push(tree_url(owner, repo, branch)?);
        extra_hosts.push(GITHUB_RAW.to_owned());
    }
    let action = hf_net::NetAction::new(hf_net::ActionKind::SkillImport, purpose(lang, false, &found.name), urls)
        .with_extra_hosts(extra_hosts);
    let session = gated.begin(action).await?;

    let content = session.get_text(&skill_url).await?;
    skill_md::parse(&content)?;
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    if let SkillSourceKind::GithubRepo { owner, repo, branch, .. } = &source.kind {
        let tree: GitTree = session.get_json(&tree_url(owner, repo, branch)?).await?;
        let base = format!("https://{GITHUB_RAW}/{owner}/{repo}/{branch}/");
        let dir = found
            .url
            .strip_prefix(&base)
            .and_then(|p| p.strip_suffix("SKILL.md"))
            .map(|d| d.trim_end_matches('/').to_owned())
            .ok_or_else(|| EngineError::Invalid(format!("unexpected skill URL: {}", found.url)))?;
        for (rel, size) in resources_of(&tree, &dir) {
            let too_many = files.len() >= MAX_RESOURCE_FILES;
            if too_many || !is_text_resource(rel) || size as usize > MAX_RESOURCE_BYTES {
                skipped.push(rel.to_owned());
                continue;
            }
            let path = if dir.is_empty() { rel.to_owned() } else { format!("{dir}/{rel}") };
            match session.get_text(&raw_url(owner, repo, branch, &path)?).await {
                Ok(text) => files.push(SkillFile { path: rel.to_owned(), content: text }),
                Err(_) => skipped.push(rel.to_owned()),
            }
        }
    }
    Ok(ImportedSkill {
        content,
        files,
        origin: SkillOrigin::Imported { source_id: source.id.clone(), url: found.url.clone() },
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> GitTree {
        let entry = |path: &str, size: u64| GitTreeEntry { path: path.into(), kind: "blob".into(), size: Some(size) };
        GitTree {
            tree: vec![
                entry("README.md", 10),
                entry("skills/pdf/SKILL.md", 100),
                entry("skills/pdf/scripts/fill.py", 2_000),
                entry("skills/pdf/assets/logo.png", 5_000),
                entry("skills/docx/SKILL.md", 100),
                GitTreeEntry { path: "skills/docx".into(), kind: "tree".into(), size: None },
                entry("other/x/SKILL.md", 1),
            ],
        }
    }

    #[test]
    fn skill_folders_and_resources_from_the_tree() {
        let t = tree();
        assert_eq!(skill_dirs(&t, Some("skills/")), vec!["skills/docx", "skills/pdf"]);
        assert_eq!(skill_dirs(&t, None).len(), 3);
        let res = resources_of(&t, "skills/pdf");
        assert_eq!(res, vec![("scripts/fill.py", 2_000), ("assets/logo.png", 5_000)]);
        assert!(is_text_resource("scripts/fill.py"));
        assert!(!is_text_resource("assets/logo.png"));
        assert!(is_text_resource("LICENSE"));
    }

    #[test]
    fn query_matching_is_local_and_accent_insensitive() {
        let s = OnlineSkill {
            source_id: "a".into(),
            source_label: "A".into(),
            name: "pdf".into(),
            description: "Remplir des formulaires PDF éditables".into(),
            url: "https://raw.githubusercontent.com/o/r/main/skills/pdf/SKILL.md".into(),
            resources: vec![],
            preview: String::new(),
        };
        assert!(matches_query(&s, "editables PDF"));
        assert!(!matches_query(&s, "excel"));
    }

    #[test]
    fn urls_target_only_the_declared_hosts() {
        let t = tree_url("anthropics", "skills", "main").unwrap();
        assert_eq!(t.host_str(), Some(GITHUB_API));
        let r = raw_url("anthropics", "skills", "main", "skills/pdf/SKILL.md").unwrap();
        assert_eq!(r.as_str(), "https://raw.githubusercontent.com/anthropics/skills/main/skills/pdf/SKILL.md");
    }
}
