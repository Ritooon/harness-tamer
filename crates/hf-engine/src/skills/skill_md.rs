//! Lecture et écriture du format SKILL.md : un en-tête YAML (`name`,
//! `description`, champs optionnels) suivi d'instructions en Markdown.
//!
//! L'en-tête n'utilise qu'un sous-ensemble simple de YAML (clés de premier
//! niveau, valeurs sur une ligne ou blocs `|` / `>`). Un analyseur minimal
//! suffit et évite une dépendance de plus ; les clés inconnues sont
//! conservées telles quelles.

use crate::error::{EngineError, Result};

/// Longueurs maximales imposées par le format.
pub const MAX_NAME: usize = 64;
pub const MAX_DESCRIPTION: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDoc {
    pub name: String,
    pub description: String,
    /// Autres clés de l'en-tête, recopiées sans modification (lignes brutes).
    pub extra_frontmatter: Vec<String>,
    /// Corps Markdown, sans l'en-tête.
    pub body: String,
}

/// Analyse un SKILL.md. Erreur si l'en-tête, `name` ou `description` manque.
pub fn parse(text: &str) -> Result<SkillDoc> {
    let text = text.trim_start_matches('\u{feff}');
    let (front, body) = split_frontmatter(text)
        .ok_or_else(|| EngineError::Invalid("SKILL.md must start with a '---' YAML header".into()))?;

    let mut name = None;
    let mut description = None;
    let mut extra = Vec::new();
    let lines: Vec<&str> = front.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let top_level =
            !line.starts_with([' ', '\t']) && !line.trim().is_empty() && !line.trim_start().starts_with('#');
        let Some((key, raw)) = line.split_once(':').filter(|_| top_level) else {
            if !line.trim().is_empty() {
                extra.push(line.to_owned());
            }
            i += 1;
            continue;
        };
        let key = key.trim();
        let raw = raw.trim();
        // Lignes suivantes indentées : bloc YAML ou valeur imbriquée.
        let mut block = Vec::new();
        let mut j = i + 1;
        while j < lines.len() && (lines[j].starts_with([' ', '\t']) || lines[j].trim().is_empty()) {
            block.push(lines[j]);
            j += 1;
        }
        match key {
            "name" | "description" => {
                let value = scalar_value(raw, &block);
                if key == "name" {
                    name = Some(value);
                } else {
                    description = Some(value);
                }
            }
            _ => {
                extra.push(line.to_owned());
                extra.extend(block.iter().map(|l| l.to_string()));
            }
        }
        i = j;
    }
    while extra.last().is_some_and(|l| l.trim().is_empty()) {
        extra.pop();
    }

    let name =
        name.filter(|n| !n.is_empty()).ok_or_else(|| EngineError::Invalid("SKILL.md header has no 'name'".into()))?;
    let description = description
        .filter(|d| !d.is_empty())
        .ok_or_else(|| EngineError::Invalid("SKILL.md header has no 'description'".into()))?;
    Ok(SkillDoc { name, description, extra_frontmatter: extra, body: body.trim_start_matches(['\r', '\n']).to_owned() })
}

/// Sépare `---\n<yaml>\n---\n<corps>`.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---")?;
    let rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

/// Valeur scalaire YAML : guillemets, ou bloc `|` / `>` sur les lignes suivantes.
fn scalar_value(raw: &str, block: &[&str]) -> String {
    if raw.starts_with('|') || raw.starts_with('>') {
        let lines: Vec<&str> = block.iter().map(|l| l.trim()).collect();
        let sep = if raw.starts_with('>') { " " } else { "\n" };
        return lines.iter().filter(|l| !l.is_empty()).copied().collect::<Vec<_>>().join(sep);
    }
    let mut value = raw.to_owned();
    // Valeur simple poursuivie sur des lignes indentées (« plain scalar »).
    for l in block.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        value.push(' ');
        value.push_str(l);
    }
    unquote(&value)
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\")
    } else if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        v.to_owned()
    }
}

/// Valeur YAML sûre : entre guillemets doubles si nécessaire.
fn yaml_scalar(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value.contains([':', '#', '"', '\'', '\n', '[', ']', '{', '}', ',', '&', '*', '!', '|', '>', '%', '@', '`'])
        || value.starts_with([' ', '-', '?'])
        || value.ends_with(' ');
    if needs_quotes {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
    } else {
        value.to_owned()
    }
}

/// Écrit un SKILL.md complet.
pub fn render(doc: &SkillDoc) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {}\n", yaml_scalar(&doc.name)));
    out.push_str(&format!("description: {}\n", yaml_scalar(&doc.description)));
    for line in &doc.extra_frontmatter {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("---\n\n");
    out.push_str(doc.body.trim_start());
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Avertissements de conformité au format (non bloquants).
pub fn lint(doc: &SkillDoc) -> Vec<String> {
    let mut warnings = Vec::new();
    if doc.name.chars().count() > MAX_NAME {
        warnings.push(format!("name is longer than {MAX_NAME} characters"));
    }
    if !doc.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        warnings.push("name should only contain lowercase letters, digits and hyphens".into());
    }
    if doc.description.chars().count() > MAX_DESCRIPTION {
        warnings.push(format!("description is longer than {MAX_DESCRIPTION} characters"));
    }
    if doc.body.trim().is_empty() {
        warnings.push("the instructions (body) are empty".into());
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_body() {
        let text = "---\nname: pdf-tools\ndescription: \"Lire: et remplir des PDF\"\nlicense: MIT\nmetadata:\n  author: x\n---\n\n# PDF\nCorps.\n";
        let doc = parse(text).unwrap();
        assert_eq!(doc.name, "pdf-tools");
        assert_eq!(doc.description, "Lire: et remplir des PDF");
        assert_eq!(doc.extra_frontmatter, vec!["license: MIT", "metadata:", "  author: x"]);
        assert_eq!(doc.body, "# PDF\nCorps.\n");
    }

    #[test]
    fn folded_and_literal_blocks() {
        let text = "---\nname: x\ndescription: >\n  Première ligne\n  seconde ligne\n---\nB";
        assert_eq!(parse(text).unwrap().description, "Première ligne seconde ligne");
        let text = "---\nname: x\ndescription: |\n  a\n  b\n---\nB";
        assert_eq!(parse(text).unwrap().description, "a\nb");
    }

    #[test]
    fn missing_fields_are_errors() {
        assert!(parse("# pas d'en-tête").is_err());
        assert!(parse("---\nname: x\n---\nB").is_err());
        assert!(parse("---\ndescription: d\n---\nB").is_err());
        assert!(parse("---\nname: x\ndescription: d\n").is_err(), "en-tête non fermé");
    }

    #[test]
    fn render_round_trips() {
        let doc = SkillDoc {
            name: "revue-react".into(),
            description: "Revue: composants \"React\"".into(),
            extra_frontmatter: vec!["license: MIT".into()],
            body: "# Étapes\n1. Lire\n".into(),
        };
        let text = render(&doc);
        assert!(text.starts_with("---\nname: revue-react\ndescription: \"Revue: composants \\\"React\\\"\"\n"));
        assert_eq!(parse(&text).unwrap(), doc);
    }

    #[test]
    fn lint_reports_format_issues() {
        let doc =
            SkillDoc { name: "Bad Name".into(), description: "d".into(), extra_frontmatter: vec![], body: " ".into() };
        assert_eq!(lint(&doc).len(), 2);
    }
}
