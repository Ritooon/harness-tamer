//! Dialogues natifs déclenchés côté Rust : consentement réseau (verrou n°6)
//! et confirmations des actions destructrices. L'interface web ne peut ni
//! les afficher elle-même ni y répondre à la place de l'utilisateur.

use std::sync::{Arc, RwLock};

use hf_engine::model::Lang;
use hf_net::{ConsentPrompt, Decision, NetAction, render_consent};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

/// Langue de l'interface, partagée avec les dialogues.
pub type SharedLang = Arc<RwLock<Lang>>;

pub fn current_lang(lang: &SharedLang) -> Lang {
    *lang.read().unwrap_or_else(|p| p.into_inner())
}

pub struct NativeConsent<R: Runtime> {
    pub app: AppHandle<R>,
    pub lang: SharedLang,
}

impl<R: Runtime> ConsentPrompt for NativeConsent<R> {
    /// Seul un clic explicite sur « Autoriser » vaut accord. Fermer la
    /// fenêtre, appuyer sur Échap ou toute erreur d'affichage vaut refus.
    fn ask(&self, action: &NetAction) -> Decision {
        let text = render_consent(action, current_lang(&self.lang).code());
        let result = self
            .app
            .dialog()
            .message(text.body)
            .title(text.title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(text.accept_label.clone(), text.refuse_label))
            .blocking_show_with_result();
        match result {
            MessageDialogResult::Ok => Decision::Accepted,
            MessageDialogResult::Custom(label) if label == text.accept_label => Decision::Accepted,
            _ => Decision::Refused,
        }
    }
}

/// Confirmation native d'une action destructrice. Bloquant : à appeler hors
/// du fil principal (les commandes Tauri asynchrones conviennent).
pub fn confirm<R: Runtime>(app: &AppHandle<R>, lang: Lang, title: &str, message: &str, confirm_label: &str) -> bool {
    let cancel = match lang {
        Lang::Fr => "Annuler",
        Lang::En => "Cancel",
    };
    let result = app
        .dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(confirm_label.to_owned(), cancel.to_owned()))
        .blocking_show_with_result();
    matches!(result, MessageDialogResult::Ok) || matches!(&result, MessageDialogResult::Custom(l) if l == confirm_label)
}
