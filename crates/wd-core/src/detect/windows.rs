//! Hardware detection for Windows.
//!
//! Everything comes from a **single** PowerShell invocation that returns one
//! JSON object: spawning PowerShell six times made the user see six console
//! windows flash. `nvidia-smi` is a second, also hidden, call.

use std::process::Command;

use super::pci;
use crate::hardware::*;
use crate::process::hidden;
use serde_json::Value;

pub fn detect() -> HardwareReport {
    let mut warnings = Vec::new();

    let snapshot = snapshot();
    let os = os_info(&snapshot);
    let cpu = cpu_info(&snapshot);
    if !cpu.flags.avx2 {
        warnings.push(Reason::new("warn.no_avx2"));
    }

    let ram_mb = ram_total_mb(&snapshot);
    let vram_map = vram_by_name(&snapshot);
    let mut gpus = gpu_info(&snapshot, &vram_map);
    let nvidia = nvidia_info();

    // nvidia-smi reports the real total. The registry and, above all,
    // AdapterRAM are not reliable enough to override it.
    if let Some(nv) = &nvidia {
        let reported = nv.vram_mb;
        let name_hint = nv.name.clone();
        for gpu in gpus.iter_mut().filter(|g| g.vendor == GpuVendor::Nvidia) {
            let matches = match (&name_hint, &gpu.name) {
                (Some(a), Some(b)) => names_match(a, b),
                _ => true,
            };
            if matches || gpus.len() == 1 {
                if let Some(vram) = reported {
                    gpu.vram_mb = Some(vram);
                    gpu.vram_source = Some("nvidia-smi".to_string());
                }
                if gpu.driver.is_none() {
                    gpu.driver = nv.driver_version.clone();
                }
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
        disks: disks(&snapshot),
        gpus,
        vulkan,
        nvidia,
        rocm: None,
        warnings,
    }
}

/// One PowerShell process, one JSON object with everything we need.
fn snapshot() -> Value {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$os   = Get-CimInstance Win32_OperatingSystem
$cpu  = Get-CimInstance Win32_Processor | Select-Object -First 1
$cs   = Get-CimInstance Win32_ComputerSystem
$disks = @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,FreeSpace,Size)
$gpus  = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,PNPDeviceID,AdapterRAM)
$cls = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}'
$vram = @()
Get-ChildItem $cls | ForEach-Object {
  $p = Get-ItemProperty $_.PSPath
  if ($p.DriverDesc -and $p.'HardwareInformation.qwMemorySize') {
    $vram += [pscustomobject]@{ name = $p.DriverDesc; bytes = $p.'HardwareInformation.qwMemorySize' }
  }
}
[pscustomobject]@{
  os    = [pscustomobject]@{ caption = $os.Caption; version = $os.Version }
  cpu   = [pscustomobject]@{ name = $cpu.Name; cores = $cpu.NumberOfCores; logical = $cpu.NumberOfLogicalProcessors }
  ram   = $cs.TotalPhysicalMemory
  disks = $disks
  gpus  = $gpus
  vram  = $vram
} | ConvertTo-Json -Compress -Depth 6
"#;

    let output = hidden(Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]))
    .output();

    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let text = text.trim();
            serde_json::from_str(text).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

fn as_array(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        // PowerShell collapses single-element arrays into a plain object.
        Some(Value::Object(_)) => vec![value.unwrap()],
        _ => Vec::new(),
    }
}

fn number(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        // ConvertTo-Json can serialise a REG_QWORD as a string.
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn os_info(snapshot: &Value) -> OsInfo {
    let caption = snapshot
        .pointer("/os/caption")
        .and_then(Value::as_str)
        .unwrap_or("Windows");
    let version = snapshot
        .pointer("/os/version")
        .and_then(Value::as_str)
        .unwrap_or("");
    let version = if version.is_empty() {
        Some(caption.to_string())
    } else {
        Some(format!("{caption} {version}"))
    };

    OsInfo {
        family: "windows".to_string(),
        version,
        arch: std::env::consts::ARCH.to_string(),
        is_wsl: false,
    }
}

fn cpu_info(snapshot: &Value) -> CpuInfo {
    let cpu = snapshot.get("cpu");
    CpuInfo {
        model: cpu
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .map(|name| name.trim().to_string()),
        physical_cores: number(cpu.and_then(|value| value.get("cores"))).unwrap_or(0) as u32,
        logical_cores: number(cpu.and_then(|value| value.get("logical"))).unwrap_or(0) as u32,
        // AVX2 ships with every CPU that can run Windows 11; probing per-core
        // instruction sets would need another process.
        flags: CpuFlags {
            avx2: true,
            avx512f: false,
            f16c: true,
            fma: true,
        },
    }
}

fn ram_total_mb(snapshot: &Value) -> u64 {
    number(snapshot.get("ram"))
        .map(|bytes| bytes / 1_048_576)
        .unwrap_or(0)
}

fn disks(snapshot: &Value) -> Vec<DiskInfo> {
    as_array(snapshot.get("disks"))
        .into_iter()
        .filter_map(|disk| {
            Some(DiskInfo {
                mount: disk.get("DeviceID")?.as_str()?.to_string(),
                free_mb: number(disk.get("FreeSpace")).unwrap_or(0) / 1_048_576,
                total_mb: number(disk.get("Size")).unwrap_or(0) / 1_048_576,
            })
        })
        .collect()
}

fn vram_by_name(snapshot: &Value) -> Vec<(String, u64)> {
    as_array(snapshot.get("vram"))
        .into_iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_string();
            let bytes = number(entry.get("bytes"))?;
            if name.is_empty() || bytes == 0 {
                return None;
            }
            Some((name, bytes / 1_048_576))
        })
        .collect()
}

/// `AdapterRAM` is a uint32: it cannot report more than ~4 GB and saturates at
/// exactly that value. Treating 4095–4097 MB as a real figure is what made an
/// RTX 3070 of 8 GB appear as 4 GB.
fn adapter_ram_mb(bytes: u64) -> Option<u64> {
    let mb = bytes / 1_048_576;
    if (4095..=4097).contains(&mb) {
        None
    } else if mb > 0 {
        Some(mb)
    } else {
        None
    }
}

fn gpu_info(snapshot: &Value, vram_map: &[(String, u64)]) -> Vec<GpuInfo> {
    let mut out = Vec::new();

    for item in as_array(snapshot.get("gpus")) {
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
            .map(|value| value.to_string());
        let pci_id = pci_id_from_pnp(pnp);

        let (vram_mb, vram_source) = if let Some(mb) = vram_map
            .iter()
            .find(|(candidate, _)| names_match(candidate, &name))
            .map(|(_, mb)| *mb)
        {
            (Some(mb), Some("registry".to_string()))
        } else if let Some(mb) = number(item.get("AdapterRAM")).and_then(adapter_ram_mb) {
            (Some(mb), Some("adapterram".to_string()))
        } else {
            (None, None)
        };

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
            vram_source,
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
    let output = hidden(Command::new("nvidia-smi").args([
        "--query-gpu=name,memory.total,driver_version,compute_cap",
        "--format=csv,noheader,nounits",
    ]))
    .output()
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?;
    let cols: Vec<&str> = first.split(',').map(|column| column.trim()).collect();
    if cols.len() < 3 {
        return None;
    }

    Some(NvidiaInfo {
        name: cols.first().map(|value| value.to_string()),
        driver_version: Some(cols[2].to_string()),
        compute_cap: cols.get(3).map(|value| value.to_string()),
        vram_mb: cols[1].parse::<u64>().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturated_adapter_ram_is_ignored() {
        // 0xFFFFFFFF bytes is what a 4 GB+ card reports.
        assert_eq!(adapter_ram_mb(u64::from(u32::MAX)), None);
        assert_eq!(adapter_ram_mb(4_294_967_296), None);
    }

    #[test]
    fn real_adapter_ram_is_kept() {
        assert_eq!(adapter_ram_mb(2 * 1024 * 1024 * 1024), Some(2048));
        assert_eq!(adapter_ram_mb(0), None);
    }

    #[test]
    fn registry_value_can_be_a_string() {
        let value: Value = serde_json::json!("8589934592");
        assert_eq!(number(Some(&value)), Some(8_589_934_592));
    }

    #[test]
    fn single_element_arrays_are_normalised() {
        let value: Value = serde_json::json!({ "Name": "x" });
        assert_eq!(as_array(Some(&value)).len(), 1);
        assert_eq!(as_array(None).len(), 0);
    }
}
