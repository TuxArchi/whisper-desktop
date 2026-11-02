import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { AppState, HardwareReport, InstallPlan } from "./types";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Every call goes through here. In a plain browser (vite dev without Tauri)
 * we return a demo machine so the UI can be worked on without building the
 * Rust side.
 */
export async function ipc<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    return tauriInvoke<T>(command, args);
  }
  return demo<T>(command);
}

const demoReport: HardwareReport = {
  schema: 1,
  os: { family: "linux", version: "Arch Linux (demo)", arch: "x86_64", isWsl: false },
  cpu: {
    model: "AMD Ryzen 7 5800X 8-Core Processor",
    physicalCores: 8,
    logicalCores: 16,
    flags: { avx2: true, avx512f: false, f16c: true, fma: true },
  },
  ramMb: 32005,
  disks: [{ mount: "/", freeMb: 180000, totalMb: 512000 }],
  gpus: [
    {
      vendor: "amd",
      name: "Navi 23 [Radeon RX 6650 XT]",
      pciId: "1002:73ef",
      vramMb: 8176,
      driver: "amdgpu",
      gfxArch: null,
      family: "rdna2",
      confidence: "high",
    },
  ],
  vulkan: { apiVersion: "1.4.354", deviceCount: 1 },
  nvidia: null,
  rocm: null,
  warnings: [],
};

const demoPlan: InstallPlan = {
  backend: "vulkan",
  fallbacks: ["cpu"],
  model: "large-v3-turbo",
  variant: "fp16",
  threads: 8,
  reasons: [
    { code: "gpu.detected", args: { name: "Navi 23 [Radeon RX 6650 XT]", vram: "8176" } },
    { code: "backend.vulkan.amd_unsupported", args: { family: "rdna2" } },
    { code: "model.by_vram", args: { vram: "8176", model: "large-v3-turbo", variant: "fp16" } },
  ],
  warnings: [],
};

const demoState: AppState = {
  installed: false,
  appVersion: "0.1.0",
  dataDir: "/home/demo/.local/share/whisper-desktop",
  platform: "linux-x86_64",
  portable: false,
  zipPreview: false,
};

async function demo<T>(command: string): Promise<T> {
  // Simulate a little work so the progress UI is visible.
  await new Promise((resolve) => setTimeout(resolve, 250));
  switch (command) {
    case "get_state":
      return demoState as unknown as T;
    case "detect_hardware":
      return demoReport as unknown as T;
    case "build_plan":
      return demoPlan as unknown as T;
    case "catalog_status":
      return {
        ok: true,
        backends: 6,
        recommended: "large-v3-turbo",
        models: [
          { id: "base", size: 147951465, vramMb: 640 },
          { id: "small", size: 487601967, vramMb: 1024 },
          { id: "medium", size: 1533763059, vramMb: 2560 },
          { id: "large-v3-turbo", size: 1624555275, vramMb: 3072 },
          { id: "large-v3", size: 3095033483, vramMb: 5120 },
        ],
        errors: [],
        notes: [],
      } as unknown as T;
    case "start_install":
      throw new Error(
        "Modo demo: el catálogo de desarrollo no tiene binarios reales que descargar.",
      );
    case "verify_installation":
      return { working: "vulkan", rtf: 12.4, tried: ["vulkan"] } as unknown as T;
    case "integrate_system":
      return {
        actions: ["Entrada de menú (demo)", "Comando en PATH (demo)"],
        errors: [],
      } as unknown as T;
    case "unintegrate_system":
      return { actions: ["Eliminado (demo)"], errors: [] } as unknown as T;
    case "studio_state":
      return {
        models: ["ggml-large-v3-turbo.bin", "ggml-small.bin"],
        defaultModel: "ggml-large-v3-turbo.bin",
        outputDir: "/home/demo/Transcripciones",
        formats: ["txt", "srt"],
        pattern: "{nombre}_{fecha}",
        task: "transcribe",
        language: "auto",
        threads: 8,
        backend: "vulkan",
        canRun: true,
        problem: null,
      } as unknown as T;
    case "preview_output":
      return [
        "/home/demo/Transcripciones/reunion_2026-11-01.txt",
        "/home/demo/Transcripciones/reunion_2026-11-01.srt",
      ] as unknown as T;
    case "list_history":
      return [] as unknown as T;
    case "scan_folder":
      return [] as unknown as T;
    case "initial_files":
      return [] as unknown as T;
    case "data_info":
      return {
        dir: "/home/demo/.local/share/whisper-desktop",
        files: 42,
        bytes: 1_800_000_000,
      } as unknown as T;
    case "relocate_data":
      throw new Error("Modo demo: mover datos necesita la app real.");
    case "start_job":
      throw new Error("Modo demo: la transcripción necesita el motor real.");
    case "cancel_job":
      return undefined as unknown as T;
    case "open_path":
      return undefined as unknown as T;
    default:
      throw new Error(`comando desconocido en modo demo: ${command}`);
  }
}
