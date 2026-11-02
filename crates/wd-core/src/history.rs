//! Job history, stored as JSON Lines.
//!
//! Deliberately not SQLite: appending one line per finished job is enough for a
//! desktop tool, has no native dependency and survives a half-written file
//! (corrupt lines are skipped on read).

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub source: String,
    pub outputs: Vec<String>,
    pub task: String,
    pub model: String,
    #[serde(default)]
    pub backend: Option<String>,
    pub finished_at: u64,
    pub audio_seconds: f64,
    pub elapsed_seconds: f64,
    pub rtf: f64,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

impl HistoryEntry {
    pub fn new(id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            outputs: Vec::new(),
            task: "transcribe".to_string(),
            model: String::new(),
            backend: None,
            finished_at: 0,
            audio_seconds: 0.0,
            elapsed_seconds: 0.0,
            rtf: 0.0,
            ok: false,
            error: None,
        }
    }
}

pub fn append(path: &Path, entry: &HistoryEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(entry)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Reads every entry, skipping lines that are not valid JSON (a crash during
/// a write must not make the whole history unreadable).
pub fn read_all(path: &Path) -> Vec<HistoryEntry> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .collect()
}

/// Most recent entries first.
pub fn recent(path: &Path, limit: usize) -> Vec<HistoryEntry> {
    let mut entries = read_all(path);
    entries.reverse();
    entries.truncate(limit);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wd-history-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("history.jsonl")
    }

    #[test]
    fn appends_and_reads_in_order() {
        let path = temp_file("roundtrip");

        let mut first = HistoryEntry::new("1", "/a.mp3");
        first.ok = true;
        first.outputs = vec!["/out/a.txt".into()];
        append(&path, &first).unwrap();

        let mut second = HistoryEntry::new("2", "/b.mp4");
        second.error = Some("falló".into());
        append(&path, &second).unwrap();

        let all = read_all(&path);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].source, "/a.mp3");
        assert_eq!(all[1].error.as_deref(), Some("falló"));

        let last = recent(&path, 1);
        assert_eq!(last[0].id, "2");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn skips_corrupt_lines() {
        let path = temp_file("corrupt");
        std::fs::write(
            &path,
            "{\"id\":\"1\",\"source\":\"/a\",\"outputs\":[],\"task\":\"transcribe\",\"model\":\"small\",\"finishedAt\":0,\"audioSeconds\":1.0,\"elapsedSeconds\":1.0,\"rtf\":1.0,\"ok\":true}\n\
             not json at all\n\
             {\"id\":\"2\",\"source\":\"/b\",\"outputs\":[],\"task\":\"transcribe\",\"model\":\"small\",\"finishedAt\":0,\"audioSeconds\":1.0,\"elapsedSeconds\":1.0,\"rtf\":1.0,\"ok\":true}\n",
        )
        .unwrap();

        let entries = read_all(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "1");
        assert_eq!(entries[1].id, "2");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(read_all(Path::new("/nonexistent/history.jsonl")).is_empty());
    }
}
