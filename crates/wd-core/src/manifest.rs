//! Install manifest: the single source of truth for "what is installed".
//!
//! Written only after a successful installation, and used to decide whether
//! the wizard must run and what the uninstaller must remove.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::decide::BackendId;

pub const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("no se pudo leer el manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest inválido: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    /// Path relative to the application root.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema: u32,
    pub app_version: String,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    pub backend: BackendId,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub models: Vec<String>,
}

impl Manifest {
    pub fn new(
        app_version: impl Into<String>,
        backend: BackendId,
        files: Vec<ManifestFile>,
    ) -> Self {
        let installed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            schema: MANIFEST_SCHEMA,
            app_version: app_version.into(),
            installed_at,
            backend,
            files,
            models: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Files that are missing or have the wrong size. Empty means healthy.
    /// Size is checked instead of hashing so the check costs milliseconds
    /// even with multi-gigabyte models.
    pub fn problems(&self, root: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        for file in &self.files {
            let path = root.join(&file.path);
            match std::fs::metadata(&path) {
                Ok(meta) if meta.len() == file.size => {}
                Ok(meta) => problems.push(format!(
                    "{}: tamaño {} (esperado {})",
                    file.path,
                    meta.len(),
                    file.size
                )),
                Err(_) => problems.push(format!("{}: no existe", file.path)),
            }
        }
        problems
    }

    pub fn is_installed(&self, root: &Path) -> bool {
        self.schema == MANIFEST_SCHEMA && self.problems(root).is_empty()
    }

    /// Resolves a manifest entry to an absolute path, refusing paths that
    /// escape the application root.
    pub fn resolve(&self, root: &Path, relative: &str) -> Option<PathBuf> {
        if relative.contains("..") || Path::new(relative).is_absolute() {
            return None;
        }
        Some(root.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wd-manifest-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_and_health_check() {
        let root = temp_root("roundtrip");
        std::fs::write(root.join("whisper-cli"), b"binary").unwrap();
        std::fs::write(root.join("ffmpeg"), b"tool").unwrap();

        let manifest = Manifest::new(
            "0.2.0",
            BackendId::Vulkan,
            vec![
                ManifestFile {
                    path: "whisper-cli".into(),
                    sha256: "a".repeat(64),
                    size: 6,
                },
                ManifestFile {
                    path: "ffmpeg".into(),
                    sha256: "b".repeat(64),
                    size: 4,
                },
            ],
        );

        let path = root.join("manifest.json");
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded, manifest);
        assert!(loaded.is_installed(&root));

        std::fs::remove_file(root.join("ffmpeg")).unwrap();
        assert!(!loaded.is_installed(&root));
        assert_eq!(loaded.problems(&root).len(), 1);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn detects_size_mismatch() {
        let root = temp_root("size");
        std::fs::write(root.join("a.bin"), b"12345").unwrap();
        let manifest = Manifest::new(
            "0.1.0",
            BackendId::Cpu,
            vec![ManifestFile {
                path: "a.bin".into(),
                sha256: "c".repeat(64),
                size: 99,
            }],
        );
        let problems = manifest.problems(&root);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("tamaño"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_paths_outside_root() {
        let manifest = Manifest::new("0.1.0", BackendId::Cpu, vec![]);
        let root = Path::new("/tmp/app");
        assert!(manifest.resolve(root, "../etc/passwd").is_none());
        assert!(manifest.resolve(root, "/etc/passwd").is_none());
        assert!(manifest.resolve(root, "models/x.bin").is_some());
    }
}
