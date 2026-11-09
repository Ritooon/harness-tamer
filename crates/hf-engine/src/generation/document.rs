//! L'AGENTS.md interne : sections délimitées par des commentaires HTML,
//! invisibles dans l'aperçu et retirés à l'export.
//!
//! ```text
//! <!-- hf:section id="commands" -->
//! ## Commandes : build, tests, lint
//! …
//! <!-- hf:end -->
//! ```
//!
//! Le texte placé hors des marqueurs (ajouts manuels) est conservé tel quel.

use std::sync::LazyLock;

use regex::Regex;

use super::sections::SectionId;

pub const MARKER_END: &str = "<!-- hf:end -->";

static MARKER_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^<!--\s*hf:section\s+id="([A-Za-z0-9_-]+)"\s*-->$"#).unwrap());
static BLANK_RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

pub fn marker_start(id: &str) -> String {
    format!("<!-- hf:section id=\"{id}\" -->")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Section { id: String, content: String },
    Free(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsDoc {
    pub blocks: Vec<Block>,
}

impl AgentsDoc {
    pub fn parse(text: &str) -> Self {
        let mut blocks = Vec::new();
        let mut free = String::new();
        let mut current: Option<(String, String)> = None;
        let flush_free = |free: &mut String, blocks: &mut Vec<Block>| {
            if !free.trim().is_empty() {
                blocks.push(Block::Free(free.trim_matches('\n').to_owned()));
            }
            free.clear();
        };
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(caps) = MARKER_START.captures(trimmed) {
                // Section non refermée : elle s'arrête au marqueur suivant.
                if let Some((id, content)) = current.take() {
                    blocks.push(Block::Section { id, content: content.trim_matches('\n').to_owned() });
                }
                flush_free(&mut free, &mut blocks);
                current = Some((caps[1].to_owned(), String::new()));
            } else if trimmed == MARKER_END {
                if let Some((id, content)) = current.take() {
                    blocks.push(Block::Section { id, content: content.trim_matches('\n').to_owned() });
                }
            } else if let Some((_, content)) = current.as_mut() {
                content.push_str(line);
                content.push('\n');
            } else {
                free.push_str(line);
                free.push('\n');
            }
        }
        if let Some((id, content)) = current.take() {
            blocks.push(Block::Section { id, content: content.trim_matches('\n').to_owned() });
        }
        flush_free(&mut free, &mut blocks);
        Self { blocks }
    }

    pub fn render(&self) -> String {
        let parts: Vec<String> = self
            .blocks
            .iter()
            .map(|b| match b {
                Block::Section { id, content } => format!("{}\n{}\n{}", marker_start(id), content.trim(), MARKER_END),
                Block::Free(text) => text.trim_matches('\n').to_owned(),
            })
            .collect();
        let mut out = parts.join("\n\n");
        out.push('\n');
        out
    }

    pub fn get(&self, id: &str) -> Option<&str> {
        self.blocks.iter().find_map(|b| match b {
            Block::Section { id: i, content } if i == id => Some(content.as_str()),
            _ => None,
        })
    }

    pub fn section_ids(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Section { id, .. } => Some(id.clone()),
                Block::Free(_) => None,
            })
            .collect()
    }

    /// Remplace une section ; si elle manque, l'insère à sa place dans le plan.
    pub fn set_section(&mut self, id: SectionId, content: String) {
        let key = id.as_str();
        for block in &mut self.blocks {
            if let Block::Section { id: i, content: c } = block
                && i == key
            {
                *c = content;
                return;
            }
        }
        let mut insert_at = match self.blocks.first() {
            Some(Block::Free(_)) => 1,
            _ => 0,
        };
        for (index, block) in self.blocks.iter().enumerate() {
            if let Block::Section { id: other, .. } = block
                && SectionId::parse(other).is_some_and(|o| o.position() < id.position())
            {
                insert_at = index + 1;
            }
        }
        self.blocks.insert(insert_at, Block::Section { id: key.to_owned(), content });
    }

    /// Texte d'en-tête (hors sections), créé s'il n'existe pas.
    pub fn ensure_preamble(&mut self, preamble: &str) {
        if !matches!(self.blocks.first(), Some(Block::Free(_))) {
            self.blocks.insert(0, Block::Free(preamble.to_owned()));
        }
    }
}

/// Version exportée : sans marqueurs internes.
pub fn strip_markers(text: &str) -> String {
    let kept: Vec<&str> = text.lines().filter(|l| !MARKER_START.is_match(l.trim()) && l.trim() != MARKER_END).collect();
    let joined = kept.join("\n");
    let mut out = BLANK_RUNS.replace_all(joined.trim(), "\n\n").into_owned();
    out.push('\n');
    out
}

/// Corps d'une section sans son titre `## …`.
pub fn section_body(content: &str) -> &str {
    let trimmed = content.trim_start();
    if trimmed.starts_with("## ") { trimmed.split_once('\n').map(|(_, rest)| rest).unwrap_or("") } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Projet\n\nIntro.\n\n<!-- hf:section id=\"context\" -->\n## Contexte\nTexte A\n<!-- hf:end -->\n\nNote manuelle.\n\n<!-- hf:section id=\"git\" -->\n## Git\nTexte B\n<!-- hf:end -->\n";

    #[test]
    fn parse_render_round_trip_keeps_free_text() {
        let doc = AgentsDoc::parse(SAMPLE);
        assert_eq!(doc.section_ids(), vec!["context", "git"]);
        assert_eq!(doc.get("git"), Some("## Git\nTexte B"));
        assert_eq!(doc.render(), SAMPLE);
    }

    #[test]
    fn missing_sections_are_inserted_in_plan_order() {
        let mut doc = AgentsDoc::parse(SAMPLE);
        doc.set_section(SectionId::Commands, "## Commandes\nC".into());
        doc.set_section(SectionId::Ui, "## UI\nU".into());
        assert_eq!(doc.section_ids(), vec!["context", "commands", "git", "ui"]);
        doc.set_section(SectionId::Context, "## Contexte\nNouveau".into());
        assert_eq!(doc.get("context"), Some("## Contexte\nNouveau"));
        // Le texte libre reste avant les sections ajoutées.
        let mut empty = AgentsDoc::default();
        empty.ensure_preamble("# T");
        empty.set_section(SectionId::Goals, "## G".into());
        assert!(matches!(&empty.blocks[0], Block::Free(t) if t == "# T"));
    }

    #[test]
    fn unclosed_sections_end_at_the_next_marker() {
        let doc = AgentsDoc::parse("<!-- hf:section id=\"a\" -->\nA\n<!-- hf:section id=\"b\" -->\nB\n");
        assert_eq!(doc.get("a"), Some("A"));
        assert_eq!(doc.get("b"), Some("B"));
    }

    #[test]
    fn export_strips_markers() {
        let out = strip_markers(SAMPLE);
        assert!(!out.contains("hf:"));
        assert!(out.starts_with("# Projet\n\nIntro.\n\n## Contexte\nTexte A\n\nNote manuelle."));
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn body_without_heading() {
        assert_eq!(section_body("## T\nCorps"), "Corps");
        assert_eq!(section_body("Corps"), "Corps");
    }
}
