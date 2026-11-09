//! Analyse locale et déterministe d'un dépôt de code (aucun LLM, aucun réseau).
//!
//! Le parcours respecte `.gitignore`, ne suit pas les liens symboliques et
//! **n'exécute jamais rien** : seuls quelques fichiers connus (manifestes,
//! configurations d'outils, workflows de CI, documentation) sont lus. Le
//! résultat, [`RepoProfile`], est enregistré dans `repo-profile.json` et sert de
//! contexte au modèle et de référence aux contrôles de cohérence.

mod git;
mod manifests;
mod tools;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::model::SCHEMA_VERSION;

pub use manifests::normalize_command;

/// Limites du parcours, pour rester rapide sur les très gros dépôts.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_files: usize,
    /// Taille maximale d'un fichier lu (manifeste, configuration, doc).
    pub max_read_bytes: u64,
    pub tree_depth: usize,
    pub tree_max_entries: usize,
    pub commit_count: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { max_files: 50_000, max_read_bytes: 512 * 1024, tree_depth: 3, tree_max_entries: 200, commit_count: 30 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LanguageStat {
    pub name: String,
    pub files: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ManifestKind {
    Npm,
    Cargo,
    PyProject,
    Requirements,
    GoMod,
    Maven,
    Gradle,
    Composer,
    Gemfile,
    DotNet,
    Pubspec,
    SwiftPackage,
    CMake,
    Makefile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Manifest {
    pub path: String,
    pub kind: ManifestKind,
    pub name: Option<String>,
    pub dependencies: Vec<String>,
    pub dev_dependencies: Vec<String>,
}

/// Commande de build, test, lint… trouvée dans le dépôt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoCommand {
    /// Rôle : `build`, `test`, `lint`, `format`, `typecheck`, `dev`, `run`…
    pub role: String,
    pub command: String,
    /// D'où vient la commande : `package.json`, `Makefile`, `CI`, `convention`…
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Lint,
    Format,
    Test,
    TypeCheck,
    Hooks,
    Container,
    Editor,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Tool {
    pub name: String,
    pub kind: ToolKind,
    pub config: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CiWorkflow {
    pub path: String,
    pub provider: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Conventions {
    /// Indentation déclarée par EditorConfig (`2 spaces`, `tab`…).
    pub indent: Option<String>,
    /// Style de nommage majoritaire des fichiers sources.
    pub file_naming: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GitInfo {
    pub branch: Option<String>,
    pub recent_subjects: Vec<String>,
    /// Part des messages au format Conventional Commits (0 à 1).
    pub conventional_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DocExcerpt {
    pub path: String,
    pub excerpt: String,
}

/// Résultat complet de l'analyse (`repo-profile.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepoProfile {
    pub schema_version: u32,
    pub root: String,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub scanned_at: DateTime<Utc>,
    pub file_count: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_bytes: u64,
    /// Vrai si la limite de fichiers a été atteinte.
    pub truncated: bool,
    pub languages: Vec<LanguageStat>,
    pub manifests: Vec<Manifest>,
    pub package_managers: Vec<String>,
    pub commands: Vec<RepoCommand>,
    pub tools: Vec<Tool>,
    pub ci: Vec<CiWorkflow>,
    pub tree: Vec<String>,
    pub conventions: Conventions,
    pub git: Option<GitInfo>,
    pub docs: Vec<DocExcerpt>,
}

impl RepoProfile {
    /// Vrai si une commande (ou son script) est connue du dépôt.
    pub fn knows_command(&self, command: &str) -> bool {
        let wanted = normalize_command(command);
        self.commands.iter().any(|c| normalize_command(&c.command) == wanted)
            || self.ci.iter().flat_map(|w| &w.commands).any(|c| normalize_command(c).contains(&wanted))
    }
}

/// Analyse le dépôt situé à `root`.
pub fn scan_repository(root: &Path, options: &ScanOptions) -> Result<RepoProfile> {
    if !root.is_dir() {
        return Err(EngineError::RepoNotFound(root.display().to_string()));
    }
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    let mut truncated = false;
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some(".git" | "node_modules" | "target" | ".venv")))
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if files.len() >= options.max_files {
            truncated = true;
            break;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path()).to_path_buf();
        files.push((rel, size));
    }
    files.sort();

    let rel_paths: Vec<String> = files.iter().map(|(p, _)| to_unix(p)).collect();
    let reader = Reader { root, max: options.max_read_bytes };

    let languages = language_stats(&files);
    let manifests = manifests::parse_all(&rel_paths, &reader);
    let package_managers = manifests::package_managers(&rel_paths, &manifests);
    let ci = tools::ci_workflows(&rel_paths, &reader);
    let tools_found = tools::detect(&rel_paths, &reader);
    let commands = manifests::commands(&rel_paths, &manifests, &package_managers, &reader, &ci);
    let conventions =
        Conventions { indent: tools::editorconfig_indent(&rel_paths, &reader), file_naming: file_naming(&rel_paths) };

    Ok(RepoProfile {
        schema_version: SCHEMA_VERSION,
        root: root.display().to_string(),
        scanned_at: Utc::now(),
        file_count: files.len() as u32,
        total_bytes: files.iter().map(|(_, s)| s).sum(),
        truncated,
        languages,
        manifests,
        package_managers,
        commands,
        tools: tools_found,
        ci,
        tree: tree(&rel_paths, options.tree_depth, options.tree_max_entries),
        conventions,
        git: git::read(root, options.commit_count),
        docs: docs(&rel_paths, &reader),
    })
}

/// Lecture bornée de fichiers texte relatifs à la racine du dépôt.
pub(crate) struct Reader<'a> {
    root: &'a Path,
    max: u64,
}

impl Reader<'_> {
    pub(crate) fn read(&self, rel: &str) -> Option<String> {
        let path = self.root.join(rel);
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() > self.max {
            return None;
        }
        std::fs::read_to_string(path).ok()
    }
}

fn to_unix(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Langage d'un fichier d'après son extension (sources seulement).
fn language_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "Rust",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "swift" => "Swift",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "dart" => "Dart",
        "scala" => "Scala",
        "ex" | "exs" => "Elixir",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "css" | "scss" | "sass" | "less" => "CSS",
        "html" | "htm" => "HTML",
        "sql" => "SQL",
        "sh" | "bash" | "zsh" => "Shell",
        "ps1" => "PowerShell",
        "lua" => "Lua",
        "zig" => "Zig",
        _ => return None,
    })
}

fn language_stats(files: &[(PathBuf, u64)]) -> Vec<LanguageStat> {
    let mut map: BTreeMap<&str, (u32, u64)> = BTreeMap::new();
    for (path, size) in files {
        if let Some(lang) = language_of(path) {
            let e = map.entry(lang).or_default();
            e.0 += 1;
            e.1 += size;
        }
    }
    let mut stats: Vec<LanguageStat> =
        map.into_iter().map(|(name, (files, bytes))| LanguageStat { name: name.into(), files, bytes }).collect();
    stats.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    stats
}

/// Arborescence des dossiers et fichiers jusqu'à `depth` niveaux.
fn tree(paths: &[String], depth: usize, max_entries: usize) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        for level in 1..=parts.len().min(depth) {
            let is_dir = level < parts.len();
            let mut entry = parts[..level].join("/");
            if is_dir {
                entry.push('/');
            }
            entries.push(entry);
        }
    }
    entries.sort();
    entries.dedup();
    let total = entries.len();
    entries.truncate(max_entries);
    if total > max_entries {
        entries.push(format!("… ({} more)", total - max_entries));
    }
    entries
}

/// Style de nommage majoritaire des fichiers sources.
fn file_naming(paths: &[String]) -> Option<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for path in paths {
        let p = Path::new(path);
        if language_of(p).is_none() {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
        let style = if stem.contains('-') && stem == stem.to_lowercase() {
            "kebab-case"
        } else if stem.contains('_') && stem == stem.to_lowercase() {
            "snake_case"
        } else if stem.chars().next().is_some_and(char::is_uppercase) && stem.chars().any(char::is_lowercase) {
            "PascalCase"
        } else if stem.chars().next().is_some_and(char::is_lowercase) && stem.chars().any(char::is_uppercase) {
            "camelCase"
        } else {
            continue;
        };
        *counts.entry(style).or_default() += 1;
    }
    let total: u32 = counts.values().sum();
    let (style, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    // On ne parle de convention qu'au-delà de 60 % des fichiers concernés.
    (total >= 3 && n * 10 >= total * 6).then(|| style.to_owned())
}

/// Extraits des documents qui décrivent déjà les règles du projet.
fn docs(paths: &[String], reader: &Reader<'_>) -> Vec<DocExcerpt> {
    const KNOWN: &[&str] = &[
        "README.md",
        "CONTRIBUTING.md",
        "AGENTS.md",
        "CLAUDE.md",
        ".cursorrules",
        ".github/copilot-instructions.md",
        "docs/CONTRIBUTING.md",
    ];
    KNOWN
        .iter()
        .filter_map(|known| paths.iter().find(|p| p.eq_ignore_ascii_case(known)))
        .filter_map(|p| {
            reader
                .read(p)
                .map(|text| DocExcerpt { path: p.clone(), excerpt: crate::text::truncate_chars(text.trim(), 1500) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_is_bounded_and_marks_directories() {
        let paths = vec!["src/app/main.rs".to_string(), "src/lib.rs".to_string(), "README.md".to_string()];
        let t = tree(&paths, 2, 10);
        assert_eq!(t, vec!["README.md", "src/", "src/app/", "src/lib.rs"]);
        let short = tree(&paths, 3, 2);
        assert_eq!(short.last().unwrap(), "… (3 more)");
    }

    #[test]
    fn naming_convention_needs_a_clear_majority() {
        let kebab: Vec<String> = ["a-b.ts", "c-d.ts", "e-f.ts", "Main.ts"].iter().map(|s| s.to_string()).collect();
        assert_eq!(file_naming(&kebab).as_deref(), Some("kebab-case"));
        let mixed: Vec<String> = ["a-b.ts", "c_d.ts", "Ef.ts"].iter().map(|s| s.to_string()).collect();
        assert_eq!(file_naming(&mixed), None);
    }
}
