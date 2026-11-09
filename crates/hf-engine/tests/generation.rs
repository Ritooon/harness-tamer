//! Génération de bout en bout avec le moteur factice : dix sections,
//! régénération partielle, nouvel essai sans réflexion, annulation.

use std::sync::Mutex;

use hf_engine::generation::sections::SectionId;
use hf_engine::generation::{GenEvent, GenerateOptions, generate};
use hf_engine::llm::scripted::ScriptedBackend;
use hf_engine::llm::{Cancel, ChatRequest};
use hf_engine::model::{ExistingCode, Lang, Platform, ProjectInput, TargetOs, VersionOrigin};
use hf_engine::store::Store;

fn input() -> ProjectInput {
    ProjectInput {
        name: "Atelier".into(),
        description: "Application de gestion d'ateliers.".into(),
        platform: Platform::Web,
        target_os: vec![TargetOs::Linux],
        imposed_tech: vec!["React".into(), "Rust".into()],
        existing_code: ExistingCode::default(),
        brand_enabled: false,
        constraints: "Aucune dépendance sous licence GPL.".into(),
        output_language: Lang::Fr,
    }
}

/// Répond par le titre de la section demandée, repris dans le prompt.
fn echo_title(req: &ChatRequest) -> hf_engine::Result<String> {
    let user = &req.messages[1].content;
    let title = user.split("# Section à rédiger : ").nth(1).and_then(|r| r.lines().next()).unwrap_or("?");
    Ok(format!("## {title}\n\nContenu de la section « {title} », suffisamment long pour les contrôles."))
}

#[tokio::test]
async fn full_then_partial_generation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let project = store.create_project(input()).unwrap();
    let backend = ScriptedBackend::new(echo_title);
    let events = Mutex::new(Vec::new());
    let on_event = |e: GenEvent| events.lock().unwrap().push(e);

    let result =
        generate(&store, &backend, &project.id, GenerateOptions::default(), &on_event, &Cancel::new()).await.unwrap();
    assert_eq!(result.version.number, 1);
    assert_eq!(result.version.origin, VersionOrigin::Generation);
    assert_eq!(result.version.model.as_deref(), Some("scripted"));
    assert!(result.issues.is_empty(), "{:?}", result.issues);
    assert!(result.content.starts_with("# Atelier\n\n"));
    for s in SectionId::ALL {
        assert!(result.content.contains(&format!(
            "<!-- hf:section id=\"{}\" -->\n## {}\n\n",
            s.as_str(),
            s.title(Lang::Fr)
        )));
    }
    // Le titre répété par le modèle est retiré.
    assert_eq!(result.content.matches("## Workflow Git").count(), 1);

    let requests = backend.requests();
    assert_eq!(requests.len(), 10);
    let thinking: Vec<bool> = requests.iter().map(|r| r.thinking).collect();
    assert_eq!(thinking.iter().filter(|t| **t).count(), 4);
    assert!(requests[0].messages[1].content.contains("Aucune dépendance sous licence GPL."));
    let started = events.lock().unwrap().iter().filter(|e| matches!(e, GenEvent::SectionStarted { .. })).count();
    assert_eq!(started, 10);

    // Régénération d'une seule section, avec consigne.
    let options =
        GenerateOptions { sections: Some(vec![SectionId::Git]), instruction: Some("Branches en anglais.".into()) };
    let partial = generate(&store, &backend, &project.id, options, &|_| {}, &Cancel::new()).await.unwrap();
    assert_eq!(partial.version.number, 2);
    assert_eq!(partial.version.origin, VersionOrigin::SectionRegeneration);
    assert_eq!(partial.version.sections, vec!["git"]);
    assert_eq!(partial.version.note.as_deref(), Some("Branches en anglais."));
    let last = backend.requests().pop().unwrap();
    assert!(last.messages[1].content.contains("# Version actuelle de la section"));
    assert!(last.messages[1].content.contains("Branches en anglais."));
    assert_eq!(store.list_versions(&project.id).unwrap().len(), 2);
}

#[tokio::test]
async fn empty_thinking_answer_is_retried_without_thinking() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let project = store.create_project(input()).unwrap();
    let backend =
        ScriptedBackend::new(|req: &ChatRequest| if req.thinking { Ok(String::new()) } else { echo_title(req) });
    let options = GenerateOptions { sections: Some(vec![SectionId::Commands]), instruction: None };
    let retried = Mutex::new(false);
    let on_event = |e: GenEvent| {
        if matches!(e, GenEvent::Retrying { section: SectionId::Commands }) {
            *retried.lock().unwrap() = true;
        }
    };
    let result = generate(&store, &backend, &project.id, options, &on_event, &Cancel::new()).await.unwrap();
    assert!(*retried.lock().unwrap());
    let thinking: Vec<bool> = backend.requests().iter().map(|r| r.thinking).collect();
    assert_eq!(thinking, vec![true, false]);
    // Les sections non générées sont signalées comme manquantes.
    assert!(result.issues.iter().any(|i| i.section.as_deref() == Some("goals")));
}

#[tokio::test]
async fn cancellation_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let project = store.create_project(input()).unwrap();
    let cancel = Cancel::new();
    cancel.cancel();
    let backend = ScriptedBackend::new(echo_title);
    let err = generate(&store, &backend, &project.id, GenerateOptions::default(), &|_| {}, &cancel).await.unwrap_err();
    assert_eq!(err.code(), "cancelled");
    assert!(store.list_versions(&project.id).unwrap().is_empty());
}
