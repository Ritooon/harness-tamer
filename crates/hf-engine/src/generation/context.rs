//! Contexte compact envoyé au modèle : formulaire du projet, profil du
//! dépôt, charte graphique et skills rattachés, chacun avec un budget de
//! caractères pour tenir dans la fenêtre de contexte d'un modèle local.

use crate::brand::BrandKit;
use crate::model::{Lang, Project, SkillMeta};
use crate::scan::RepoProfile;
use crate::text::truncate_chars;

use super::sections::Needs;

/// Budgets en caractères (≈ 3,5 caractères par token).
const PROJECT_BUDGET: usize = 4_000;
const REPO_BUDGET: usize = 9_000;
const BRAND_BUDGET: usize = 3_000;
const SKILLS_BUDGET: usize = 1_500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBlocks {
    pub project: String,
    pub repo: Option<String>,
    pub brand: Option<String>,
    pub skills: Option<String>,
}

impl ContextBlocks {
    /// Blocs utiles à une section, séparés par une ligne vide.
    pub fn for_needs(&self, needs: Needs) -> String {
        let mut parts = vec![self.project.as_str()];
        if needs.repo
            && let Some(r) = &self.repo
        {
            parts.push(r);
        }
        if needs.brand
            && let Some(b) = &self.brand
        {
            parts.push(b);
        }
        if needs.skills
            && let Some(s) = &self.skills
        {
            parts.push(s);
        }
        parts.join("\n\n")
    }
}

struct Labels {
    project: &'static str,
    name: &'static str,
    description: &'static str,
    platform: &'static str,
    targets: &'static str,
    imposed: &'static str,
    existing: &'static str,
    existing_yes: &'static str,
    existing_no: &'static str,
    brand: &'static str,
    brand_yes: &'static str,
    brand_no: &'static str,
    constraints: &'static str,
    none: &'static str,
    repo: &'static str,
    files: &'static str,
    partial: &'static str,
    languages: &'static str,
    package_managers: &'static str,
    manifests: &'static str,
    deps: &'static str,
    dev_deps: &'static str,
    commands: &'static str,
    tools: &'static str,
    ci: &'static str,
    conventions: &'static str,
    indent: &'static str,
    naming: &'static str,
    git: &'static str,
    branch: &'static str,
    conventional: &'static str,
    recent: &'static str,
    tree: &'static str,
    docs: &'static str,
    brand_title: &'static str,
    colors: &'static str,
    fonts: &'static str,
    logos: &'static str,
    other_files: &'static str,
    guidelines: &'static str,
    skills_title: &'static str,
}

const FR: Labels = Labels {
    project: "# Projet",
    name: "Nom",
    description: "Description",
    platform: "Plateforme",
    targets: "Systèmes cibles",
    imposed: "Technologies imposées",
    existing: "Code existant",
    existing_yes: "oui, dépôt analysé ci-dessous",
    existing_no: "non, nouveau projet",
    brand: "Charte graphique",
    brand_yes: "fournie",
    brand_no: "aucune",
    constraints: "Contraintes supplémentaires",
    none: "aucune",
    repo: "# Dépôt analysé",
    files: "Fichiers",
    partial: "analyse partielle, dépôt très volumineux",
    languages: "Langages",
    package_managers: "Gestionnaires de paquets",
    manifests: "Manifestes",
    deps: "dépendances",
    dev_deps: "développement",
    commands: "Commandes détectées",
    tools: "Outils configurés",
    ci: "Intégration continue",
    conventions: "Conventions détectées",
    indent: "indentation",
    naming: "nommage des fichiers",
    git: "Git",
    branch: "branche courante",
    conventional: "des messages récents suivent Conventional Commits",
    recent: "Messages de commit récents",
    tree: "Arborescence",
    docs: "Documents existants (extraits)",
    brand_title: "# Charte graphique",
    colors: "Couleurs",
    fonts: "Typographies",
    logos: "Logos",
    other_files: "Autres fichiers fournis (non analysés)",
    guidelines: "Consignes fournies (extraits)",
    skills_title: "# Skills rattachés au projet",
};

const EN: Labels = Labels {
    project: "# Project",
    name: "Name",
    description: "Description",
    platform: "Platform",
    targets: "Target systems",
    imposed: "Imposed technologies",
    existing: "Existing code",
    existing_yes: "yes, repository analysed below",
    existing_no: "no, new project",
    brand: "Brand guidelines",
    brand_yes: "provided",
    brand_no: "none",
    constraints: "Additional constraints",
    none: "none",
    repo: "# Analysed repository",
    files: "Files",
    partial: "partial analysis, very large repository",
    languages: "Languages",
    package_managers: "Package managers",
    manifests: "Manifests",
    deps: "dependencies",
    dev_deps: "development",
    commands: "Detected commands",
    tools: "Configured tools",
    ci: "Continuous integration",
    conventions: "Detected conventions",
    indent: "indentation",
    naming: "file naming",
    git: "Git",
    branch: "current branch",
    conventional: "of recent messages follow Conventional Commits",
    recent: "Recent commit messages",
    tree: "Tree",
    docs: "Existing documents (excerpts)",
    brand_title: "# Brand guidelines",
    colors: "Colours",
    fonts: "Typefaces",
    logos: "Logos",
    other_files: "Other provided files (not analysed)",
    guidelines: "Provided guidelines (excerpts)",
    skills_title: "# Skills attached to the project",
};

fn labels(lang: Lang) -> &'static Labels {
    match lang {
        Lang::Fr => &FR,
        Lang::En => &EN,
    }
}

fn percent(value: f64, lang: Lang) -> String {
    match lang {
        Lang::Fr => format!("{value:.0} %"),
        Lang::En => format!("{value:.0}%"),
    }
}

pub fn build(
    project: &Project,
    profile: Option<&RepoProfile>,
    brand: Option<&BrandKit>,
    skills: &[SkillMeta],
    lang: Lang,
) -> ContextBlocks {
    ContextBlocks {
        project: project_block(project, brand.is_some(), lang),
        repo: profile.filter(|_| project.existing_code.enabled).map(|p| repo_block(p, lang)),
        brand: brand.filter(|_| project.brand.enabled).map(|b| brand_block(b, lang)),
        skills: (!skills.is_empty()).then(|| skills_block(skills, lang)),
    }
}

fn project_block(p: &Project, has_brand: bool, lang: Lang) -> String {
    let l = labels(lang);
    let list = |items: Vec<String>| if items.is_empty() { l.none.to_owned() } else { items.join(", ") };
    let mut out = format!("{}\n- {}: {}\n", l.project, l.name, p.name);
    if !p.description.trim().is_empty() {
        out.push_str(&format!("- {}: {}\n", l.description, truncate_chars(p.description.trim(), 1_500)));
    }
    out.push_str(&format!("- {}: {}\n", l.platform, p.platform.label(lang)));
    out.push_str(&format!("- {}: {}\n", l.targets, list(p.target_os.iter().map(|o| o.label().to_owned()).collect())));
    out.push_str(&format!("- {}: {}\n", l.imposed, list(p.imposed_tech.clone())));
    let existing = if p.existing_code.enabled { l.existing_yes } else { l.existing_no };
    out.push_str(&format!("- {}: {existing}\n", l.existing));
    let brand = if p.brand.enabled && has_brand { l.brand_yes } else { l.brand_no };
    out.push_str(&format!("- {}: {brand}\n", l.brand));
    let constraints = p.constraints.trim();
    if constraints.is_empty() {
        out.push_str(&format!("- {}: {}\n", l.constraints, l.none));
    } else {
        out.push_str(&format!("- {}:\n{}\n", l.constraints, truncate_chars(constraints, 1_500)));
    }
    truncate_chars(out.trim_end(), PROJECT_BUDGET)
}

/// Ajoute des parties dans l'ordre de priorité tant que le budget le permet.
struct Budgeted {
    out: String,
    budget: usize,
}

impl Budgeted {
    fn push(&mut self, part: String) {
        if part.is_empty() {
            return;
        }
        let used = self.out.chars().count();
        if used >= self.budget {
            return;
        }
        let remaining = self.budget - used;
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out.push_str(&truncate_chars(&part, remaining.saturating_sub(1)));
    }
}

fn repo_block(p: &RepoProfile, lang: Lang) -> String {
    let l = labels(lang);
    let mut b = Budgeted { out: l.repo.to_owned(), budget: REPO_BUDGET };

    let mut files = format!("- {}: {}", l.files, p.file_count);
    if p.truncated {
        files.push_str(&format!(" ({})", l.partial));
    }
    b.push(files);

    if !p.commands.is_empty() {
        let lines: Vec<String> =
            p.commands.iter().map(|c| format!("  - {}: `{}` ({})", c.role, c.command, c.source)).collect();
        b.push(format!("- {}:\n{}", l.commands, lines.join("\n")));
    }
    if !p.tools.is_empty() {
        let items: Vec<String> = p.tools.iter().map(|t| format!("{} (`{}`)", t.name, t.config)).collect();
        b.push(format!("- {}: {}", l.tools, items.join(", ")));
    }
    let total: u64 = p.languages.iter().map(|s| s.bytes).sum();
    if total > 0 {
        let items: Vec<String> = p
            .languages
            .iter()
            .take(6)
            .map(|s| format!("{} {}", s.name, percent(s.bytes as f64 * 100.0 / total as f64, lang)))
            .collect();
        b.push(format!("- {}: {}", l.languages, items.join(", ")));
    }
    if !p.package_managers.is_empty() {
        b.push(format!("- {}: {}", l.package_managers, p.package_managers.join(", ")));
    }
    if !p.manifests.is_empty() {
        let lines: Vec<String> = p
            .manifests
            .iter()
            .take(8)
            .map(|m| {
                let mut line = format!("  - `{}`", m.path);
                if let Some(name) = &m.name {
                    line.push_str(&format!(" ({name})"));
                }
                if !m.dependencies.is_empty() {
                    line.push_str(&format!(" — {}: {}", l.deps, take_list(&m.dependencies, 25)));
                }
                if !m.dev_dependencies.is_empty() {
                    line.push_str(&format!(" ; {}: {}", l.dev_deps, take_list(&m.dev_dependencies, 15)));
                }
                line
            })
            .collect();
        b.push(format!("- {}:\n{}", l.manifests, lines.join("\n")));
    }
    if !p.ci.is_empty() {
        let lines: Vec<String> =
            p.ci.iter()
                .take(5)
                .map(|w| {
                    let cmds: Vec<String> = w.commands.iter().take(10).map(|c| format!("`{c}`")).collect();
                    format!("  - `{}` ({}): {}", w.path, w.provider, cmds.join(", "))
                })
                .collect();
        b.push(format!("- {}:\n{}", l.ci, lines.join("\n")));
    }
    let mut conv = Vec::new();
    if let Some(i) = &p.conventions.indent {
        conv.push(format!("{} {i}", l.indent));
    }
    if let Some(n) = &p.conventions.file_naming {
        conv.push(format!("{} {n}", l.naming));
    }
    if !conv.is_empty() {
        b.push(format!("- {}: {}", l.conventions, conv.join(" ; ")));
    }
    if let Some(git) = &p.git {
        let mut line = format!("- {}:", l.git);
        if let Some(branch) = &git.branch {
            line.push_str(&format!(" {} `{branch}` ;", l.branch));
        }
        line.push_str(&format!(" {} {}", percent(f64::from(git.conventional_ratio) * 100.0, lang), l.conventional));
        b.push(line);
        if !git.recent_subjects.is_empty() {
            let subjects: Vec<String> = git.recent_subjects.iter().take(8).map(|s| format!("  - {s}")).collect();
            b.push(format!("- {}:\n{}", l.recent, subjects.join("\n")));
        }
    }
    if !p.tree.is_empty() {
        let tree: Vec<&str> = p.tree.iter().take(80).map(String::as_str).collect();
        b.push(format!("- {}:\n```text\n{}\n```", l.tree, tree.join("\n")));
    }
    if !p.docs.is_empty() {
        let docs: Vec<String> =
            p.docs.iter().map(|d| format!("### {}\n{}", d.path, truncate_chars(&d.excerpt, 800))).collect();
        b.push(format!("- {}:\n{}", l.docs, docs.join("\n\n")));
    }
    b.out
}

fn take_list(items: &[String], n: usize) -> String {
    let mut out = items.iter().take(n).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > n {
        out.push_str(&format!(", … (+{})", items.len() - n));
    }
    out
}

fn brand_block(kit: &BrandKit, lang: Lang) -> String {
    let l = labels(lang);
    let mut b = Budgeted { out: l.brand_title.to_owned(), budget: BRAND_BUDGET };
    if !kit.colors.is_empty() {
        let items: Vec<String> = kit
            .colors
            .iter()
            .map(|c| match &c.name {
                Some(name) => format!("`{}` {name}", c.value),
                None => format!("`{}`", c.value),
            })
            .collect();
        b.push(format!("- {}: {}", l.colors, items.join(", ")));
    }
    if !kit.fonts.is_empty() {
        let items: Vec<&str> = kit.fonts.iter().map(|f| f.family.as_str()).collect();
        b.push(format!("- {}: {}", l.fonts, items.join(", ")));
    }
    if !kit.logos.is_empty() {
        b.push(format!("- {}: {}", l.logos, kit.logos.join(", ")));
    }
    if !kit.other_files.is_empty() {
        b.push(format!("- {}: {}", l.other_files, kit.other_files.join(", ")));
    }
    if !kit.guidelines.is_empty() {
        let items: Vec<String> =
            kit.guidelines.iter().map(|g| format!("### {}\n{}", g.file, truncate_chars(&g.excerpt, 1_200))).collect();
        b.push(format!("- {}:\n{}", l.guidelines, items.join("\n\n")));
    }
    b.out
}

fn skills_block(skills: &[SkillMeta], lang: Lang) -> String {
    let l = labels(lang);
    let lines: Vec<String> =
        skills.iter().map(|s| format!("- `{}`: {}", s.slug, truncate_chars(&s.description, 200))).collect();
    truncate_chars(&format!("{}\n{}", l.skills_title, lines.join("\n")), SKILLS_BUDGET)
}

#[cfg(test)]
pub(crate) mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::{BrandSettings, ExistingCode, Platform, ProjectStatus, SCHEMA_VERSION, TargetOs};
    use crate::scan::{GitInfo, LanguageStat, RepoCommand};

    pub(crate) fn project(existing: bool) -> Project {
        Project {
            schema_version: SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            name: "Atelier".into(),
            description: "Gestion d'ateliers de couture.".into(),
            status: ProjectStatus::Active,
            platform: Platform::Web,
            target_os: vec![TargetOs::Linux, TargetOs::Windows],
            imposed_tech: vec!["React".into()],
            existing_code: ExistingCode { enabled: existing, repo_path: existing.then(|| "/tmp/x".into()) },
            brand: BrandSettings::default(),
            constraints: "Pas de dépendance GPL.".into(),
            output_language: Lang::Fr,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub(crate) fn profile() -> RepoProfile {
        RepoProfile {
            schema_version: SCHEMA_VERSION,
            root: "/tmp/x".into(),
            scanned_at: Utc::now(),
            file_count: 42,
            total_bytes: 1000,
            truncated: false,
            languages: vec![
                LanguageStat { name: "TypeScript".into(), files: 30, bytes: 750 },
                LanguageStat { name: "CSS".into(), files: 5, bytes: 250 },
            ],
            manifests: vec![],
            package_managers: vec!["pnpm".into()],
            commands: vec![RepoCommand {
                role: "test".into(),
                command: "pnpm test".into(),
                source: "package.json".into(),
            }],
            tools: vec![],
            ci: vec![],
            tree: vec!["src/".into(), "src/main.ts".into()],
            conventions: Default::default(),
            git: Some(GitInfo {
                branch: Some("main".into()),
                recent_subjects: vec!["feat: x".into()],
                conventional_ratio: 1.0,
            }),
            docs: vec![],
        }
    }

    #[test]
    fn blocks_follow_the_form_and_needs() {
        let p = project(true);
        let ctx = build(&p, Some(&profile()), None, &[], Lang::Fr);
        assert!(ctx.project.contains("- Nom: Atelier"));
        assert!(ctx.project.contains("Linux, Windows"));
        assert!(ctx.project.contains("Pas de dépendance GPL."));
        let repo = ctx.repo.as_deref().unwrap();
        assert!(repo.contains("test: `pnpm test` (package.json)"));
        assert!(repo.contains("TypeScript 75 %"));
        assert!(repo.contains("branche courante `main`"));
        let goals = ctx.for_needs(Needs { repo: false, brand: false, skills: false });
        assert!(!goals.contains("Dépôt analysé"));

        // Sans code existant, le profil n'est pas envoyé.
        let new = build(&project(false), Some(&profile()), None, &[], Lang::En);
        assert!(new.repo.is_none());
        assert!(new.project.contains("no, new project"));
    }

    #[test]
    fn repo_block_respects_its_budget() {
        let mut big = profile();
        big.tree = (0..5_000).map(|i| format!("src/module_{i}/file.ts")).collect();
        let ctx = build(&project(true), Some(&big), None, &[], Lang::Fr);
        assert!(ctx.repo.unwrap().chars().count() <= REPO_BUDGET);
    }
}
