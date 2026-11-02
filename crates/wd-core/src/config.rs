//! User configuration (`config.json`).

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no se pudo leer la configuración: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuración inválida: {0}")]
    Parse(#[from] serde_json::Error),
}

pub use crate::job::Task;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OnCollision {
    #[default]
    #[serde(rename = "rename")]
    Rename,
    #[serde(rename = "overwrite")]
    Overwrite,
    #[serde(rename = "ask")]
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub schema: u32,
    pub install: InstallConfig,
    pub defaults: Defaults,
    pub data: DataConfig,
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            install: InstallConfig::default(),
            defaults: Defaults::default(),
            data: DataConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Loads when present, returns defaults otherwise.
    pub fn load_or_default(path: &Path) -> Self {
        Self::load(path).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallConfig {
    pub backend: Option<String>,
    pub backend_dir: Option<String>,
    pub models: Vec<String>,
    #[serde(default)]
    pub verified_backend: Option<String>,
    #[serde(default)]
    pub verified_rtf: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Defaults {
    pub task: Task,
    pub language: String,
    pub model: String,
    pub output_dir: String,
    pub formats: Vec<String>,
    pub name_pattern: String,
    pub on_collision: OnCollision,
    pub preserve_tree: bool,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            task: Task::Transcribe,
            language: "auto".to_string(),
            model: "large-v3-turbo".to_string(),
            output_dir: String::new(),
            formats: vec!["txt".to_string(), "srt".to_string()],
            name_pattern: "{nombre}_{fecha}".to_string(),
            on_collision: OnCollision::Rename,
            preserve_tree: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DataConfig {
    /// Empty means "next to the application" (portable mode on Windows) or
    /// the platform data directory on Linux.
    pub dir: String,
    pub is_custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiConfig {
    pub language: String,
    pub open_folder_when_done: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: "es".to_string(),
            open_folder_when_done: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.defaults.task, Task::Transcribe);
        assert_eq!(config.defaults.on_collision, OnCollision::Rename);
        assert_eq!(config.defaults.formats, vec!["txt", "srt"]);
        assert!(!config.data.is_custom);
    }

    #[test]
    fn roundtrip_and_partial_files() {
        let dir = std::env::temp_dir().join(format!("wd-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let mut config = Config::default();
        config.defaults.model = "small".to_string();
        config.ui.language = "en".to_string();
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, config);

        // A partial file must still load, filling the rest with defaults.
        std::fs::write(&path, r#"{"defaults":{"model":"base"}}"#).unwrap();
        let partial = Config::load(&path).unwrap();
        assert_eq!(partial.defaults.model, "base");
        assert_eq!(partial.ui.language, "es");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_returns_defaults() {
        let config = Config::load_or_default(Path::new("/nonexistent/whisper/config.json"));
        assert_eq!(config.schema, 1);
    }
}
