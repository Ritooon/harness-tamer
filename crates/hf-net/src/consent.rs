//! Consentement de l'utilisateur pour une action réseau.
//!
//! Une *action* est une opération demandée explicitement par l'utilisateur
//! (télécharger un modèle, chercher des skills…). Elle déclare à l'avance les
//! URLs et les hôtes qu'elle contactera. Le dialogue affiche cette liste ; une
//! fois l'action autorisée, toute requête ou redirection vers un hôte absent de
//! la liste est refusée.

use serde::{Deserialize, Serialize};
use url::Url;

/// Nature d'une action réseau. Sert au journal et au texte du dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    ModelDownload,
    RuntimeDownload,
    SkillSearch,
    SkillImport,
    OpenLink,
}

impl ActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionKind::ModelDownload => "model.download",
            ActionKind::RuntimeDownload => "runtime.download",
            ActionKind::SkillSearch => "skills.search",
            ActionKind::SkillImport => "skills.import",
            ActionKind::OpenLink => "link.open",
        }
    }
}

/// Description complète d'une action réseau, montrée telle quelle à l'utilisateur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetAction {
    pub kind: ActionKind,
    /// Phrase lisible : « Télécharger le modèle Qwen3.5-9B (Q4_K_M) ».
    pub purpose: String,
    /// URLs que l'action prévoit d'appeler en premier.
    pub urls: Vec<Url>,
    /// Hôtes supplémentaires autorisés (CDN, API annexe). Un motif `*.exemple.org`
    /// autorise tous les sous-domaines de `exemple.org`.
    pub extra_hosts: Vec<String>,
    /// Volume estimé, affiché dans le dialogue quand il est connu.
    pub estimated_bytes: Option<u64>,
}

impl NetAction {
    pub fn new(kind: ActionKind, purpose: impl Into<String>, urls: Vec<Url>) -> Self {
        Self { kind, purpose: purpose.into(), urls, extra_hosts: Vec::new(), estimated_bytes: None }
    }

    pub fn with_extra_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_hosts.extend(hosts.into_iter().map(Into::into));
        self
    }

    pub fn with_estimated_bytes(mut self, bytes: Option<u64>) -> Self {
        self.estimated_bytes = bytes;
        self
    }

    /// Liste triée et dédoublonnée des hôtes (et motifs) que l'action peut contacter.
    pub fn hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = self
            .urls
            .iter()
            .filter_map(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .chain(self.extra_hosts.iter().map(|h| h.trim().to_ascii_lowercase()))
            .filter(|h| !h.is_empty())
            .collect();
        hosts.sort();
        hosts.dedup();
        hosts
    }
}

/// Vrai si `host` correspond à l'un des hôtes ou motifs déclarés.
pub fn host_allowed(declared: &[String], host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    declared.iter().any(|pattern| match pattern.strip_prefix("*.") {
        Some(suffix) => host.len() > suffix.len() && host.ends_with(&format!(".{suffix}")),
        None => *pattern == host,
    })
}

/// Réponse de l'utilisateur au dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accepted,
    Refused,
}

/// Affiche le dialogue de consentement et attend la réponse.
///
/// L'implémentation de production (dans `src-tauri`) ouvre une boîte de
/// dialogue **native**, déclenchée côté Rust : l'interface web ne peut ni la
/// simuler ni la contourner. « Refuser » doit être le choix par défaut.
pub trait ConsentPrompt: Send + Sync {
    fn ask(&self, action: &NetAction) -> Decision;
}

/// Texte du dialogue, prêt à afficher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsentText {
    pub title: String,
    pub body: String,
    pub accept_label: String,
    pub refuse_label: String,
}

/// Construit le texte du dialogue dans la langue de l'interface (`"fr"` ou `"en"`).
pub fn render_consent(action: &NetAction, lang: &str) -> ConsentText {
    let hosts = action
        .hosts()
        .into_iter()
        .map(|h| format!("  • {h}"))
        .collect::<Vec<_>>()
        .join("\n");
    let size = action.estimated_bytes.map(|b| human_size(b, lang));
    if lang == "en" {
        let mut body = format!("{}\n\nHosts contacted:\n{hosts}", action.purpose);
        if let Some(size) = size {
            body.push_str(&format!("\n\nEstimated size: {size}"));
        }
        body.push_str(
            "\n\nNothing else leaves this computer. This access will be recorded in the network log.",
        );
        ConsentText {
            title: "Internet access requested".into(),
            body,
            accept_label: "Allow".into(),
            refuse_label: "Refuse".into(),
        }
    } else {
        let mut body = format!("{}\n\nHôtes contactés :\n{hosts}", action.purpose);
        if let Some(size) = size {
            body.push_str(&format!("\n\nTaille estimée : {size}"));
        }
        body.push_str(
            "\n\nRien d'autre ne quitte cet ordinateur. Cet accès sera inscrit dans le journal réseau.",
        );
        ConsentText {
            title: "Accès à Internet demandé".into(),
            body,
            accept_label: "Autoriser".into(),
            refuse_label: "Refuser".into(),
        }
    }
}

/// Taille lisible (Go, Mo, Ko) avec la virgule décimale française si besoin.
pub fn human_size(bytes: u64, lang: &str) -> String {
    let (value, unit_fr, unit_en) = if bytes >= 1_000_000_000 {
        (bytes as f64 / 1e9, "Go", "GB")
    } else if bytes >= 1_000_000 {
        (bytes as f64 / 1e6, "Mo", "MB")
    } else {
        (bytes as f64 / 1e3, "Ko", "KB")
    };
    if lang == "en" {
        format!("{value:.1} {unit_en}")
    } else {
        format!("{value:.1} {unit_fr}").replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action() -> NetAction {
        NetAction::new(
            ActionKind::ModelDownload,
            "Télécharger le modèle Qwen3.5-9B",
            vec![Url::parse("https://huggingface.co/x/y.gguf").unwrap()],
        )
        .with_extra_hosts(["*.hf.co", "HuggingFace.co"])
        .with_estimated_bytes(Some(5_680_522_464))
    }

    #[test]
    fn hosts_are_sorted_lowercased_and_deduplicated() {
        assert_eq!(action().hosts(), vec!["*.hf.co".to_string(), "huggingface.co".to_string()]);
    }

    #[test]
    fn wildcard_matches_subdomains_only() {
        let declared = action().hosts();
        assert!(host_allowed(&declared, "us.aws.cdn.hf.co"));
        assert!(host_allowed(&declared, "HUGGINGFACE.CO"));
        assert!(!host_allowed(&declared, "hf.co"));
        assert!(!host_allowed(&declared, "evilhf.co"));
        assert!(!host_allowed(&declared, "example.org"));
    }

    #[test]
    fn consent_text_lists_every_host_and_the_size() {
        let fr = render_consent(&action(), "fr");
        assert!(fr.body.contains("• huggingface.co"));
        assert!(fr.body.contains("• *.hf.co"));
        assert!(fr.body.contains("5,7 Go"));
        assert_eq!(fr.refuse_label, "Refuser");
        let en = render_consent(&action(), "en");
        assert!(en.body.contains("Estimated size: 5.7 GB"));
    }
}
