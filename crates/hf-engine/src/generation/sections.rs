//! Plan fixe de l'AGENTS.md : dix sections, chacune générée par un appel au
//! modèle (ADR 0002, « Génération »).

use serde::{Deserialize, Serialize};

use crate::model::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SectionId {
    Context,
    Goals,
    Stack,
    Structure,
    Commands,
    Conventions,
    Quality,
    Prohibitions,
    Git,
    Ui,
}

/// Contexte utile à une section (pour ne pas envoyer tout au modèle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub repo: bool,
    pub brand: bool,
    pub skills: bool,
}

impl SectionId {
    pub const ALL: [SectionId; 10] = [
        SectionId::Context,
        SectionId::Goals,
        SectionId::Stack,
        SectionId::Structure,
        SectionId::Commands,
        SectionId::Conventions,
        SectionId::Quality,
        SectionId::Prohibitions,
        SectionId::Git,
        SectionId::Ui,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SectionId::Context => "context",
            SectionId::Goals => "goals",
            SectionId::Stack => "stack",
            SectionId::Structure => "structure",
            SectionId::Commands => "commands",
            SectionId::Conventions => "conventions",
            SectionId::Quality => "quality",
            SectionId::Prohibitions => "prohibitions",
            SectionId::Git => "git",
            SectionId::Ui => "ui",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == id)
    }

    pub fn position(&self) -> usize {
        Self::ALL.iter().position(|s| s == self).expect("listed")
    }

    pub fn title(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (SectionId::Context, Lang::Fr) => "Contexte du projet",
            (SectionId::Context, Lang::En) => "Project context",
            (SectionId::Goals, Lang::Fr) => "Objectifs",
            (SectionId::Goals, Lang::En) => "Goals",
            (SectionId::Stack, Lang::Fr) => "Stack technique",
            (SectionId::Stack, Lang::En) => "Tech stack",
            (SectionId::Structure, Lang::Fr) => "Structure du dépôt",
            (SectionId::Structure, Lang::En) => "Repository structure",
            (SectionId::Commands, Lang::Fr) => "Commandes : build, tests, lint",
            (SectionId::Commands, Lang::En) => "Commands: build, test, lint",
            (SectionId::Conventions, Lang::Fr) => "Conventions de code",
            (SectionId::Conventions, Lang::En) => "Coding conventions",
            (SectionId::Quality, Lang::Fr) => "Règles de qualité",
            (SectionId::Quality, Lang::En) => "Quality rules",
            (SectionId::Prohibitions, Lang::Fr) => "Interdits",
            (SectionId::Prohibitions, Lang::En) => "Prohibitions",
            (SectionId::Git, Lang::Fr) => "Workflow Git",
            (SectionId::Git, Lang::En) => "Git workflow",
            (SectionId::Ui, Lang::Fr) => "Charte graphique et interface",
            (SectionId::Ui, Lang::En) => "UI and brand guidelines",
        }
    }

    /// Réflexion activée pour les sections où l'exactitude prime (ADR 0002).
    pub fn thinking(&self) -> bool {
        matches!(self, SectionId::Conventions | SectionId::Commands | SectionId::Quality | SectionId::Prohibitions)
    }

    /// Longueur maximale de la réponse (hors réflexion).
    pub fn max_answer_tokens(&self) -> u32 {
        match self {
            SectionId::Context | SectionId::Goals => 600,
            SectionId::Stack | SectionId::Prohibitions | SectionId::Git => 800,
            SectionId::Structure | SectionId::Commands | SectionId::Quality | SectionId::Ui => 1000,
            SectionId::Conventions => 1300,
        }
    }

    pub fn needs(&self) -> Needs {
        match self {
            SectionId::Context => Needs { repo: true, brand: false, skills: true },
            SectionId::Goals => Needs { repo: false, brand: false, skills: false },
            SectionId::Ui => Needs { repo: true, brand: true, skills: false },
            _ => Needs { repo: true, brand: false, skills: false },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_are_ordered() {
        for (i, s) in SectionId::ALL.iter().enumerate() {
            assert_eq!(SectionId::parse(s.as_str()), Some(*s));
            assert_eq!(s.position(), i);
        }
        assert_eq!(SectionId::parse("nope"), None);
        assert_eq!(SectionId::ALL.iter().filter(|s| s.thinking()).count(), 4);
    }
}
