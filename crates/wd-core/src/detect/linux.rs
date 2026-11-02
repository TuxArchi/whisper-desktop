//! Hardware detection for Linux.
//!
//! Everything is read from `/sys`, `/proc` and (optionally) `lspci`,
//! `nvidia-smi`, `rocminfo` and `vulkaninfo`. No root, no extra crates.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::pci;
use crate::hardware::*;

pub fn detect() -> HardwareReport {
    let mut warnings = Vec::new();

    let os = os_info();
    if os.is_wsl {
        warnings.push(Reason::new("warn.wsl"));
    }

    let cpu = cpu_info();
    if !cpu.flags.avx2 {
        warnings.push(Reason::new("warn.no_avx2"));
    }

    let ram_mb = mem_total_mb();
    let mut gpus = gpu_info();
    let nvidia = nvidia_info();

    // nvidia-smi is the only reliable source of VRAM for NVIDIA on Linux.
    if let Some(nv) = &nvidia {
        for gpu in gpus.iter_mut().filter(|g| g.vendor == GpuVendor::Nvidia) {
            if gpu.vram_mb.is_none() {
                gpu.vram_mb = nv.vram_mb;
            }
            if gpu.driver.is_none() {
                gpu.driver = nv.driver_version.clone();
            }
        }
        if !gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia) {
            gpus.push(GpuInfo {
                vendor: GpuVendor::Nvidia,
                name: Some("NVIDIA GPU".to_string()),
                vram_mb: nv.vram_mb,
                driver: nv.driver_version.clone(),
                family: ArchFamily::Unknown,
                confidence: Confidence::Low,
                ..Default::default()
            });
        }
    }

    let rocm = rocm_info();
    let rocm_target = rocm
        .as_ref()
        .and_then(|info| info.gfx_target.as_ref())
        .cloned();
    if let Some(target) = rocm_target {
        for gpu in gpus.iter_mut().filter(|g| g.vendor == GpuVendor::Amd) {
            if gpu.gfx_arch.is_none() {
                gpu.gfx_arch = Some(target.clone());
            }
        }
    }

    let vulkan = vulkan_info();
    if vulkan.is_none() && !gpus.is_empty() {
        warnings.push(Reason::new("warn.vulkan_unverified"));
    }

    HardwareReport {
        schema: SCHEMA_VERSION,
        os,
        cpu,
        ram_mb,
        disks: disks(),
        gpus,
        vulkan,
        nvidia,
        rocm,
        warnings,
    }
}

fn os_info() -> OsInfo {
    let version = fs::read_to_string("/etc/os-release").ok().and_then(|text| {
        text.lines()
            .find(|l| l.starts_with("PRETTY_NAME="))
            .map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
    });
    let is_wsl = fs::read_to_string("/proc/version")
        .map(|v| v.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false);
    OsInfo {
        family: "linux".to_string(),
        version,
        arch: std::env::consts::ARCH.to_string(),
        is_wsl,
    }
}

fn read_trim(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn cpu_info() -> CpuInfo {
    let text = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let mut model = None;
    let mut flags_line = String::new();
    let mut logical = 0u32;
    let mut cores: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut phys_id = 0u32;

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "processor" => logical += 1,
            "model name" if model.is_none() => model = Some(value.to_string()),
            "flags" => flags_line = value.to_string(),
            "physical id" => phys_id = value.parse().unwrap_or(0),
            "core id" => {
                let core: u32 = value.parse().unwrap_or(0);
                cores.insert((phys_id, core));
            }
            _ => {}
        }
    }

    let flags: Vec<&str> = flags_line.split_whitespace().collect();
    let physical_cores = if cores.is_empty() {
        logical
    } else {
        cores.len() as u32
    };

    CpuInfo {
        model,
        physical_cores,
        logical_cores: logical,
        flags: CpuFlags {
            avx2: flags.contains(&"avx2"),
            avx512f: flags.contains(&"avx512f"),
            f16c: flags.contains(&"f16c"),
            fma: flags.contains(&"fma"),
        },
    }
}

fn mem_total_mb() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<u64>().ok())
                .map(|kb| kb / 1024)
        })
        .unwrap_or(0)
}

/// Parses `lspci -nn` into `pci-id -> human name`.
fn lspci_names() -> Vec<(String, String)> {
    let output = match Command::new("lspci").arg("-nn").output() {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().filter_map(parse_lspci_line).collect()
}

fn parse_lspci_line(line: &str) -> Option<(String, String)> {
    let end = line.rfind(']')?;
    let start = line[..end].rfind('[')?;
    let id = line[start + 1..end].to_ascii_lowercase();
    if !id.contains(':') {
        return None;
    }
    let before = line[..start].trim();
    let name = match before.split_once(": ") {
        Some((_, rest)) => rest.to_string(),
        None => before.to_string(),
    };
    Some((id, clean_name(&name)))
}

/// `lspci` repeats the vendor: "Advanced Micro Devices, Inc. [AMD/ATI] Navi 23
/// [Radeon RX 6650 XT]". The vendor half is noise in the UI.
fn clean_name(name: &str) -> String {
    const PREFIXES: &[&str] = &[
        "Advanced Micro Devices, Inc. [AMD/ATI] ",
        "Advanced Micro Devices, Inc. [AMD] ",
        "NVIDIA Corporation ",
        "Intel Corporation ",
    ];
    let mut value = name.trim();
    for prefix in PREFIXES {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest;
            break;
        }
    }
    value.to_string()
}

fn gpu_info() -> Vec<GpuInfo> {
    let names = lspci_names();
    let mut out = Vec::new();

    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return out;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !is_card_entry(&file_name) {
            continue;
        }
        let base = format!("/sys/class/drm/{file_name}/device");

        let vendor_id = read_trim(&format!("{base}/vendor"))
            .and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok());
        let device_id = read_trim(&format!("{base}/device"))
            .and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok());

        let vendor = match vendor_id {
            Some(v) => pci::vendor_from_u16(v),
            None => continue,
        };
        if matches!(vendor, GpuVendor::Other) {
            continue;
        }

        let pci_id = match (vendor_id, device_id) {
            (Some(v), Some(d)) => Some(format!("{v:04x}:{d:04x}")),
            _ => None,
        };

        let name = pci_id
            .as_ref()
            .and_then(|id| names.iter().find(|(k, _)| k == id).map(|(_, n)| n.clone()))
            .or_else(|| read_trim(&format!("{base}/product")).filter(|s| !s.is_empty()));

        let driver = fs::read_link(format!("{base}/driver"))
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));

        let vram_mb = read_trim(&format!("{base}/mem_info_vram_total"))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|bytes| bytes / 1_048_576);

        let family = match &name {
            Some(n) => pci::family_from_name(vendor, n),
            None => ArchFamily::Unknown,
        };

        let confidence = if name.is_some() && vram_mb.is_some() {
            Confidence::High
        } else if name.is_some() {
            Confidence::Medium
        } else {
            Confidence::Low
        };

        out.push(GpuInfo {
            vendor,
            name,
            pci_id,
            vram_mb,
            driver,
            gfx_arch: None,
            family,
            confidence,
        });
    }

    out
}

fn is_card_entry(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("card") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

fn nvidia_info() -> Option<NvidiaInfo> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version,compute_cap",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?;
    let cols: Vec<&str> = first.split(',').map(|c| c.trim()).collect();
    if cols.len() < 3 {
        return None;
    }
    Some(NvidiaInfo {
        driver_version: Some(cols[2].to_string()),
        compute_cap: cols.get(3).map(|s| s.to_string()),
        vram_mb: cols[1].parse::<u64>().ok(),
    })
}

fn rocm_info() -> Option<RocmInfo> {
    let has_amdgpu = Path::new("/sys/module/amdgpu").exists();
    let gfx_target = rocminfo_gfx();
    if !has_amdgpu && gfx_target.is_none() {
        return None;
    }
    let kfd_accessible = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kfd")
        .is_ok();
    Some(RocmInfo {
        gfx_target,
        kfd_accessible,
        groups_ok: kfd_accessible,
    })
}

fn rocminfo_gfx() -> Option<String> {
    let output = Command::new("rocminfo").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        for token in line.split_whitespace() {
            if token.starts_with("gfx") && token.len() > 4 {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn vulkan_info() -> Option<VulkanInfo> {
    let output = Command::new("vulkaninfo").arg("--summary").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let api_version = text
        .lines()
        .find(|l| l.contains("apiVersion"))
        .and_then(|l| l.split('=').nth(1))
        .map(|s| s.trim().to_string());
    let device_count = text
        .lines()
        .filter(|l| l.trim_start().starts_with("deviceName"))
        .count() as u32;
    Some(VulkanInfo {
        api_version,
        device_count,
    })
}

fn disks() -> Vec<DiskInfo> {
    let output = match Command::new("df").args(["-B1", "-P", "/"]).output() {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let total: u64 = cols[1].parse().unwrap_or(0);
        let free: u64 = cols[3].parse().unwrap_or(0);
        out.push(DiskInfo {
            mount: cols[cols.len() - 1].to_string(),
            total_mb: total / 1_048_576,
            free_mb: free / 1_048_576,
        });
    }
    out
}
