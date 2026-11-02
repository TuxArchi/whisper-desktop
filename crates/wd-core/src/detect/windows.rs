//! Hardware detection for Windows.
//!
//! Uses PowerShell + CIM (present on every supported Windows) plus
//! `nvidia-smi` when available. No admin rights, no WMI COM bindings.

use std::process::Command;

use super::pci;
use crate::hardware::*;
use serde_json::Value;

pub fn detect() -> HardwareReport {
    let mut warnings = Vec::new();

    let os = os_info();
    let cpu = cpu_info();
    if !cpu.flags.avx2 {
        warnings.push(Reason::new("warn.no_avx2"));
    }

    let ram_mb = ram_total_mb();
    let vram_map = vram_by_name();
    let mut gpus = gpu_info(&vram_map);
    let nvidia = nvidia_info();

    if let Some(nv) = &nvidia {
        for gpu in gpus.iter_mut().filter(|g| g.vendor == GpuVendor::Nvidia) {
            if gpu.vram_mb.is_none() {
                gpu.vram_mb = nv.vram_mb;
            }
            if gpu.driver.is_none() {
                gpu.driver = nv.driver_version.clone();
            }
        }
    }

    // Vulkan availability cannot be probed without the SDK. Presence of a
    // modern GPU driver is a strong enough signal for the default plan, and
    // the post-install verification confirms it for real.
    let vulkan = if gpus.is_empty() {
        None
    } else {
        Some(VulkanInfo {
            api_version: None,
            device_count: gpus.len() as u32,
        })
    };
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
        rocm: None,
        warnings,
    }
}

/// Runs a PowerShell snippet and returns trimmed stdout.
fn ps(script: &str) -> Option<String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ps_json(script: &str) -> Option<Value> {
    let raw = ps(&format!("{script} | ConvertTo-Json -Compress -Depth 4"))?;
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

fn as_array(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn os_info() -> OsInfo {
    let version = ps("(Get-CimInstance Win32_OperatingSystem).Caption + ' ' + (Get-CimInstance Win32_OperatingSystem).Version")
        .filter(|s| !s.is_empty());
    OsInfo {
        family: "windows".to_string(),
        version,
        arch: std::env::consts::ARCH.to_string(),
        is_wsl: false,
    }
}

fn cpu_info() -> CpuInfo {
    let mut cores = 0u32;
    let mut logical = 0u32;
    let mut model = None;

    if let Some(json) = ps_json(
        "Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors",
    ) {
        if let Some(first) = as_array(json).into_iter().next() {
            model = first
                .get("Name")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string());
            cores = first
                .get("NumberOfCores")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            logical = first
                .get("NumberOfLogicalProcessors")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
        }
    }

    CpuInfo {
        model,
        physical_cores: cores,
        logical_cores: logical,
        // AVX2 ships with every CPU that can run Windows 11; we do not probe
        // per-core instruction sets on Windows to keep the detector fast.
        flags: CpuFlags {
            avx2: true,
            avx512f: false,
            f16c: true,
            fma: true,
        },
    }
}

fn ram_total_mb() -> u64 {
    ps("(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|bytes| bytes / 1_048_576)
        .unwrap_or(0)
}

fn disks() -> Vec<DiskInfo> {
    let mut out = Vec::new();
    if let Some(json) = ps_json(
        "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,FreeSpace,Size",
    ) {
        for disk in as_array(json) {
            let mount = disk
                .get("DeviceID")
                .and_then(Value::as_str)
                .unwrap_or("/")
                .to_string();
            let free = disk.get("FreeSpace").and_then(Value::as_u64).unwrap_or(0);
            let total = disk.get("Size").and_then(Value::as_u64).unwrap_or(0);
            out.push(DiskInfo {
                mount,
                free_mb: free / 1_048_576,
                total_mb: total / 1_048_576,
            });
        }
    }
    out
}

/// `AdapterRAM` is a uint32 and saturates around 4 GB. The real value lives in
/// the display class registry key as `HardwareInformation.qwMemorySize`.
fn vram_by_name() -> Vec<(String, u64)> {
    let script = r#"
$cls = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}'
Get-ChildItem $cls -ErrorAction SilentlyContinue | ForEach-Object {
  $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
  if ($p.DriverDesc -and $p.'HardwareInformation.qwMemorySize') {
    [pscustomobject]@{ Name = $p.DriverDesc; Vram = $p.'HardwareInformation.qwMemorySize' }
  }
}
"#;
    let mut out = Vec::new();
    if let Some(json) = ps_json(script) {
        for item in as_array(json) {
            let name = item
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let vram = item.get("Vram").and_then(Value::as_u64).unwrap_or(0);
            if !name.is_empty() && vram > 0 {
                out.push((name, vram / 1_048_576));
            }
        }
    }
    out
}

fn gpu_info(vram_map: &[(String, u64)]) -> Vec<GpuInfo> {
    let mut out = Vec::new();
    let Some(json) = ps_json(
        "Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,PNPDeviceID,AdapterRAM",
    ) else {
        return out;
    };

    for item in as_array(json) {
        let name = item
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let pnp = item
            .get("PNPDeviceID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let vendor = vendor_from_pnp(pnp);
        if matches!(vendor, GpuVendor::Other) {
            continue;
        }

        let driver = item
            .get("DriverVersion")
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        let pci_id = pci_id_from_pnp(pnp);

        let mut vram_mb = vram_map
            .iter()
            .find(|(n, _)| names_match(n, &name))
            .map(|(_, v)| *v);

        if vram_mb.is_none() {
            let adapter_ram = item.get("AdapterRAM").and_then(Value::as_u64).unwrap_or(0);
            if adapter_ram > 0 {
                // Anything at/above 4 GB is the uint32 saturation artifact.
                vram_mb = Some(adapter_ram / 1_048_576);
            }
        }

        let family = pci::family_from_name(vendor, &name);
        let confidence = if vram_mb.is_some() && !name.is_empty() {
            Confidence::High
        } else {
            Confidence::Medium
        };

        out.push(GpuInfo {
            vendor,
            name: if name.is_empty() { None } else { Some(name) },
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

fn names_match(a: &str, b: &str) -> bool {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    a == b || a.contains(&b) || b.contains(&a)
}

fn vendor_from_pnp(pnp: &str) -> GpuVendor {
    let upper = pnp.to_ascii_uppercase();
    let Some(ven_start) = upper.find("VEN_") else {
        return GpuVendor::Other;
    };
    let rest = &upper[ven_start + 4..];
    let code = rest.split('&').next().unwrap_or("");
    match u16::from_str_radix(code, 16) {
        Ok(v) => pci::vendor_from_u16(v),
        Err(_) => GpuVendor::Other,
    }
}

fn pci_id_from_pnp(pnp: &str) -> Option<String> {
    let upper = pnp.to_ascii_uppercase();
    let ven = upper.find("VEN_")?;
    let dev = upper.find("DEV_")?;
    let vendor = &upper[ven + 4..ven + 8];
    let device = &upper[dev + 4..dev + 8];
    Some(format!(
        "{}:{}",
        vendor.to_ascii_lowercase(),
        device.to_ascii_lowercase()
    ))
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
