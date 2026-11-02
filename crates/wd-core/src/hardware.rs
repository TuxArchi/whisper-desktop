//! Data contract produced by the hardware detector and consumed by the
//! installation planner, the GUI and the diagnostics export.
//!
//! Everything here must stay serializable and stable: a `hardware.json`
//! captured from a user's machine can be replayed later to reproduce a
//! decision without owning that hardware.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the report shape. Bump on breaking changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Free-form, translatable message. `code` identifies the text and `args`
/// fills its placeholders. Keeps the core free of hardcoded prose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl Reason {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_string(),
            args: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, key: &str, value: impl ToString) -> Self {
        self.args.insert(key.to_string(), value.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareReport {
    pub schema: u32,
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub ram_mb: u64,
    #[serde(default)]
    pub disks: Vec<DiskInfo>,
    #[serde(default)]
    pub gpus: Vec<GpuInfo>,
    #[serde(default)]
    pub vulkan: Option<VulkanInfo>,
    #[serde(default)]
    pub nvidia: Option<NvidiaInfo>,
    #[serde(default)]
    pub rocm: Option<RocmInfo>,
    #[serde(default)]
    pub warnings: Vec<Reason>,
}

impl HardwareReport {
    /// GPU with the most VRAM, which is the one we plan for.
    pub fn primary_gpu(&self) -> Option<&GpuInfo> {
        self.gpus
            .iter()
            .max_by_key(|gpu| (gpu.vram_mb.unwrap_or(0), gpu.is_discrete()))
    }

    pub fn gpu_vendors(&self) -> Vec<GpuVendor> {
        let mut out = Vec::new();
        for gpu in &self.gpus {
            if !out.contains(&gpu.vendor) {
                out.push(gpu.vendor);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    /// `windows`, `linux`, `macos` or `unknown`.
    pub family: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub is_wsl: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub physical_cores: u32,
    #[serde(default)]
    pub logical_cores: u32,
    #[serde(default)]
    pub flags: CpuFlags,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuFlags {
    #[serde(default)]
    pub avx2: bool,
    #[serde(default)]
    pub avx512f: bool,
    #[serde(default)]
    pub f16c: bool,
    #[serde(default)]
    pub fma: bool,
}

impl CpuInfo {
    /// whisper.cpp scales with physical cores, not logical ones.
    pub fn thread_count(&self) -> u32 {
        let cores = if self.physical_cores > 0 {
            self.physical_cores
        } else {
            self.logical_cores
        };
        cores.clamp(1, 32)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount: String,
    pub free_mb: u64,
    pub total_mb: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    #[default]
    Other,
}

impl GpuVendor {
    pub fn as_str(self) -> &'static str {
        match self {
            GpuVendor::Nvidia => "nvidia",
            GpuVendor::Amd => "amd",
            GpuVendor::Intel => "intel",
            GpuVendor::Apple => "apple",
            GpuVendor::Other => "other",
        }
    }
}

/// Coarse microarchitecture. We deliberately avoid exact `gfx` numbers for
/// the decision: families are stable and well documented, individual PCI ids
/// drift between board partners.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchFamily {
    #[serde(rename = "rdna1")]
    Rdna1,
    #[serde(rename = "rdna2")]
    Rdna2,
    #[serde(rename = "rdna3")]
    Rdna3,
    #[serde(rename = "rdna4")]
    Rdna4,
    #[serde(rename = "vega")]
    Vega,
    #[serde(rename = "gcn")]
    Gcn,
    #[serde(rename = "cdna")]
    Cdna,
    #[serde(rename = "ryzen-ai")]
    RyzenAi,
    #[serde(rename = "arc")]
    Arc,
    #[serde(rename = "turing")]
    Turing,
    #[serde(rename = "ampere")]
    Ampere,
    #[serde(rename = "ada")]
    Ada,
    #[serde(rename = "blackwell")]
    Blackwell,
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
}

impl ArchFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            ArchFamily::Rdna1 => "rdna1",
            ArchFamily::Rdna2 => "rdna2",
            ArchFamily::Rdna3 => "rdna3",
            ArchFamily::Rdna4 => "rdna4",
            ArchFamily::Vega => "vega",
            ArchFamily::Gcn => "gcn",
            ArchFamily::Cdna => "cdna",
            ArchFamily::RyzenAi => "ryzen-ai",
            ArchFamily::Arc => "arc",
            ArchFamily::Turing => "turing",
            ArchFamily::Ampere => "ampere",
            ArchFamily::Ada => "ada",
            ArchFamily::Blackwell => "blackwell",
            ArchFamily::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    #[default]
    Low,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pci_id: Option<String>,
    #[serde(default)]
    pub vram_mb: Option<u64>,
    #[serde(default)]
    pub driver: Option<String>,
    /// Exact target when available (from `rocminfo`), e.g. `gfx1031`.
    #[serde(default)]
    pub gfx_arch: Option<String>,
    #[serde(default)]
    pub family: ArchFamily,
    #[serde(default)]
    pub confidence: Confidence,
}

impl GpuInfo {
    pub fn is_discrete(&self) -> bool {
        // Integrated parts report small or shared memory; >= 2 GB is a safe
        // heuristic for a dedicated card.
        self.vram_mb.unwrap_or(0) >= 2048
    }

    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{} GPU", self.vendor.as_str()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanInfo {
    #[serde(default)]
    pub api_version: Option<String>,
    #[serde(default)]
    pub device_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaInfo {
    #[serde(default)]
    pub driver_version: Option<String>,
    #[serde(default)]
    pub compute_cap: Option<String>,
    #[serde(default)]
    pub vram_mb: Option<u64>,
}

impl NvidiaInfo {
    /// CUDA 12 builds require driver 525+, but the practical floor for
    /// whisper.cpp is 500-series era drivers.
    pub fn driver_major(&self) -> Option<u32> {
        self.driver_version
            .as_deref()
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.parse::<u32>().ok())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RocmInfo {
    #[serde(default)]
    pub gfx_target: Option<String>,
    #[serde(default)]
    pub kfd_accessible: bool,
    #[serde(default)]
    pub groups_ok: bool,
}
