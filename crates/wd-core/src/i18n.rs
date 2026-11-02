//! Minimal translation layer for decision reasons.
//!
//! The core produces codes plus arguments, never prose. Rendering happens
//! here (CLI) or in the frontend (GUI), which keeps the rules i18n-clean.

use crate::hardware::Reason;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Es,
    En,
}

impl Lang {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" => Lang::En,
            _ => Lang::Es,
        }
    }
}

fn arg<'a>(reason: &'a Reason, key: &str) -> &'a str {
    reason.args.get(key).map(String::as_str).unwrap_or("")
}

pub fn render(reason: &Reason, lang: Lang) -> String {
    let es = lang == Lang::Es;
    let vram = arg(reason, "vram");
    let model = arg(reason, "model");
    let variant = arg(reason, "variant");

    match reason.code.as_str() {
        "gpu.detected" => {
            if es {
                format!(
                    "GPU detectada: {} ({} MB de VRAM)",
                    arg(reason, "name"),
                    vram
                )
            } else {
                format!("Detected GPU: {} ({} MB VRAM)", arg(reason, "name"), vram)
            }
        }
        "backend.cuda" => {
            if es {
                format!(
                    "NVIDIA con driver {} → CUDA (la vía más rápida)",
                    arg(reason, "driver")
                )
            } else {
                format!(
                    "NVIDIA with driver {} → CUDA (fastest path)",
                    arg(reason, "driver")
                )
            }
        }
        "backend.rocm" => {
            if es {
                format!(
                    "GPU AMD {} soportada por ROCm → se usará el backend ROCm",
                    arg(reason, "family")
                )
            } else {
                format!(
                    "AMD {} GPU supported by ROCm → ROCm backend",
                    arg(reason, "family")
                )
            }
        }
        "backend.vulkan.amd_windows" => {
            if es {
                format!(
                    "AMD {} en Windows: ROCm no está soportado oficialmente aquí → Vulkan",
                    arg(reason, "family")
                )
            } else {
                format!(
                    "AMD {} on Windows: ROCm is not officially supported here → Vulkan",
                    arg(reason, "family")
                )
            }
        }
        "backend.vulkan.amd_linux" => {
            if es {
                format!(
                    "AMD {} en Linux sin acceso a ROCm → Vulkan",
                    arg(reason, "family")
                )
            } else {
                format!(
                    "AMD {} on Linux without ROCm access → Vulkan",
                    arg(reason, "family")
                )
            }
        }
        "backend.vulkan.amd_unsupported" => {
            if es {
                format!(
                    "AMD {} no está en la matriz de ROCm → Vulkan",
                    arg(reason, "family")
                )
            } else {
                format!(
                    "AMD {} is not in the ROCm support matrix → Vulkan",
                    arg(reason, "family")
                )
            }
        }
        "backend.vulkan.generic" => {
            if es {
                format!(
                    "{} → Vulkan (funciona en cualquier GPU con driver reciente)",
                    arg(reason, "name")
                )
            } else {
                format!(
                    "{} → Vulkan (works on any GPU with a recent driver)",
                    arg(reason, "name")
                )
            }
        }
        "backend.cpu" => {
            if es {
                "No se detectó GPU utilizable → modo CPU".to_string()
            } else {
                "No usable GPU detected → CPU mode".to_string()
            }
        }
        "model.by_vram" => {
            if es {
                format!("{} MB de VRAM permiten {} en {}", vram, model, variant)
            } else {
                format!("{} MB VRAM allow {} at {}", vram, model, variant)
            }
        }
        "model.cpu_good" => {
            if es {
                format!(
                    "{} MB de RAM con AVX2 permiten small cuantizado",
                    arg(reason, "ram")
                )
            } else {
                format!(
                    "{} MB RAM with AVX2 allows quantised small",
                    arg(reason, "ram")
                )
            }
        }
        "model.cpu_base" => {
            if es {
                format!(
                    "{} MB de RAM → base cuantizado (mejor relación calidad/tiempo en CPU)",
                    arg(reason, "ram")
                )
            } else {
                format!(
                    "{} MB RAM → quantised base (best quality/time on CPU)",
                    arg(reason, "ram")
                )
            }
        }
        "warn.wsl" => {
            if es {
                "Estás en WSL: la GPU del equipo anfitrión no es accesible desde aquí".to_string()
            } else {
                "Running under WSL: the host GPU is not reachable from here".to_string()
            }
        }
        "warn.no_avx2" => {
            if es {
                "La CPU no tiene AVX2: la transcripción será mucho más lenta".to_string()
            } else {
                "CPU lacks AVX2: transcription will be much slower".to_string()
            }
        }
        "warn.vulkan_unverified" => {
            if es {
                "No se pudo comprobar Vulkan; se asume disponible y se verificará tras instalar"
                    .to_string()
            } else {
                "Vulkan could not be probed; assuming available, verified after install".to_string()
            }
        }
        "warn.cuda_driver_old" => {
            if es {
                format!(
                    "Driver NVIDIA {} demasiado antiguo para CUDA 12; se usará Vulkan",
                    arg(reason, "driver")
                )
            } else {
                format!(
                    "NVIDIA driver {} too old for CUDA 12; falling back to Vulkan",
                    arg(reason, "driver")
                )
            }
        }
        "warn.cuda_unavailable" => {
            if es {
                "GPU NVIDIA detectada pero nvidia-smi no responde: falta el driver".to_string()
            } else {
                "NVIDIA GPU found but nvidia-smi does not respond: driver missing".to_string()
            }
        }
        "warn.rocm_no_kfd" => {
            if es {
                "ROCm no accesible (/dev/kfd). Añade tu usuario a render y video, y reinicia sesión"
                    .to_string()
            } else {
                "ROCm not accessible (/dev/kfd). Add your user to render and video, then log out"
                    .to_string()
            }
        }
        "warn.vram_unknown" => {
            if es {
                "No se pudo leer la VRAM; se elige un modelo conservador".to_string()
            } else {
                "VRAM could not be read; choosing a conservative model".to_string()
            }
        }
        other => other.to_string(),
    }
}
