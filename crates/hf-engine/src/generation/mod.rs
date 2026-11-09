//! Génération de l'AGENTS.md, section par section (ADR 0002) :
//!
//! 1. profil du dépôt et charte (calculés s'ils manquent) ;
//! 2. contexte compact ([`context`]) ;
//! 3. un appel au modèle par section ([`sections`], [`prompts`]) ;
//! 4. nettoyage de la réponse et assemblage avec marqueurs ([`document`]) ;
//! 5. contrôles ([`checks`]) puis nouvelle version dans l'historique.
//!
//! La régénération partielle suit exactement le même chemin, pour les
//! seules sections demandées, avec le texte actuel et une consigne
//! facultative de l'utilisateur.

pub mod checks;
pub mod context;
pub mod document;
pub mod prompts;
pub mod sections;

use serde::{Deserialize, Serialize};

use self::checks::{Issue, IssueKind, check_document};
use self::document::{AgentsDoc, section_body};
use self::prompts::{Template, render, template};
use self::sections::SectionId;
use crate::brand::{BrandKit, extract_brand_kit};
use crate::error::{EngineError, Result};
use crate::llm::{Cancel, ChatBackend, ChatMessage, ChatRequest, StreamEvent};
use crate::model::{Lang, Project, VersionMeta, VersionOrigin};
use crate::scan::{RepoProfile, ScanOptions, scan_repository};
use crate::store::{Store, VersionDraft};

/// Tokens réservés à la réflexion, en plus de la réponse.
const THINKING_BUDGET: u32 = 3_072;

/// Événements relayés à l'interface pendant la génération.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GenEvent {
    Preparing,
    SectionStarted {
        section: SectionId,
        index: usize,
        total: usize,
    },
    Reasoning {
        section: SectionId,
        text: String,
    },
    Token {
        section: SectionId,
        text: String,
    },
    /// Nouvel essai sans réflexion (réponse vide ou coupée).
    Retrying {
        section: SectionId,
    },
    SectionDone {
        section: SectionId,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GenerateOptions {
    /// Sections à (re)générer ; `None` = document complet.
    pub sections: Option<Vec<SectionId>>,
    /// Consigne libre de l'utilisateur (régénération partielle).
    pub instruction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GenerationResult {
    pub version: VersionMeta,
    pub content: String,
    pub issues: Vec<Issue>,
}

/// Profil du dépôt : celui enregistré, sinon une nouvelle analyse.
pub async fn ensure_repo_profile(store: &Store, project: &Project, rescan: bool) -> Result<Option<RepoProfile>> {
    if !project.existing_code.enabled {
        return Ok(None);
    }
    if !rescan && let Some(profile) = store.load_repo_profile(&project.id)? {
        return Ok(Some(profile));
    }
    let path = project.existing_code.repo_path.clone().ok_or_else(|| EngineError::RepoNotFound(String::new()))?;
    // L'analyse lit le disque : hors du fil d'exécution asynchrone.
    let profile =
        tokio::task::spawn_blocking(move || scan_repository(std::path::Path::new(&path), &ScanOptions::default()))
            .await
            .map_err(|e| EngineError::Runtime(e.to_string()))??;
    store.save_repo_profile(&project.id, &profile)?;
    Ok(Some(profile))
}

/// Charte : celle enregistrée, sinon extraite des fichiers importés.
pub fn ensure_brand_kit(store: &Store, project: &Project, refresh: bool) -> Result<Option<BrandKit>> {
    if !project.brand.enabled || project.brand.files.is_empty() {
        return Ok(None);
    }
    if !refresh && let Some(kit) = store.load_brand_kit(&project.id)? {
        return Ok(Some(kit));
    }
    let kit = extract_brand_kit(&store.brand_dir(&project.id)?, &project.brand.files);
    store.save_brand_kit(&project.id, &kit)?;
    Ok(Some(kit))
}

fn preamble(project: &Project, lang: Lang) -> String {
    let intro = match lang {
        Lang::Fr => {
            "Instructions pour les agents de code qui travaillent sur ce projet. Document généré localement par HarnessForge : relisez-le avant de le publier."
        }
        Lang::En => {
            "Instructions for coding agents working on this project. Generated locally by HarnessForge: review it before publishing."
        }
    };
    format!("# {}\n\n{intro}", project.name)
}

/// Nettoie la réponse du modèle : bloc ```markdown englobant, titre
/// répété, titres de niveau 1 ou 2 ramenés au niveau 3.
pub fn clean_section_output(raw: &str, title: &str) -> String {
    let mut text = raw.trim().to_owned();
    let explicit = text.strip_prefix("```markdown\n").or_else(|| text.strip_prefix("```md\n"));
    if let Some(inner) = explicit.and_then(|rest| rest.trim_end().strip_suffix("```")) {
        text = inner.trim().to_owned();
    } else if let Some(inner) = text.strip_prefix("```\n").and_then(|rest| rest.trim_end().strip_suffix("```"))
        // Un bloc ``` nu n'est retiré que s'il ne contient pas d'autre bloc.
        && !inner.contains("```")
    {
        text = inner.trim().to_owned();
    }
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if let Some(first) = lines.first() {
        let heading = first.trim_start_matches('#').trim();
        if first.starts_with('#') && heading.eq_ignore_ascii_case(title) {
            lines.remove(0);
        }
    }
    let mut in_code = false;
    for line in &mut lines {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if !in_code && (line.starts_with("# ") || line.starts_with("## ")) {
            *line = format!("### {}", line.trim_start_matches('#').trim());
        }
    }
    lines.join("\n").trim().to_owned()
}

fn user_message(
    lang: Lang,
    section: SectionId,
    context: &str,
    current: Option<&str>,
    instruction: Option<&str>,
) -> String {
    let current = current.filter(|c| !c.trim().is_empty()).map(|c| match lang {
        Lang::Fr => format!("\n# Version actuelle de la section (à améliorer)\n\n{c}\n"),
        Lang::En => format!("\n# Current version of the section (to improve)\n\n{c}\n"),
    });
    let instruction = instruction.map(str::trim).filter(|i| !i.is_empty()).map(|i| match lang {
        Lang::Fr => format!("\n# Consigne de l'utilisateur\n\n{i}\n"),
        Lang::En => format!("\n# User instruction\n\n{i}\n"),
    });
    render(
        template(Template::AgentsUser, lang),
        &[
            ("context", context),
            ("title", section.title(lang)),
            ("task", template(Template::Section(section), lang).trim()),
            ("current", current.as_deref().unwrap_or("")),
            ("instruction", instruction.as_deref().unwrap_or("")),
        ],
    )
}

/// Génère (ou régénère en partie) l'AGENTS.md d'un projet et l'enregistre
/// comme nouvelle version. Rien n'est enregistré en cas d'annulation.
pub async fn generate(
    store: &Store,
    backend: &dyn ChatBackend,
    project_id: &str,
    options: GenerateOptions,
    on_event: &(dyn Fn(GenEvent) + Send + Sync),
    cancel: &Cancel,
) -> Result<GenerationResult> {
    on_event(GenEvent::Preparing);
    let project = store.get_project(project_id)?;
    let lang = project.output_language;
    let profile = ensure_repo_profile(store, &project, false).await?;
    let brand = ensure_brand_kit(store, &project, false)?;
    let skills: Vec<_> = store.list_project_skills(project_id)?.into_iter().map(|s| s.meta).collect();
    let ctx = context::build(&project, profile.as_ref(), brand.as_ref(), &skills, lang);

    let mut doc = store.load_agents(project_id)?.map(|t| AgentsDoc::parse(&t)).unwrap_or_default();
    doc.ensure_preamble(&preamble(&project, lang));

    let partial = options.sections.is_some();
    let mut plan = options.sections.unwrap_or_else(|| SectionId::ALL.to_vec());
    plan.sort();
    plan.dedup();
    if plan.is_empty() {
        return Err(EngineError::Invalid("no section to generate".into()));
    }
    let system = template(Template::AgentsSystem, lang).trim().to_owned();
    let mut truncated = Vec::new();

    for (index, section) in plan.iter().copied().enumerate() {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        on_event(GenEvent::SectionStarted { section, index, total: plan.len() });
        let current = partial.then(|| doc.get(section.as_str()).map(section_body)).flatten();
        let user = user_message(
            lang,
            section,
            &ctx.for_needs(section.needs()),
            current,
            options.instruction.as_deref().filter(|_| partial),
        );
        let sink = |event: StreamEvent<'_>| match event {
            StreamEvent::Reasoning(text) => on_event(GenEvent::Reasoning { section, text: text.to_owned() }),
            StreamEvent::Content(text) => on_event(GenEvent::Token { section, text: text.to_owned() }),
        };
        let mut thinking = section.thinking();
        let mut attempt = 0;
        let output = loop {
            let request = ChatRequest {
                messages: vec![ChatMessage::system(system.clone()), ChatMessage::user(user.clone())],
                max_tokens: section.max_answer_tokens() + if thinking { THINKING_BUDGET } else { 0 },
                temperature: if thinking { 0.6 } else { 0.4 },
                thinking,
            };
            let output = backend.chat(request, &sink, cancel).await?;
            let unusable = output.content.trim().is_empty() || (thinking && output.truncated());
            if unusable && attempt == 0 {
                // Une réflexion trop longue a pu consommer tout le budget.
                attempt += 1;
                thinking = false;
                on_event(GenEvent::Retrying { section });
                continue;
            }
            break output;
        };
        if output.content.trim().is_empty() {
            return Err(EngineError::BadModelOutput(format!("empty answer for section {}", section.as_str())));
        }
        if output.truncated() {
            truncated.push(section);
        }
        let title = section.title(lang);
        let body = clean_section_output(&output.content, title);
        doc.set_section(section, format!("## {title}\n\n{body}"));
        on_event(GenEvent::SectionDone { section });
    }

    let content = doc.render();
    let mut issues = check_document(&doc, &SectionId::ALL, profile.as_ref());
    issues.extend(truncated.iter().map(|s| Issue {
        kind: IssueKind::Truncated,
        section: Some(s.as_str().to_owned()),
        detail: String::new(),
    }));
    let version = store.save_agents_version(
        project_id,
        &content,
        VersionDraft {
            origin: Some(if partial { VersionOrigin::SectionRegeneration } else { VersionOrigin::Generation }),
            model: Some(backend.model_label()),
            sections: plan.iter().map(|s| s.as_str().to_owned()).collect(),
            restored_from: None,
            note: options.instruction.filter(|i| partial && !i.trim().is_empty()),
        },
    )?;
    Ok(GenerationResult { version, content, issues })
}

/// Contrôles du document courant (après une édition manuelle par exemple).
pub fn check_project(store: &Store, project_id: &str) -> Result<Vec<Issue>> {
    let Some(text) = store.load_agents(project_id)? else { return Ok(vec![]) };
    let profile = store.load_repo_profile(project_id)?;
    Ok(check_document(&AgentsDoc::parse(&text), &SectionId::ALL, profile.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_output_is_cleaned() {
        let raw = "```markdown\n## Workflow Git\n\n# Branches\n- `main`\n```bash\n# commentaire\n```\n```";
        assert_eq!(clean_section_output(raw, "Workflow Git"), "### Branches\n- `main`\n```bash\n# commentaire\n```");
        assert_eq!(clean_section_output("Texte simple", "T"), "Texte simple");
    }
}
