//! Recherche et tri des projets de l'accueil, en mémoire.
//!
//! Quelques centaines de `project.json` se lisent en quelques millisecondes :
//! un index persistant (SQLite) n'apporterait que de la complexité (ADR 0002).

use serde::{Deserialize, Serialize};

use crate::model::{ProjectStatus, ProjectSummary};
use crate::text::fold;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ProjectSort {
    #[default]
    UpdatedDesc,
    CreatedDesc,
    NameAsc,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProjectQuery {
    /// Texte cherché dans le nom, la description et les technologies imposées.
    pub text: Option<String>,
    /// `None` = tous les statuts.
    pub status: Option<ProjectStatus>,
    pub sort: ProjectSort,
}

/// Filtre puis trie. Chaque mot de la requête doit apparaître (ET logique),
/// sans tenir compte de la casse ni des accents.
pub fn query_projects(projects: Vec<ProjectSummary>, query: &ProjectQuery) -> Vec<ProjectSummary> {
    let words: Vec<String> =
        query.text.as_deref().map(fold).unwrap_or_default().split_whitespace().map(str::to_owned).collect();
    let mut out: Vec<ProjectSummary> = projects
        .into_iter()
        .filter(|p| query.status.is_none_or(|s| p.status == s))
        .filter(|p| {
            if words.is_empty() {
                return true;
            }
            let haystack = fold(&format!("{} {} {}", p.name, p.description, p.imposed_tech.join(" ")));
            words.iter().all(|w| haystack.contains(w.as_str()))
        })
        .collect();
    match query.sort {
        ProjectSort::UpdatedDesc => out.sort_by_key(|p| std::cmp::Reverse(p.updated_at)),
        ProjectSort::CreatedDesc => out.sort_by_key(|p| std::cmp::Reverse(p.created_at)),
        ProjectSort::NameAsc => out.sort_by_key(|p| fold(&p.name)),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;
    use chrono::{Duration, Utc};

    fn summary(name: &str, tech: &[&str], status: ProjectStatus, age_days: i64) -> ProjectSummary {
        let t = Utc::now() - Duration::days(age_days);
        ProjectSummary {
            id: name.into(),
            name: name.into(),
            description: format!("Projet {name}"),
            status,
            platform: Platform::Web,
            imposed_tech: tech.iter().map(|s| s.to_string()).collect(),
            has_agents: false,
            created_at: t,
            updated_at: t,
        }
    }

    fn sample() -> Vec<ProjectSummary> {
        vec![
            summary("Éditeur", &["Rust"], ProjectStatus::Active, 3),
            summary("Boutique", &["React Native"], ProjectStatus::Active, 1),
            summary("Ancien site", &["PHP"], ProjectStatus::Archived, 10),
        ]
    }

    #[test]
    fn filters_by_status_and_sorts_by_update() {
        let q = ProjectQuery { status: Some(ProjectStatus::Active), ..Default::default() };
        let names: Vec<_> = query_projects(sample(), &q).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Boutique", "Éditeur"]);
    }

    #[test]
    fn text_search_ignores_case_and_accents_and_covers_technologies() {
        let q = ProjectQuery { text: Some("EDITEUR rust".into()), ..Default::default() };
        assert_eq!(query_projects(sample(), &q).len(), 1);
        let q = ProjectQuery { text: Some("native".into()), ..Default::default() };
        assert_eq!(query_projects(sample(), &q)[0].name, "Boutique");
    }

    #[test]
    fn name_sort_is_accent_insensitive() {
        let q = ProjectQuery { sort: ProjectSort::NameAsc, ..Default::default() };
        let names: Vec<_> = query_projects(sample(), &q).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Ancien site", "Boutique", "Éditeur"]);
    }
}
