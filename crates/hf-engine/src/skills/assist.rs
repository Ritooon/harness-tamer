//! Création assistée et adaptation de skills par le modèle local.
//!
//! Le résultat n'est jamais enregistré directement : il est rendu à
//! l'interface en aperçu, l'utilisateur le relit puis l'enregistre.

use serde::{Deserialize, Serialize};

use super::skill_md::{self, SkillDoc};
use crate::error::{EngineError, Result};
use crate::generation::context::ContextBlocks;
use crate::generation::prompts::{Template, render, template};
use crate::llm::{Cancel, ChatBackend, ChatMessage, ChatRequest, EventSink};
use crate::model::{Lang, Skill, SkillFile};
use crate::store::safe_relative;
use crate::text::slugify;

const MAX_TOKENS: u32 = 4_096;
const FILE_OPEN: &str = "<<<FILE ";
const FILE_CLOSE: &str = "FILE>>>";

/// Skill proposé par le modèle, avant enregistrement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SkillProposal {
    pub content: String,
    pub files: Vec<SkillFile>,
    /// Avertissements de format (nom, longueur de la description…).
    pub warnings: Vec<String>,
}

/// Squelette d'un skill vide, pour une création sans modèle.
pub fn blank_skill(name: &str, description: &str, lang: Lang) -> String {
    let name = slugify(name);
    let body = match lang {
        Lang::Fr => "# Titre\n\n## Quand l'utiliser\n\n- …\n\n## Étapes\n\n1. …\n\n## Vérifications\n\n- …\n",
        Lang::En => "# Title\n\n## When to use\n\n- …\n\n## Steps\n\n1. …\n\n## Checks\n\n- …\n",
    };
    skill_md::render(&SkillDoc {
        name: if name.is_empty() { "new-skill".into() } else { name },
        description: description.trim().to_owned(),
        extra_frontmatter: vec![],
        body: body.into(),
    })
}

/// Sépare le SKILL.md des blocs `<<<FILE chemin … FILE>>>`, valide l'en-tête
/// et normalise le nom en kebab-case.
pub fn parse_model_output(raw: &str) -> Result<SkillProposal> {
    let mut text = raw.trim();
    for fence in ["```markdown\n", "```md\n", "```yaml\n", "```\n"] {
        if let Some(inner) = text.strip_prefix(fence) {
            text = inner.trim_end().strip_suffix("```").unwrap_or(inner).trim();
            break;
        }
    }
    // Texte éventuel avant l'en-tête : ignoré.
    let start = if text.starts_with("---") {
        0
    } else {
        text.find("\n---\n").map(|i| i + 1).ok_or_else(|| EngineError::BadModelOutput("no SKILL.md header".into()))?
    };
    let text = &text[start..];
    let (skill_part, files) = match text.find(FILE_OPEN) {
        Some(i) => (&text[..i], parse_files(&text[i..])?),
        None => (text, vec![]),
    };
    let mut doc = skill_md::parse(skill_part).map_err(|e| EngineError::BadModelOutput(e.to_string()))?;
    let slug = slugify(&doc.name);
    if !slug.is_empty() {
        doc.name = slug;
    }
    let warnings = skill_md::lint(&doc);
    Ok(SkillProposal { content: skill_md::render(&doc), files, warnings })
}

fn parse_files(mut text: &str) -> Result<Vec<SkillFile>> {
    let mut files: Vec<SkillFile> = Vec::new();
    while let Some(start) = text.find(FILE_OPEN) {
        let after = &text[start + FILE_OPEN.len()..];
        let (path, rest) = after.split_once('\n').unwrap_or((after, ""));
        let path = path.trim().to_owned();
        let (content, next) = match rest.find(FILE_CLOSE) {
            Some(end) => (&rest[..end], &rest[end + FILE_CLOSE.len()..]),
            None => (rest, ""),
        };
        safe_relative(&path).map_err(|_| EngineError::BadModelOutput(format!("unsafe file path: {path}")))?;
        if path != "SKILL.md" && !files.iter().any(|f| f.path == path) {
            files.push(SkillFile { path, content: content.trim_end().to_owned() + "\n" });
        }
        text = next;
    }
    Ok(files)
}

async fn ask(
    backend: &dyn ChatBackend,
    lang: Lang,
    user: String,
    on_event: EventSink<'_>,
    cancel: &Cancel,
) -> Result<SkillProposal> {
    let request = ChatRequest {
        messages: vec![ChatMessage::system(template(Template::SkillsSystem, lang).trim()), ChatMessage::user(user)],
        max_tokens: MAX_TOKENS,
        temperature: 0.4,
        thinking: false,
    };
    let output = backend.chat(request, on_event, cancel).await?;
    parse_model_output(&output.content)
}

/// Crée un skill à partir d'une demande en langage naturel, éventuellement
/// dans le contexte d'un projet.
pub async fn propose_skill(
    backend: &dyn ChatBackend,
    request: &str,
    context: Option<&ContextBlocks>,
    lang: Lang,
    on_event: EventSink<'_>,
    cancel: &Cancel,
) -> Result<SkillProposal> {
    if request.trim().is_empty() {
        return Err(EngineError::Invalid("the skill request is empty".into()));
    }
    let context =
        context.map(|c| format!("\n{}\n", c.for_needs(crate::generation::sections::SectionId::Commands.needs())));
    let user = render(
        template(Template::SkillsCreate, lang),
        &[("request", request.trim()), ("context", context.as_deref().unwrap_or(""))],
    );
    ask(backend, lang, user, on_event, cancel).await
}

/// Adapte un skill (importé ou de la bibliothèque) à un projet.
pub async fn adapt_skill(
    backend: &dyn ChatBackend,
    skill: &Skill,
    context: &ContextBlocks,
    instruction: Option<&str>,
    lang: Lang,
    on_event: EventSink<'_>,
    cancel: &Cancel,
) -> Result<SkillProposal> {
    let mut skill_text = skill.content.clone();
    for f in &skill.files {
        skill_text.push_str(&format!("\n{FILE_OPEN}{}\n{}\n{FILE_CLOSE}\n", f.path, f.content.trim_end()));
    }
    let instruction = instruction.map(str::trim).filter(|i| !i.is_empty()).map(|i| match lang {
        Lang::Fr => format!("\n# Consigne de l'utilisateur\n\n{i}\n"),
        Lang::En => format!("\n# User instruction\n\n{i}\n"),
    });
    let user = render(
        template(Template::SkillsAdapt, lang),
        &[
            ("context", &context.for_needs(crate::generation::sections::SectionId::Commands.needs())),
            ("skill", &skill_text),
            ("instruction", instruction.as_deref().unwrap_or("")),
        ],
    );
    let mut proposal = ask(backend, lang, user, on_event, cancel).await?;
    // Le nom du skill est conservé, même si le modèle l'a changé.
    let mut doc = skill_md::parse(&proposal.content)?;
    if let Ok(original) = skill_md::parse(&skill.content) {
        doc.name = original.name;
        proposal.content = skill_md::render(&doc);
    }
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::scripted::ScriptedBackend;

    #[test]
    fn model_output_with_files_and_noise() {
        let raw = "Voici le skill :\n---\nname: Revue React\ndescription: Relit des composants. À utiliser quand une PR touche src/components.\n---\n\n# Revue\n1. Lire\n<<<FILE scripts/check.sh\n#!/bin/sh\npnpm lint\nFILE>>>\n<<<FILE ../evil\nx\nFILE>>>";
        assert!(parse_model_output(raw).is_err(), "chemin dangereux refusé");
        let ok = raw.split("<<<FILE ../evil").next().unwrap();
        let p = parse_model_output(ok).unwrap();
        assert!(p.content.starts_with("---\nname: revue-react\n"));
        assert_eq!(
            p.files,
            vec![SkillFile { path: "scripts/check.sh".into(), content: "#!/bin/sh\npnpm lint\n".into() }]
        );
        assert!(p.warnings.is_empty());
        assert!(parse_model_output("pas de skill").is_err());
    }

    #[test]
    fn blank_skill_is_valid() {
        let text = blank_skill("Mon Skill", "Fait X. À utiliser quand Y.", Lang::Fr);
        let doc = skill_md::parse(&text).unwrap();
        assert_eq!(doc.name, "mon-skill");
    }

    #[tokio::test]
    async fn adaptation_keeps_the_original_name() {
        let backend = ScriptedBackend::new(|req: &ChatRequest| {
            assert!(req.messages[1].content.contains("<<<FILE ref.md"));
            Ok("---\nname: autre-nom\ndescription: Adapté. À utiliser quand X.\n---\n# Adapté\n".into())
        });
        let skill = Skill {
            meta: crate::model::SkillMeta {
                schema_version: 1,
                slug: "pdf".into(),
                name: "pdf".into(),
                description: "d".into(),
                origin: crate::model::SkillOrigin::Created,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            content: "---\nname: pdf\ndescription: d\n---\nCorps".into(),
            files: vec![SkillFile { path: "ref.md".into(), content: "r".into() }],
        };
        let ctx = ContextBlocks { project: "# Projet".into(), repo: None, brand: None, skills: None };
        let p = adapt_skill(&backend, &skill, &ctx, None, Lang::Fr, &|_| {}, &Cancel::new()).await.unwrap();
        assert!(p.content.starts_with("---\nname: pdf\n"));
    }
}
