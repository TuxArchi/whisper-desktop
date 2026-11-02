//! Decision types shared by the GUI, the CLI and the tests.

mod model;
mod rules;

pub use model::{download_size, file_name, spec, ModelSpec, ModelVariant, MODELS};
pub use rules::plan;

use serde::{Deserialize, Serialize};

/// Inference backend selected for this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendId {
    #[serde(rename = "cpu")]
    Cpu,
    #[serde(rename = "cuda")]
    Cuda,
    #[serde(rename = "vulkan")]
    Vulkan,
    #[serde(rename = "rocm")]
    Rocm,
}

impl BackendId {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendId::Cpu => "cpu",
            BackendId::Cuda => "cuda",
            BackendId::Vulkan => "vulkan",
            BackendId::Rocm => "rocm",
        }
    }
}

impl std::fmt::Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the installer must download and configure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub backend: BackendId,
    /// Backends to try in order if the verification step does not confirm
    /// the primary one.
    pub fallbacks: Vec<BackendId>,
    pub model: String,
    pub variant: ModelVariant,
    pub threads: u32,
    /// Human-readable, translatable justification shown in the wizard.
    pub reasons: Vec<crate::hardware::Reason>,
    pub warnings: Vec<crate::hardware::Reason>,
}

impl InstallPlan {
    pub fn model_file(&self) -> String {
        file_name(&self.model, self.variant)
    }

    pub fn download_size(&self) -> u64 {
        model::download_size(&self.model, self.variant)
    }
}
