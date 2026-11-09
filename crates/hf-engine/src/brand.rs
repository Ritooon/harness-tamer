//! Extraction d'une charte graphique à partir des fichiers importés :
//! couleurs (CSS, SCSS, JSON de design tokens, SVG), typographies (CSS, fichiers
//! de police), logos et consignes textuelles. Tout est local et déterministe.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::model::SCHEMA_VERSION;
use crate::text::truncate_chars;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BrandColor {
    pub name: Option<String>,
    /// Valeur hexadécimale normalisée `#rrggbb`.
    pub value: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BrandFont {
    pub family: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Guideline {
    pub file: String,
    pub excerpt: String,
}

/// Contenu de `brand/brand.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BrandKit {
    pub schema_version: u32,
    pub colors: Vec<BrandColor>,
    pub fonts: Vec<BrandFont>,
    pub logos: Vec<String>,
    pub guidelines: Vec<Guideline>,
    /// Fichiers présents mais non analysés (PDF, images…), cités tels quels.
    pub other_files: Vec<String>,
}

const MAX_COLORS: usize = 24;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

static CSS_VAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--([A-Za-z0-9_-]+)\s*:\s*(#[0-9A-Fa-f]{3,8})\b").unwrap());
static SCSS_VAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$([A-Za-z0-9_-]+)\s*:\s*(#[0-9A-Fa-f]{3,8})\b").unwrap());
static HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(?:[0-9A-Fa-f]{6}|[0-9A-Fa-f]{3})\b").unwrap());
static FONT_FAMILY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"font-family\s*:\s*([^;}\n]+)").unwrap());

/// Analyse les fichiers `files` (noms relatifs à `brand_dir`).
pub fn extract_brand_kit(brand_dir: &Path, files: &[String]) -> BrandKit {
    let mut kit = BrandKit {
        schema_version: SCHEMA_VERSION,
        colors: vec![],
        fonts: vec![],
        logos: vec![],
        guidelines: vec![],
        other_files: vec![],
    };
    for file in files {
        let path = brand_dir.join(file);
        let ext = Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let lower = file.to_ascii_lowercase();
        let read = || -> Option<String> {
            let meta = std::fs::metadata(&path).ok()?;
            (meta.len() <= MAX_FILE_BYTES).then(|| std::fs::read_to_string(&path).ok()).flatten()
        };
        match ext.as_str() {
            "css" | "scss" | "sass" | "less" => {
                if let Some(text) = read() {
                    colors_from_css(&text, file, &mut kit.colors);
                    fonts_from_css(&text, file, &mut kit.fonts);
                }
            }
            "json" => {
                if let Some(json) = read().and_then(|t| serde_json::from_str::<Json>(&t).ok()) {
                    colors_from_json(&json, "", file, &mut kit.colors);
                }
            }
            "svg" => {
                if lower.contains("logo") || lower.contains("brand") || lower.contains("mark") {
                    kit.logos.push(file.clone());
                }
                if let Some(text) = read() {
                    unnamed_colors(&text, file, &mut kit.colors);
                }
            }
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "ico" => {
                if lower.contains("logo") || lower.contains("brand") || lower.contains("icon") {
                    kit.logos.push(file.clone());
                } else {
                    kit.other_files.push(file.clone());
                }
            }
            "ttf" | "otf" | "woff" | "woff2" => {
                let family = font_family_from_file(file);
                push_font(&mut kit.fonts, family, file);
            }
            "md" | "txt" | "markdown" => {
                if let Some(text) = read() {
                    unnamed_colors(&text, file, &mut kit.colors);
                    kit.guidelines.push(Guideline { file: file.clone(), excerpt: truncate_chars(text.trim(), 2000) });
                }
            }
            _ => kit.other_files.push(file.clone()),
        }
    }
    kit.colors.truncate(MAX_COLORS);
    kit
}

fn normalize_hex(raw: &str) -> String {
    let hex = raw.trim_start_matches('#').to_ascii_lowercase();
    let hex =
        if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex[..hex.len().min(6)].to_owned() };
    format!("#{hex}")
}

fn push_color(colors: &mut Vec<BrandColor>, name: Option<String>, raw: &str, source: &str) {
    let value = normalize_hex(raw);
    if value.len() != 7 {
        return;
    }
    // Une couleur nommée remplace la même valeur trouvée sans nom.
    if let Some(existing) = colors.iter_mut().find(|c| c.value == value) {
        if existing.name.is_none() && name.is_some() {
            existing.name = name;
            existing.source = source.to_owned();
        }
        return;
    }
    colors.push(BrandColor { name, value, source: source.to_owned() });
}

fn colors_from_css(text: &str, source: &str, colors: &mut Vec<BrandColor>) {
    for c in CSS_VAR.captures_iter(text).chain(SCSS_VAR.captures_iter(text)) {
        push_color(colors, Some(c[1].to_owned()), &c[2], source);
    }
    unnamed_colors(text, source, colors);
}

fn unnamed_colors(text: &str, source: &str, colors: &mut Vec<BrandColor>) {
    for m in HEX.find_iter(text) {
        push_color(colors, None, m.as_str(), source);
    }
}

/// Design tokens JSON : chaque valeur hexadécimale prend le chemin de ses clés pour nom.
fn colors_from_json(json: &Json, prefix: &str, source: &str, colors: &mut Vec<BrandColor>) {
    match json {
        Json::Object(map) => {
            for (key, value) in map {
                // Format W3C des design tokens : { "$value": "#…" }.
                let name = if key == "$value" || key == "value" {
                    prefix.to_owned()
                } else if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                colors_from_json(value, &name, source, colors);
            }
        }
        Json::String(s) if HEX.is_match(s) && s.trim().starts_with('#') => {
            push_color(colors, (!prefix.is_empty()).then(|| prefix.to_owned()), s.trim(), source);
        }
        _ => {}
    }
}

fn fonts_from_css(text: &str, source: &str, fonts: &mut Vec<BrandFont>) {
    const GENERIC: &[&str] = &["serif", "sans-serif", "monospace", "system-ui", "inherit", "cursive", "fantasy"];
    for c in FONT_FAMILY.captures_iter(text) {
        let first = c[1].split(',').next().unwrap_or("").trim().trim_matches(|ch| ch == '"' || ch == '\'');
        if !first.is_empty() && !first.starts_with("var(") && !GENERIC.contains(&first) {
            push_font(fonts, first.to_owned(), source);
        }
    }
}

/// `Inter-SemiBold.woff2` → `Inter`.
fn font_family_from_file(file: &str) -> String {
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
    stem.split(['-', '_']).next().unwrap_or(stem).to_owned()
}

fn push_font(fonts: &mut Vec<BrandFont>, family: String, source: &str) {
    let known: BTreeSet<String> = fonts.iter().map(|f| f.family.to_lowercase()).collect();
    if !known.contains(&family.to_lowercase()) {
        fonts.push(BrandFont { family, source: source.to_owned() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_colors_fonts_logos_and_guidelines() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(
            d.join("theme.css"),
            ":root { --brand-primary: #0055FF; --brand-accent: #f60; }\nbody { font-family: \"Inter\", sans-serif; color: #0055ff; }",
        )
        .unwrap();
        std::fs::write(d.join("tokens.json"), r##"{"color": {"neutral": {"900": {"$value": "#111827"}}}}"##).unwrap();
        std::fs::write(d.join("logo.svg"), "<svg><path fill=\"#FF0000\"/></svg>").unwrap();
        std::fs::write(d.join("guide.md"), "Toujours écrire « nous ».").unwrap();
        std::fs::write(d.join("Inter-Bold.woff2"), [0u8; 4]).unwrap();
        std::fs::write(d.join("charte.pdf"), [0u8; 4]).unwrap();
        let files: Vec<String> = ["theme.css", "tokens.json", "logo.svg", "guide.md", "Inter-Bold.woff2", "charte.pdf"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let kit = extract_brand_kit(d, &files);
        let values: Vec<(&str, Option<&str>)> =
            kit.colors.iter().map(|c| (c.value.as_str(), c.name.as_deref())).collect();
        assert!(values.contains(&("#0055ff", Some("brand-primary"))));
        assert!(values.contains(&("#ff6600", Some("brand-accent"))));
        assert!(values.contains(&("#111827", Some("color.neutral.900"))));
        assert!(values.contains(&("#ff0000", None)));
        assert_eq!(kit.colors.iter().filter(|c| c.value == "#0055ff").count(), 1, "pas de doublon");
        assert_eq!(kit.fonts.len(), 1, "Inter vu dans le CSS et la police");
        assert_eq!(kit.logos, vec!["logo.svg"]);
        assert_eq!(kit.guidelines[0].file, "guide.md");
        assert_eq!(kit.other_files, vec!["charte.pdf"]);
    }
}
