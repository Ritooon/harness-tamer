//! Journal réseau : une ligne JSON par événement, en ajout seul.
//!
//! Chaque action (acceptée, refusée ou bloquée par la politique) et chaque
//! requête effectuée y sont inscrites. L'utilisateur le consulte dans les
//! paramètres de l'application.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::consent::ActionKind;
use crate::error::NetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEvent {
    /// Décision prise pour une action (dialogue ou politique).
    Decision,
    /// Requête HTTP effectuée dans le cadre d'une action autorisée.
    Request,
    /// Redirection ou URL refusée car l'hôte n'était pas déclaré.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalDecision {
    Accepted,
    Refused,
    BlockedByPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub ts: DateTime<Utc>,
    pub action_id: String,
    pub action: ActionKind,
    pub event: JournalEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<JournalDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256_ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl JournalEntry {
    /// Entrée minimale ; les champs optionnels se remplissent ensuite.
    pub fn new(action_id: &str, action: ActionKind, event: JournalEvent) -> Self {
        Self {
            ts: Utc::now(),
            action_id: action_id.to_owned(),
            action,
            event,
            decision: None,
            purpose: None,
            hosts: Vec::new(),
            method: None,
            url: None,
            status: None,
            bytes: None,
            sha256_ok: None,
            error: None,
        }
    }
}

/// Fichier `network.log.jsonl`. Les écritures sont sérialisées par un verrou.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Journal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), lock: Mutex::new(()) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ajoute une entrée. Une erreur d'écriture est remontée : une action dont
    /// on ne peut pas garder la trace ne doit pas avoir lieu.
    pub fn append(&self, entry: &JournalEntry) -> Result<(), NetError> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        let line = serde_json::to_string(entry).map_err(|e| NetError::Json(e.to_string()))?;
        writeln!(file, "{line}")?;
        file.flush()?;
        Ok(())
    }

    /// Relit les `limit` dernières entrées, de la plus récente à la plus ancienne.
    /// Les lignes illisibles sont ignorées.
    pub fn read_recent(&self, limit: usize) -> Result<Vec<JournalEntry>, NetError> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut entries: Vec<JournalEntry> = BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect();
        entries.reverse();
        entries.truncate(limit);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_then_read_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::new(dir.path().join("logs/network.log.jsonl"));
        for i in 0..3 {
            let mut e = JournalEntry::new(&format!("a{i}"), ActionKind::SkillSearch, JournalEvent::Decision);
            e.decision = Some(JournalDecision::Refused);
            journal.append(&e).unwrap();
        }
        let recent = journal.read_recent(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action_id, "a2");
        assert_eq!(recent[1].action_id, "a1");
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::new(dir.path().join("none.jsonl"));
        assert!(journal.read_recent(10).unwrap().is_empty());
    }
}
