//! Signed catalogue: where backends, tools and models come from.
//!
//! The application ships no artifact URLs. It downloads `catalog.json`,
//! verifies its minisign signature and takes everything from there. Changing
//! a whisper.cpp build therefore needs no app release.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::decide::BackendId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    #[serde(rename = "windows-x86_64")]
    WindowsX64,
    #[serde(rename = "linux-x86_64")]
    LinuxX64,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::WindowsX64
        } else {
            Platform::LinuxX64
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Platform::WindowsX64 => "windows-x86_64",
            Platform::LinuxX64 => "linux-x86_64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl Artifact {
    /// Placeholder hashes are used in the development catalogue so the
    /// repository is useful before the first backend release exists.
    pub fn is_placeholder(&self) -> bool {
        self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit())
    }

    pub fn is_secure(&self) -> bool {
        self.url.starts_with("https://")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBackend {
    pub id: String,
    pub platform: Platform,
    #[serde(default)]
    pub priority: u32,
    pub artifact: Artifact,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTool {
    pub id: String,
    pub platform: Platform,
    #[serde(default)]
    pub version: Option<String>,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub vram_mb: u32,
    #[serde(default)]
    pub multilingual: bool,
}

impl CatalogModel {
    pub fn is_placeholder(&self) -> bool {
        self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema: u32,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub backends: Vec<CatalogBackend>,
    #[serde(default)]
    pub tools: Vec<CatalogTool>,
    #[serde(default)]
    pub models: Vec<CatalogModel>,
}

impl Catalog {
    pub fn parse(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }

    pub fn backend(&self, id: BackendId, platform: Platform) -> Option<&CatalogBackend> {
        self.backends
            .iter()
            .find(|b| b.id == id.as_str() && b.platform == platform)
    }

    pub fn tool(&self, id: &str, platform: Platform) -> Option<&CatalogTool> {
        self.tools
            .iter()
            .find(|t| t.id == id && t.platform == platform)
    }

    pub fn model(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Checks that this catalogue can be installed from. Returns a list of
    /// human readable problems, empty when the catalogue is usable.
    pub fn validate(&self, app_version: &str) -> Vec<String> {
        let mut errors = Vec::new();

        if self.schema != 1 {
            errors.push(format!("esquema de catálogo no soportado: {}", self.schema));
        }

        if let Some(min) = &self.min_app_version {
            if !version_at_least(app_version, min) {
                errors.push(format!(
                    "el catálogo requiere Whisper Desktop {min} o superior (tienes {app_version})"
                ));
            }
        }

        for backend in &self.backends {
            validate_artifact(&backend.artifact, &mut errors);
        }
        for tool in &self.tools {
            validate_artifact(&tool.artifact, &mut errors);
        }
        for model in &self.models {
            if !model.url.starts_with("https://") {
                errors.push(format!("modelo {} sin HTTPS: {}", model.id, model.url));
            }
            if model.is_placeholder() {
                errors.push(format!("modelo {} con sha256 inválido", model.id));
            }
            if model.size == 0 {
                errors.push(format!("modelo {} sin tamaño", model.id));
            }
        }

        errors
    }
}

fn validate_artifact(artifact: &Artifact, errors: &mut Vec<String>) {
    if !artifact.is_secure() {
        errors.push(format!("artifact sin HTTPS: {}", artifact.url));
    }
    if artifact.is_placeholder() {
        errors.push(format!("artifact con sha256 inválido: {}", artifact.url));
    }
    if artifact.size == 0 {
        errors.push(format!("artifact sin tamaño: {}", artifact.url));
    }
}

/// Minimal semantic version comparison, enough for `1.2.3` style versions.
pub fn version_at_least(current: &str, minimum: &str) -> bool {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(['.', '-', '+'])
            .take(3)
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let current = parse(current);
    let minimum = parse(minimum);
    for index in 0..3 {
        let a = current.get(index).copied().unwrap_or(0);
        let b = minimum.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(version_at_least("0.2.0", "0.1.0"));
        assert!(version_at_least("0.1.0", "0.1.0"));
        assert!(!version_at_least("0.1.0", "0.2.0"));
        assert!(!version_at_least("1.0.0", "1.0.1"));
        assert!(version_at_least("1.10.0", "1.9.0"));
    }

    #[test]
    fn placeholder_detection() {
        let good = Artifact {
            url: "https://example.com/a.zip".into(),
            sha256: "a".repeat(64),
            size: 10,
        };
        assert!(!good.is_placeholder());
        let bad = Artifact {
            url: "http://example.com/a.zip".into(),
            sha256: "PENDING".into(),
            size: 0,
        };
        assert!(bad.is_placeholder());
        assert!(!bad.is_secure());
    }

    #[test]
    fn placeholder_catalog_is_rejected() {
        let raw = r#"{
            "schema": 1,
            "backends": [{
                "id": "cpu",
                "platform": "linux-x86_64",
                "artifact": { "url": "https://example.com/a.zip", "sha256": "PENDING", "size": 0 }
            }]
        }"#;
        let catalog = Catalog::parse(raw).expect("parsea");
        let errors = catalog.validate("0.1.0");
        assert!(!errors.is_empty(), "los placeholders deben rechazarse");
    }

    #[test]
    fn shipped_catalog_is_valid() {
        let raw = include_str!("../../../catalog/catalog.json");
        let catalog = Catalog::parse(raw).expect("el catálogo del repo debe parsear");
        let errors = catalog.validate("0.1.0");
        assert!(errors.is_empty(), "errores: {errors:?}");
        assert!(catalog
            .backend(BackendId::Cpu, Platform::LinuxX64)
            .is_some());
        assert!(catalog.model("base").is_some());
    }
}
