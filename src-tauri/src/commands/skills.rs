//! Skills : bibliothèque, création assistée, rattachement aux projets,
//! adaptation, recherche et import en ligne (sous consentement).

use hf_engine::generation::{context, ensure_brand_kit, ensure_repo_profile};
use hf_engine::llm::StreamEvent;
use hf_engine::model::{Lang, Skill, SkillFile, SkillMeta, SkillOrigin};
use hf_engine::skills::assist::{SkillProposal, adapt_skill, blank_skill, propose_skill};
use hf_engine::skills::online::{self, ImportedSkill, OnlineSkill};
use hf_engine::store::{SkillDraft, SystemTrash};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::consent::{confirm, current_lang};
use crate::error::CmdResult;
use crate::state::AppState;

/// Fragment de texte produit par le modèle, relayé à l'aperçu.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextEvent {
    Reasoning { text: String },
    Token { text: String },
}

fn relay(channel: &Channel<TextEvent>) -> impl Fn(StreamEvent<'_>) + Send + Sync + '_ {
    move |event| {
        let _ = channel.send(match event {
            StreamEvent::Reasoning(t) => TextEvent::Reasoning { text: t.to_owned() },
            StreamEvent::Content(t) => TextEvent::Token { text: t.to_owned() },
        });
    }
}

#[tauri::command]
pub fn list_skills(state: State<'_, AppState>) -> CmdResult<Vec<SkillMeta>> {
    Ok(state.store.list_skills()?)
}

#[tauri::command]
pub fn get_skill(state: State<'_, AppState>, slug: String) -> CmdResult<Skill> {
    Ok(state.store.get_skill(&slug)?)
}

/// Squelette à remplir à la main (sans modèle).
#[tauri::command]
pub fn blank_skill_template(state: State<'_, AppState>, name: String, description: String) -> String {
    blank_skill(&name, &description, state.settings().default_output_language)
}

/// Enregistre un skill (créé à la main, proposé par le modèle ou importé).
#[tauri::command]
pub fn save_new_skill(
    state: State<'_, AppState>,
    content: String,
    files: Vec<SkillFile>,
    origin: SkillOrigin,
) -> CmdResult<Skill> {
    Ok(state.store.create_skill(SkillDraft { content, files, origin })?)
}

#[tauri::command]
pub fn update_skill(
    state: State<'_, AppState>,
    slug: String,
    content: String,
    files: Vec<SkillFile>,
) -> CmdResult<Skill> {
    Ok(state.store.update_skill(&slug, content, files)?)
}

#[tauri::command]
pub async fn delete_skill(app: AppHandle, state: State<'_, AppState>, slug: String) -> CmdResult<bool> {
    let skill = state.store.get_skill(&slug)?;
    let lang = current_lang(&state.lang);
    let (title, message, label) = match lang {
        Lang::Fr => (
            "Supprimer le skill ?",
            format!(
                "Le skill « {} » sera envoyé à la corbeille. Les copies rattachées aux projets sont conservées.",
                skill.meta.name
            ),
            "Supprimer",
        ),
        Lang::En => (
            "Delete the skill?",
            format!(
                "The \"{}\" skill will be moved to the trash. Copies attached to projects are kept.",
                skill.meta.name
            ),
            "Delete",
        ),
    };
    let confirmed = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || confirm(&app, lang, title, &message, label)).await?
    };
    if confirmed {
        state.store.delete_skill(&slug, &SystemTrash)?;
    }
    Ok(confirmed)
}

/// Propose un skill à partir d'une demande, dans le contexte d'un projet si fourni.
#[tauri::command]
pub async fn propose_new_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    request: String,
    project_id: Option<String>,
    on_event: Channel<TextEvent>,
) -> CmdResult<SkillProposal> {
    let settings = state.settings();
    let (ctx, lang) = match &project_id {
        Some(id) => {
            let (ctx, lang) = project_context(&state, id).await?;
            (Some(ctx), lang)
        }
        None => (None, settings.default_output_language),
    };
    let backend = state.runtime.backend(&app, &state.store, &settings).await?;
    let cancel = state.begin_cancellable("skill:propose");
    let sink = relay(&on_event);
    let result = propose_skill(backend.as_ref(), &request, ctx.as_ref(), lang, &sink, &cancel).await;
    state.end_cancellable("skill:propose");
    Ok(result?)
}

async fn project_context(state: &AppState, id: &str) -> CmdResult<(context::ContextBlocks, Lang)> {
    let project = state.store.get_project(id)?;
    let profile = ensure_repo_profile(&state.store, &project, false).await?;
    let brand = ensure_brand_kit(&state.store, &project, false)?;
    let lang = project.output_language;
    Ok((context::build(&project, profile.as_ref(), brand.as_ref(), &[], lang), lang))
}

#[tauri::command]
pub fn list_project_skills(state: State<'_, AppState>, id: String) -> CmdResult<Vec<Skill>> {
    Ok(state.store.list_project_skills(&id)?)
}

#[tauri::command]
pub fn attach_skill(state: State<'_, AppState>, id: String, slug: String) -> CmdResult<Skill> {
    Ok(state.store.attach_skill(&id, &slug)?)
}

#[tauri::command]
pub fn detach_skill(state: State<'_, AppState>, id: String, slug: String) -> CmdResult<()> {
    Ok(state.store.detach_skill(&id, &slug)?)
}

#[tauri::command]
pub fn update_project_skill(
    state: State<'_, AppState>,
    id: String,
    slug: String,
    content: String,
    files: Vec<SkillFile>,
    adapted: bool,
) -> CmdResult<Skill> {
    let origin = adapted.then(|| SkillOrigin::Adapted { from: slug.clone() });
    Ok(state.store.update_project_skill(&id, &slug, content, files, origin)?)
}

/// Propose une adaptation d'un skill rattaché au projet (aperçu, non enregistré).
#[tauri::command]
pub async fn adapt_project_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    slug: String,
    instruction: Option<String>,
    on_event: Channel<TextEvent>,
) -> CmdResult<SkillProposal> {
    let skill = state.store.get_project_skill(&id, &slug)?;
    let (ctx, lang) = project_context(&state, &id).await?;
    let backend = state.runtime.backend(&app, &state.store, &state.settings()).await?;
    let op = format!("skill:adapt:{id}:{slug}");
    let cancel = state.begin_cancellable(&op);
    let sink = relay(&on_event);
    let result = adapt_skill(backend.as_ref(), &skill, &ctx, instruction.as_deref(), lang, &sink, &cancel).await;
    state.end_cancellable(&op);
    Ok(result?)
}

/// Recherche en ligne : un dialogue de consentement, puis filtre local.
#[tauri::command]
pub async fn search_online_skills(state: State<'_, AppState>, query: String) -> CmdResult<Vec<OnlineSkill>> {
    let settings = state.settings();
    Ok(online::search(&state.gated, &settings.skill_sources, &query, settings.ui_language).await?)
}

/// Import d'un résultat de recherche : un nouveau dialogue de consentement.
#[tauri::command]
pub async fn import_online_skill(state: State<'_, AppState>, found: OnlineSkill) -> CmdResult<ImportedSkill> {
    let settings = state.settings();
    Ok(online::import(&state.gated, &settings.skill_sources, &found, settings.ui_language).await?)
}
