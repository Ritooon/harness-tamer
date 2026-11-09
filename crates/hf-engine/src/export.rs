//! Export d'un projet vers un dossier (ou une archive ZIP) :
//!
//! ```text
//! <cible>/
//! ├── AGENTS.md                 sans marqueurs internes (ou dans .agents/)
//! └── .agents/
//!     ├── skills/<slug>/        SKILL.md + ressources des skills rattachés
//!     └── harnessforge.json     version exportée, date, modèle
//! ```
//!
//! Rien n'est écrasé sans l'option `overwrite` : l'interface liste d'abord
//! les fichiers existants et demande confirmation.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::generation::document::strip_markers;
use crate::model::{Lang, Skill};
use crate::store::{Store, safe_relative, write_atomic};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExportOptions {
    /// `AGENTS.md` à la racine de la cible (sinon dans `.agents/`).
    pub agents_at_root: bool,
    pub include_skills: bool,
    pub overwrite: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { agents_at_root: true, include_skills: true, overwrite: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExportReport {
    pub target: String,
    /// Fichiers écrits, relatifs à la cible.
    pub written: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Manifest<'a> {
    generator: &'static str,
    generator_version: &'static str,
    project: &'a str,
    agents_version: Option<u32>,
    model: Option<&'a str>,
    exported_at: String,
    skills: Vec<&'a str>,
}

/// Fichiers de l'export, en mémoire (chemin relatif → contenu).
pub fn export_files(store: &Store, project_id: &str, options: &ExportOptions) -> Result<Vec<(String, Vec<u8>)>> {
    let project = store.get_project(project_id)?;
    let agents = store
        .load_agents(project_id)?
        .ok_or_else(|| EngineError::Invalid("the project has no AGENTS.md yet".into()))?;
    let skills = if options.include_skills { store.list_project_skills(project_id)? } else { vec![] };
    let versions = store.list_versions(project_id)?;
    let latest = versions.first();

    let mut agents_md = strip_markers(&agents);
    if !skills.is_empty() {
        agents_md.push('\n');
        agents_md.push_str(&skills_appendix(&skills, project.output_language, options.agents_at_root));
    }
    let agents_path = if options.agents_at_root { "AGENTS.md" } else { ".agents/AGENTS.md" };
    let mut files = vec![(agents_path.to_owned(), agents_md.into_bytes())];

    for skill in &skills {
        let dir = format!(".agents/skills/{}", skill.meta.slug);
        files.push((format!("{dir}/SKILL.md"), skill.content.clone().into_bytes()));
        for f in &skill.files {
            safe_relative(&f.path)?;
            files.push((format!("{dir}/{}", f.path), f.content.clone().into_bytes()));
        }
    }
    let manifest = Manifest {
        generator: "HarnessForge",
        generator_version: env!("CARGO_PKG_VERSION"),
        project: &project.name,
        agents_version: latest.map(|v| v.number),
        model: latest.and_then(|v| v.model.as_deref()),
        exported_at: Utc::now().to_rfc3339(),
        skills: skills.iter().map(|s| s.meta.slug.as_str()).collect(),
    };
    let mut json = serde_json::to_vec_pretty(&manifest)?;
    json.push(b'\n');
    files.push((".agents/harnessforge.json".into(), json));
    Ok(files)
}

/// Liste déterministe des skills, ajoutée à la fin de l'AGENTS.md exporté.
fn skills_appendix(skills: &[Skill], lang: Lang, agents_at_root: bool) -> String {
    let base = if agents_at_root { ".agents/skills" } else { "skills" };
    let mut out = match lang {
        Lang::Fr => format!(
            "## Skills\n\nLes skills suivants sont disponibles dans `{base}/`. Chargez celui qui correspond à la tâche avant de commencer.\n\n"
        ),
        Lang::En => format!(
            "## Skills\n\nThe following skills are available in `{base}/`. Load the one that matches the task before starting.\n\n"
        ),
    };
    for s in skills {
        out.push_str(&format!("- [`{}`]({base}/{}/SKILL.md) : {}\n", s.meta.slug, s.meta.slug, s.meta.description));
    }
    if lang == Lang::En {
        out = out.replace(") : ", "): ");
    }
    out
}

/// Fichiers qui seraient remplacés dans `target`.
pub fn conflicts(target: &Path, files: &[(String, Vec<u8>)]) -> Vec<String> {
    files.iter().filter(|(rel, _)| target.join(rel).exists()).map(|(rel, _)| rel.clone()).collect()
}

/// Écrit l'export dans un dossier existant (la racine d'un dépôt, en général).
pub fn export_to_dir(store: &Store, project_id: &str, target: &Path, options: &ExportOptions) -> Result<ExportReport> {
    if !target.is_dir() {
        return Err(EngineError::Invalid(format!("not a folder: {}", target.display())));
    }
    let files = export_files(store, project_id, options)?;
    let existing = conflicts(target, &files);
    if !existing.is_empty() && !options.overwrite {
        return Err(EngineError::ExportTargetExists(existing.join(", ")));
    }
    let mut written = Vec::new();
    for (rel, data) in &files {
        let path: PathBuf = target.join(safe_relative(rel)?);
        write_atomic(&path, data)?;
        written.push(rel.clone());
    }
    Ok(ExportReport { target: target.display().to_string(), written })
}

/// Écrit l'export dans une archive ZIP.
pub fn export_to_zip(store: &Store, project_id: &str, dest: &Path, options: &ExportOptions) -> Result<ExportReport> {
    let files = export_files(store, project_id, options)?;
    if dest.exists() && !options.overwrite {
        return Err(EngineError::ExportTargetExists(dest.display().to_string()));
    }
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (rel, data) in &files {
            zip.start_file(rel.as_str(), opts).map_err(|e| EngineError::Io(std::io::Error::other(e)))?;
            zip.write_all(data)?;
        }
        zip.finish().map_err(|e| EngineError::Io(std::io::Error::other(e)))?;
    }
    write_atomic(dest, buffer.get_ref())?;
    Ok(ExportReport { target: dest.display().to_string(), written: files.into_iter().map(|(rel, _)| rel).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExistingCode, Platform, ProjectInput, SkillOrigin};
    use crate::store::{SkillDraft, VersionDraft};

    fn setup() -> (tempfile::TempDir, Store, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("data")).unwrap();
        let project = store
            .create_project(ProjectInput {
                name: "Atelier".into(),
                description: String::new(),
                platform: Platform::Cli,
                target_os: vec![],
                imposed_tech: vec![],
                existing_code: ExistingCode::default(),
                brand_enabled: false,
                constraints: String::new(),
                output_language: Lang::Fr,
            })
            .unwrap();
        store
            .save_agents_version(
                &project.id,
                "# Atelier\n\n<!-- hf:section id=\"git\" -->\n## Git\nTexte\n<!-- hf:end -->\n",
                VersionDraft { model: Some("Qwen".into()), ..Default::default() },
            )
            .unwrap();
        let skill = store
            .create_skill(SkillDraft {
                content: "---\nname: pdf\ndescription: Remplit des PDF.\n---\nCorps\n".into(),
                files: vec![crate::model::SkillFile { path: "scripts/a.py".into(), content: "print(1)\n".into() }],
                origin: SkillOrigin::Created,
            })
            .unwrap();
        store.attach_skill(&project.id, &skill.meta.slug).unwrap();
        (dir, store, project.id)
    }

    #[test]
    fn folder_export_layout_and_conflicts() {
        let (dir, store, id) = setup();
        let target = dir.path().join("repo");
        std::fs::create_dir_all(&target).unwrap();
        let report = export_to_dir(&store, &id, &target, &ExportOptions::default()).unwrap();
        assert_eq!(
            report.written,
            vec![
                "AGENTS.md",
                ".agents/skills/pdf/SKILL.md",
                ".agents/skills/pdf/scripts/a.py",
                ".agents/harnessforge.json"
            ]
        );
        let agents = std::fs::read_to_string(target.join("AGENTS.md")).unwrap();
        assert!(!agents.contains("hf:"));
        assert!(agents.contains("- [`pdf`](.agents/skills/pdf/SKILL.md) : Remplit des PDF."));
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(target.join(".agents/harnessforge.json")).unwrap()).unwrap();
        assert_eq!(manifest["agents_version"], 1);
        assert_eq!(manifest["model"], "Qwen");

        // Second export : refusé sans `overwrite`, accepté avec.
        let err = export_to_dir(&store, &id, &target, &ExportOptions::default()).unwrap_err();
        assert_eq!(err.code(), "export.target_exists");
        let opts = ExportOptions { overwrite: true, ..Default::default() };
        assert!(export_to_dir(&store, &id, &target, &opts).is_ok());
    }

    #[test]
    fn zip_export_and_agents_inside_folder() {
        let (dir, store, id) = setup();
        let dest = dir.path().join("export.zip");
        let opts = ExportOptions { agents_at_root: false, ..Default::default() };
        let report = export_to_zip(&store, &id, &dest, &opts).unwrap();
        assert_eq!(report.written[0], ".agents/AGENTS.md");
        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(&bytes[..2], b"PK");
        let files = export_files(&store, &id, &opts).unwrap();
        let agents = String::from_utf8(files[0].1.clone()).unwrap();
        assert!(agents.contains("(skills/pdf/SKILL.md)"));
    }
}
