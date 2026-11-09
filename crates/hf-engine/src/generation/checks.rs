//! Contrôles de cohérence après génération (ou édition) de l'AGENTS.md :
//! sections manquantes ou vides, commandes citées absentes du dépôt,
//! espaces réservés oubliés, réponse tronquée.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::document::{AgentsDoc, section_body};
use super::sections::SectionId;
use crate::scan::RepoProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    MissingSection,
    EmptySection,
    UnknownCommand,
    Placeholder,
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Issue {
    pub kind: IssueKind,
    pub section: Option<String>,
    /// Élément en cause (commande, texte…), affiché tel quel.
    pub detail: String,
}

/// Programmes dont une ligne est considérée comme une commande à vérifier.
const COMMAND_TOOLS: &[&str] = &[
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "npx",
    "deno",
    "cargo",
    "make",
    "go",
    "python",
    "python3",
    "pytest",
    "uv",
    "poetry",
    "pip",
    "mvn",
    "gradle",
    "./gradlew",
    "gradlew",
    "dotnet",
    "flutter",
    "dart",
    "swift",
    "composer",
    "bundle",
    "rake",
    "just",
    "task",
    "tox",
    "nox",
    "ruff",
    "mypy",
    "eslint",
    "prettier",
    "tsc",
    "vitest",
    "jest",
];

static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]+)`").unwrap());
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bTODO\b|\bTBD\b|\bXXX\b|\{\{[^}]*\}\}|<\s*(à compléter|to be completed)\s*>").unwrap()
});

/// Lignes de commande citées : blocs ```bash/sh/shell/console et code en ligne.
pub fn cited_commands(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_shell = false;
    let mut in_other = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(lang) = trimmed.strip_prefix("```") {
            if in_shell || in_other {
                in_shell = false;
                in_other = false;
            } else {
                let lang = lang.trim().to_ascii_lowercase();
                in_shell = matches!(lang.as_str(), "bash" | "sh" | "shell" | "console" | "zsh" | "powershell" | "");
                in_other = !in_shell;
            }
            continue;
        }
        if in_other {
            continue;
        }
        if in_shell {
            let cmd = trimmed.trim_start_matches("$ ").trim();
            if !cmd.is_empty() && !cmd.starts_with('#') {
                push_command(&mut out, cmd);
            }
            continue;
        }
        for caps in INLINE_CODE.captures_iter(line) {
            push_command(&mut out, caps[1].trim());
        }
    }
    out
}

fn push_command(out: &mut Vec<String>, candidate: &str) {
    // Une ligne peut enchaîner plusieurs commandes.
    for part in candidate.split("&&").map(str::trim) {
        let first = part.split_whitespace().next().unwrap_or("");
        if COMMAND_TOOLS.contains(&first) && part.split_whitespace().count() >= 2 && !out.iter().any(|c| c == part) {
            out.push(part.to_owned());
        }
    }
}

/// Commandes d'installation ou d'outillage qu'on ne s'attend pas à trouver
/// dans les scripts du dépôt (ne pas les signaler).
fn is_generic(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    matches!(
        words.as_slice(),
        ["npm", "install" | "ci" | "i", ..]
            | ["pnpm" | "yarn" | "bun", "install" | "add", ..]
            | ["yarn"]
            | ["cargo", "add" | "install" | "fetch" | "update", ..]
            | ["pip" | "uv" | "poetry", "install" | "add" | "sync", ..]
            | ["go", "mod" | "get", ..]
            | ["bundle", "install", ..]
            | ["composer", "install" | "require", ..]
            | ["dotnet", "restore" | "add", ..]
            | ["flutter" | "dart", "pub", ..]
    )
}

pub fn check_document(doc: &AgentsDoc, plan: &[SectionId], profile: Option<&RepoProfile>) -> Vec<Issue> {
    let mut issues = Vec::new();
    for id in plan {
        let key = id.as_str();
        match doc.get(key) {
            None => {
                issues.push(Issue { kind: IssueKind::MissingSection, section: Some(key.into()), detail: String::new() })
            }
            Some(content) if section_body(content).trim().chars().count() < 20 => {
                issues.push(Issue { kind: IssueKind::EmptySection, section: Some(key.into()), detail: String::new() })
            }
            _ => {}
        }
    }
    for section in doc.section_ids() {
        let content = doc.get(&section).unwrap_or("");
        for m in PLACEHOLDER.find_iter(content) {
            issues.push(Issue {
                kind: IssueKind::Placeholder,
                section: Some(section.clone()),
                detail: m.as_str().into(),
            });
        }
        if let Some(profile) = profile {
            for cmd in cited_commands(content) {
                if !is_generic(&cmd) && !profile.knows_command(&cmd) {
                    issues.push(Issue { kind: IssueKind::UnknownCommand, section: Some(section.clone()), detail: cmd });
                }
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::context::tests::profile;

    #[test]
    fn commands_are_found_in_blocks_and_inline_code() {
        let md = "Lancez `pnpm test` puis `src/main.ts`.\n```bash\n$ pnpm install && pnpm build\n# commentaire\n```\n```json\n{\"npm run\": 1}\n```";
        assert_eq!(cited_commands(md), vec!["pnpm test", "pnpm install", "pnpm build"]);
    }

    #[test]
    fn unknown_commands_missing_sections_and_placeholders() {
        let doc = AgentsDoc::parse(
            "<!-- hf:section id=\"commands\" -->\n## Commandes\n- Tests : `pnpm test`\n- Lint : `pnpm run lint`\n- Install : `pnpm install`\n<!-- hf:end -->\n<!-- hf:section id=\"git\" -->\n## Git\nTODO\n<!-- hf:end -->\n",
        );
        let issues = check_document(&doc, &[SectionId::Commands, SectionId::Git, SectionId::Ui], Some(&profile()));
        let summary: Vec<(IssueKind, Option<&str>, &str)> =
            issues.iter().map(|i| (i.kind, i.section.as_deref(), i.detail.as_str())).collect();
        assert!(summary.contains(&(IssueKind::UnknownCommand, Some("commands"), "pnpm run lint")));
        assert!(!summary.iter().any(|(_, _, d)| *d == "pnpm test" || *d == "pnpm install"));
        assert!(summary.contains(&(IssueKind::EmptySection, Some("git"), "")));
        assert!(summary.contains(&(IssueKind::Placeholder, Some("git"), "TODO")));
        assert!(summary.contains(&(IssueKind::MissingSection, Some("ui"), "")));
        // Sans dépôt (nouveau projet), les commandes sont des propositions : pas de contrôle.
        let none = check_document(&doc, &[SectionId::Commands], None);
        assert!(none.iter().all(|i| i.kind != IssueKind::UnknownCommand));
    }
}
