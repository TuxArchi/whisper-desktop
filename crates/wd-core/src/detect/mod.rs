//! Hardware detection entry point.

pub mod pci;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::hardware::HardwareReport;

/// Detects the current machine. Never fails: unknown pieces are reported as
/// missing data with `confidence: low` instead of aborting.
pub fn detect() -> HardwareReport {
    #[cfg(target_os = "linux")]
    {
        linux::detect()
    }

    #[cfg(target_os = "windows")]
    {
        windows::detect()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        unsupported()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn unsupported() -> HardwareReport {
    use crate::hardware::{CpuFlags, CpuInfo, OsInfo, SCHEMA_VERSION};
    HardwareReport {
        schema: SCHEMA_VERSION,
        os: OsInfo {
            family: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            ..Default::default()
        },
        cpu: CpuInfo {
            flags: CpuFlags::default(),
            ..Default::default()
        },
        ..Default::default()
    }
}
