import type { Reason } from "./types";

/**
 * Spanish renderer for decision reasons. Mirrors crates/wd-core/src/i18n.rs:
 * the core sends codes + arguments, never prose, so both surfaces stay in
 * sync and adding a language does not touch the rules.
 */
export function formatReason(reason: Reason): string {
  const a = reason.args ?? {};
  switch (reason.code) {
    case "gpu.detected":
      return `GPU detectada: ${a.name} (${a.vram} MB de VRAM)`;
    case "backend.cuda":
      return `NVIDIA con driver ${a.driver} → CUDA (la vía más rápida)`;
    case "backend.rocm":
      return `GPU AMD ${a.family} soportada por ROCm → se usará el backend ROCm`;
    case "backend.vulkan.amd_windows":
      return `AMD ${a.family} en Windows: ROCm no está soportado oficialmente aquí → Vulkan`;
    case "backend.vulkan.amd_linux":
      return `AMD ${a.family} en Linux sin acceso a ROCm → Vulkan`;
    case "backend.vulkan.amd_unsupported":
      return `AMD ${a.family} no está en la matriz de ROCm → Vulkan`;
    case "backend.vulkan.generic":
      return `${a.name} → Vulkan (funciona en cualquier GPU con driver reciente)`;
    case "backend.cpu":
      return "No se detectó GPU utilizable → modo CPU";
    case "model.by_vram":
      return `${a.vram} MB de VRAM permiten ${a.model} en ${a.variant}`;
    case "model.cpu_good":
      return `${a.ram} MB de RAM con AVX2 permiten small cuantizado`;
    case "model.cpu_base":
      return `${a.ram} MB de RAM → base cuantizado (mejor relación calidad/tiempo en CPU)`;
    case "warn.wsl":
      return "Estás en WSL: la GPU del equipo anfitrión no es accesible desde aquí";
    case "warn.no_avx2":
      return "La CPU no tiene AVX2: la transcripción será mucho más lenta";
    case "warn.vulkan_unverified":
      return "No se pudo comprobar Vulkan; se verificará tras instalar";
    case "warn.cuda_driver_old":
      return `Driver NVIDIA ${a.driver} demasiado antiguo para CUDA 12; se usará Vulkan`;
    case "warn.cuda_unavailable":
      return "GPU NVIDIA detectada pero nvidia-smi no responde: falta el driver";
    case "warn.rocm_no_kfd":
      return "ROCm no accesible (/dev/kfd). Añade tu usuario a render y video, y reinicia sesión";
    case "warn.vram_unknown":
      return "No se pudo leer la VRAM; se elige un modelo conservador";
    default:
      return reason.code;
  }
}

export function formatBytes(bytes: number): string {
  if (!bytes) return "—";
  const gb = 1024 ** 3;
  const mb = 1024 ** 2;
  return bytes >= 1e9 ? `${(bytes / gb).toFixed(1)} GB` : `${Math.round(bytes / mb)} MB`;
}
