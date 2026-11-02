// Mirror of the Rust contract. Keep in sync with crates/wd-core/src/hardware.rs
// and decide/mod.rs. Field names match serde's camelCase output.

export type BackendId = "cpu" | "cuda" | "vulkan" | "rocm";
export type ModelVariant = "fp16" | "q5_0";
export type GpuVendor = "nvidia" | "amd" | "intel" | "apple" | "other";

export interface Reason {
  code: string;
  args: Record<string, string>;
}

export interface OsInfo {
  family: string;
  version?: string | null;
  arch: string;
  isWsl: boolean;
}

export interface CpuFlags {
  avx2: boolean;
  avx512f: boolean;
  f16c: boolean;
  fma: boolean;
}

export interface CpuInfo {
  model?: string | null;
  physicalCores: number;
  logicalCores: number;
  flags: CpuFlags;
}

export interface DiskInfo {
  mount: string;
  freeMb: number;
  totalMb: number;
}

export interface GpuInfo {
  vendor: GpuVendor;
  name?: string | null;
  pciId?: string | null;
  vramMb?: number | null;
  driver?: string | null;
  gfxArch?: string | null;
  family: string;
  confidence: "high" | "medium" | "low";
}

export interface VulkanInfo {
  apiVersion?: string | null;
  deviceCount: number;
}

export interface NvidiaInfo {
  driverVersion?: string | null;
  computeCap?: string | null;
  vramMb?: number | null;
}

export interface HardwareReport {
  schema: number;
  os: OsInfo;
  cpu: CpuInfo;
  ramMb: number;
  disks: DiskInfo[];
  gpus: GpuInfo[];
  vulkan?: VulkanInfo | null;
  nvidia?: NvidiaInfo | null;
  rocm?: unknown;
  warnings: Reason[];
}

export interface InstallPlan {
  backend: BackendId;
  fallbacks: BackendId[];
  model: string;
  variant: ModelVariant;
  threads: number;
  reasons: Reason[];
  warnings: Reason[];
}

export interface AppState {
  installed: boolean;
  appVersion: string;
  dataDir: string;
  platform: string;
  portable: boolean;
  zipPreview: boolean;
}

export interface CatalogStatus {
  ok: boolean;
  backends: number;
  models: number;
  errors: string[];
}

export type InstallEvent =
  | { kind: "step"; id: string; label: string }
  | { kind: "progress"; done: number; total: number }
  | { kind: "finished" };

export interface VerifyView {
  working: string | null;
  rtf: number;
  tried: string[];
}

export interface IntegrationReport {
  actions: string[];
  errors: string[];
}

export interface DataInfo {
  dir: string;
  files: number;
  bytes: number;
}

export interface MediaFile {
  path: string;
  /** Subdirectory relative to the scanned folder; empty for picked files. */
  relative: string;
}

export interface StudioState {
  models: string[];
  defaultModel: string;
  outputDir: string;
  formats: string[];
  pattern: string;
  task: string;
  language: string;
  threads: number;
  backend: string;
  canRun: boolean;
  problem: string | null;
}

export type JobEvent =
  | { kind: "started"; index: number; total: number; source: string }
  | { kind: "progress"; index: number; percent: number }
  | {
      kind: "finished";
      index: number;
      ok: boolean;
      outputs: string[];
      error: string | null;
    }
  | { kind: "done"; ok: number; failed: number };

export interface HistoryEntry {
  id: string;
  source: string;
  outputs: string[];
  task: string;
  model: string;
  backend: string | null;
  finishedAt: number;
  audioSeconds: number;
  elapsedSeconds: number;
  rtf: number;
  ok: boolean;
  error: string | null;
}
