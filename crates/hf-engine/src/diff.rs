//! Différences ligne à ligne entre deux versions de l'AGENTS.md.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum DiffTag {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DiffLine {
    pub tag: DiffTag,
    pub text: String,
}

pub fn line_diff(old: &str, new: &str) -> Vec<DiffLine> {
    TextDiff::from_lines(old, new)
        .iter_all_changes()
        .map(|change| DiffLine {
            tag: match change.tag() {
                ChangeTag::Equal => DiffTag::Equal,
                ChangeTag::Insert => DiffTag::Insert,
                ChangeTag::Delete => DiffTag::Delete,
            },
            text: change.value().trim_end_matches('\n').to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertions_and_deletions() {
        let d = line_diff("a\nb\nc\n", "a\nB\nc\n");
        let tags: Vec<DiffTag> = d.iter().map(|l| l.tag).collect();
        assert_eq!(tags, vec![DiffTag::Equal, DiffTag::Delete, DiffTag::Insert, DiffTag::Equal]);
        assert_eq!(d[2].text, "B");
    }
}
