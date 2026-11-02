//! Installation planning: turns a `HardwareReport` into an `InstallPlan`.
//!
//! All backend and model rules live here and nowhere else. The GUI renders
//! the reasons; it never re-implements the decision.

use super::{BackendId, InstallPlan, ModelVariant};
use crate::hardware::{ArchFamily, GpuVendor, HardwareReport, Reason};

pub fn plan(report: &HardwareReport) -> InstallPlan {
    let mut reasons = Vec::new();
    let mut warnings = report.warnings.clone();

    let (backend, fallbacks) = choose_backend(report, &mut reasons, &mut warnings);
    let (model, variant) = choose_model(report, backend, &mut reasons, &mut warnings);

    InstallPlan {
        backend,
        fallbacks,
        model,
        variant,
        threads: report.cpu.thread_count(),
        reasons,
        warnings,
    }
}

fn choose_backend(
    report: &HardwareReport,
    reasons: &mut Vec<Reason>,
    warnings: &mut Vec<Reason>,
) -> (BackendId, Vec<BackendId>) {
    let primary = report.primary_gpu();

    if let Some(gpu) = primary {
        reasons.push(
            Reason::new("gpu.detected")
                .arg("name", gpu.label())
                .arg("vram", gpu.vram_mb.unwrap_or(0)),
        );
    }

    let is_windows = report.os.family == "windows";
    let is_linux = report.os.family == "linux";

    // --- NVIDIA: CUDA when the driver is usable -------------------------
    if report.gpu_vendors().contains(&GpuVendor::Nvidia) {
        match &report.nvidia {
            Some(nv) => {
                let major = nv.driver_major().unwrap_or(0);
                if major == 0 || major >= 500 {
                    reasons.push(Reason::new("backend.cuda").arg(
                        "driver",
                        nv.driver_version.clone().unwrap_or_else(|| "?".to_string()),
                    ));
                    return (BackendId::Cuda, vec![BackendId::Vulkan, BackendId::Cpu]);
                }
                warnings.push(
                    Reason::new("warn.cuda_driver_old")
                        .arg("driver", nv.driver_version.clone().unwrap_or_default()),
                );
            }
            None => warnings.push(Reason::new("warn.cuda_unavailable")),
        }
    }

    // --- AMD -------------------------------------------------------------
    if let Some(gpu) = primary.filter(|g| g.vendor == GpuVendor::Amd) {
        let family = gpu.family;

        // ROCm on Windows is officially limited to RDNA4 and Ryzen AI.
        // On Linux it needs /dev/kfd access, otherwise the build cannot run.
        let rocm_ok = if is_windows {
            matches!(family, ArchFamily::Rdna4 | ArchFamily::RyzenAi)
        } else if is_linux {
            matches!(
                family,
                ArchFamily::Rdna3 | ArchFamily::Rdna4 | ArchFamily::Cdna
            ) && report
                .rocm
                .as_ref()
                .map(|r| r.kfd_accessible && r.runtime_available)
                .unwrap_or(false)
        } else {
            false
        };

        if rocm_ok {
            reasons.push(Reason::new("backend.rocm").arg("family", family.as_str()));
            return (BackendId::Rocm, vec![BackendId::Vulkan, BackendId::Cpu]);
        }

        if is_windows {
            reasons.push(Reason::new("backend.vulkan.amd_windows").arg("family", family.as_str()));
        } else if matches!(
            family,
            ArchFamily::Rdna3 | ArchFamily::Rdna4 | ArchFamily::Cdna
        ) {
            // ROCm-capable silicon, but the runtime cannot reach the device.
            reasons.push(Reason::new("backend.vulkan.amd_linux").arg("family", family.as_str()));
            if report
                .rocm
                .as_ref()
                .map(|r| r.kfd_accessible && !r.runtime_available)
                .unwrap_or(false)
            {
                warnings.push(Reason::new("warn.rocm_no_runtime"));
            } else {
                warnings.push(Reason::new("warn.rocm_no_kfd"));
            }
        } else {
            // RDNA1/2, Vega, GCN: not in the ROCm support matrix at all.
            reasons
                .push(Reason::new("backend.vulkan.amd_unsupported").arg("family", family.as_str()));
        }
        return (BackendId::Vulkan, vec![BackendId::Cpu]);
    }

    // --- Intel, Apple or anything else with a driver ----------------------
    if let Some(gpu) = primary {
        if report.vulkan.is_none() {
            warnings.push(Reason::new("warn.vulkan_unverified"));
        }
        reasons.push(Reason::new("backend.vulkan.generic").arg("name", gpu.label()));
        return (BackendId::Vulkan, vec![BackendId::Cpu]);
    }

    // --- CPU only ---------------------------------------------------------
    reasons.push(Reason::new("backend.cpu"));
    (BackendId::Cpu, Vec::new())
}

fn choose_model(
    report: &HardwareReport,
    backend: BackendId,
    reasons: &mut Vec<Reason>,
    warnings: &mut Vec<Reason>,
) -> (String, ModelVariant) {
    if backend == BackendId::Cpu {
        let ram = report.ram_mb;
        if ram >= 16_000 && report.cpu.flags.avx2 {
            reasons.push(Reason::new("model.cpu_good").arg("ram", ram));
            return ("small".to_string(), ModelVariant::Q5_0);
        }
        reasons.push(Reason::new("model.cpu_base").arg("ram", ram));
        return ("base".to_string(), ModelVariant::Q5_0);
    }

    let vram = report
        .primary_gpu()
        .and_then(|g| g.vram_mb)
        .or_else(|| report.nvidia.as_ref().and_then(|n| n.vram_mb))
        .unwrap_or(0);

    let (model, variant) = match vram {
        v if v >= 16_000 => ("large-v3", ModelVariant::Fp16),
        v if v >= 12_000 => ("large-v3", ModelVariant::Q5_0),
        v if v >= 8_000 => ("large-v3-turbo", ModelVariant::Fp16),
        v if v >= 6_000 => ("large-v3-turbo", ModelVariant::Q5_0),
        v if v >= 4_000 => ("small", ModelVariant::Fp16),
        _ => ("base", ModelVariant::Q5_0),
    };

    if vram == 0 {
        warnings.push(Reason::new("warn.vram_unknown"));
    }

    reasons.push(
        Reason::new("model.by_vram")
            .arg("vram", vram)
            .arg("model", model)
            .arg("variant", variant.as_str()),
    );

    (model.to_string(), variant)
}
