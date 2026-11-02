//! Post-install benchmark.
//!
//! The plan says "Vulkan"; this module proves it. It synthesises a short WAV,
//! runs the installed `whisper-cli` on it, reads the backend line from the log
//! and measures the real time factor.
//!
//! If the backend that actually ran is not the one we invoked (a GPU that
//! silently falls back to CPU is extremely common), the caller walks
//! `plan.fallbacks` until something works.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::decide::{BackendId, InstallPlan};
use crate::paths::AppPaths;
use crate::verify::{parse_backend, realtime_factor, VerifyReport};

/// Audio used for the benchmark. Long enough for whisper to engage the
/// encoder, short enough that the user does not wait.
pub const BENCH_SECONDS: f32 = 10.0;
pub const BENCH_FILE: &str = "bench.wav";

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("no se pudo ejecutar el motor: {0}")]
    Spawn(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub elapsed_seconds: f64,
}

/// Injectable so the flow can be tested without a GPU, whisper.cpp or even a
/// real process.
pub trait CliRunner {
    fn run(&self, command: &BenchCommand) -> Result<RunOutput, BenchError>;
}

pub struct SystemRunner;

impl CliRunner for SystemRunner {
    fn run(&self, command: &BenchCommand) -> Result<RunOutput, BenchError> {
        let started = Instant::now();
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        for (key, value) in &command.env {
            process.env(key, value);
        }
        // Never inherit stdin: a CLI waiting for input would hang the wizard.
        process.stdin(std::process::Stdio::null());

        let output = process
            .output()
            .map_err(|error| BenchError::Spawn(error.to_string()))?;

        Ok(RunOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            elapsed_seconds: started.elapsed().as_secs_f64(),
        })
    }
}

pub fn backend_binary(backend_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        backend_dir.join("whisper-cli.exe")
    } else {
        backend_dir.join("whisper-cli")
    }
}

pub fn build_command(
    program: PathBuf,
    model: &Path,
    wav: &Path,
    threads: u32,
    language: &str,
) -> BenchCommand {
    BenchCommand {
        program,
        args: vec![
            "-m".to_string(),
            model.to_string_lossy().to_string(),
            "-f".to_string(),
            wav.to_string_lossy().to_string(),
            "-t".to_string(),
            threads.max(1).to_string(),
            "-l".to_string(),
            language.to_string(),
        ],
        env: Vec::new(),
    }
}

/// Writes a silent mono 16 kHz PCM WAV. Silence is intentional: it keeps the
/// run fast and deterministic, and we only care about which backend loaded.
pub fn write_silence_wav(path: &Path, seconds: f32) -> Result<f64, BenchError> {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BITS: u16 = 16;

    let samples = (seconds.max(0.1) * SAMPLE_RATE as f32) as u32;
    let data_size = samples * u32::from(CHANNELS) * u32::from(BITS / 8);
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS / 8);
    let block_align = CHANNELS * (BITS / 8);

    let mut out = Vec::with_capacity(44 + data_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend(std::iter::repeat(0u8).take(data_size as usize));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &out)?;
    Ok(samples as f64 / f64::from(SAMPLE_RATE))
}

/// Parses a PCM WAV header and returns its duration in seconds.
pub fn wav_duration_seconds(path: &Path) -> Option<f64> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }

    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    let mut data_size = 0u32;
    let mut position = 12usize;

    while position + 8 <= data.len() {
        let id = &data[position..position + 4];
        let size = u32::from_le_bytes([
            data[position + 4],
            data[position + 5],
            data[position + 6],
            data[position + 7],
        ]) as usize;
        let body = position + 8;

        if id == b"fmt " && body + 16 <= data.len() {
            channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
            sample_rate = u32::from_le_bytes([
                data[body + 4],
                data[body + 5],
                data[body + 6],
                data[body + 7],
            ]);
            bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
        } else if id == b"data" {
            data_size = size as u32;
        }

        position = body + size + (size % 2);
    }

    if sample_rate == 0 || channels == 0 || bits == 0 || data_size == 0 {
        return None;
    }
    let bytes_per_second = f64::from(sample_rate) * f64::from(channels) * f64::from(bits / 8);
    if bytes_per_second <= 0.0 {
        return None;
    }
    Some(f64::from(data_size) / bytes_per_second)
}

pub struct VerificationOutcome {
    pub working: Option<BackendId>,
    pub report: Option<VerifyReport>,
    pub tried: Vec<BackendId>,
}

/// Runs the benchmark against one installed backend. Returns `None` when that
/// backend is not present on disk.
pub fn verify_backend(
    paths: &AppPaths,
    backend: BackendId,
    model: &Path,
    threads: u32,
    runner: &dyn CliRunner,
) -> Option<VerifyReport> {
    let binary = backend_binary(&paths.backend_dir(backend.as_str()));
    if !binary.exists() {
        return None;
    }

    let wav = paths.data.join(BENCH_FILE);
    let seconds = write_silence_wav(&wav, BENCH_SECONDS).ok()?;
    let command = build_command(binary, model, &wav, threads, "auto");
    let output = runner.run(&command).ok()?;

    let log = format!("{}\n{}", output.stdout, output.stderr);
    let seen = parse_backend(&log);
    let audio_seconds = wav_duration_seconds(&wav).unwrap_or(seconds);

    Some(VerifyReport {
        backend_seen: seen,
        rtf: realtime_factor(audio_seconds, output.elapsed_seconds),
        matches_plan: seen == Some(backend),
    })
}

/// Walks `plan.backend` and its fallbacks, returning the first one that both
/// runs and reports itself.
pub fn verify_install(
    paths: &AppPaths,
    plan: &InstallPlan,
    model: &Path,
    threads: u32,
    runner: &dyn CliRunner,
) -> VerificationOutcome {
    let mut tried = Vec::new();

    for candidate in std::iter::once(plan.backend).chain(plan.fallbacks.iter().copied()) {
        if tried.contains(&candidate) {
            continue;
        }
        tried.push(candidate);

        if let Some(report) = verify_backend(paths, candidate, model, threads, runner) {
            if report.matches_plan && report.rtf > 0.0 {
                return VerificationOutcome {
                    working: Some(candidate),
                    report: Some(report),
                    tried,
                };
            }
        }
    }

    VerificationOutcome {
        working: None,
        report: None,
        tried,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decide::ModelVariant;
    use std::cell::RefCell;

    struct FakeRunner {
        log: String,
        elapsed: f64,
        calls: RefCell<Vec<String>>,
    }

    impl FakeRunner {
        fn new(log: &str, elapsed: f64) -> Self {
            Self {
                log: log.to_string(),
                elapsed,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CliRunner for FakeRunner {
        fn run(&self, command: &BenchCommand) -> Result<RunOutput, BenchError> {
            self.calls.borrow_mut().push(format!("{:?}", command.args));
            Ok(RunOutput {
                stdout: String::new(),
                stderr: self.log.clone(),
                success: true,
                elapsed_seconds: self.elapsed,
            })
        }
    }

    fn plan(backend: BackendId, fallbacks: Vec<BackendId>) -> InstallPlan {
        InstallPlan {
            backend,
            fallbacks,
            model: "small".into(),
            variant: ModelVariant::Fp16,
            threads: 4,
            reasons: vec![],
            warnings: vec![],
        }
    }

    fn installed_paths(tag: &str, backends: &[&str]) -> (AppPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!("wd-bench-{tag}-{}", std::process::id()));
        let paths = crate::paths::resolve(&root);
        paths.ensure().unwrap();
        for backend in backends {
            let dir = paths.backend_dir(backend);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(backend_binary(&dir), b"fake").unwrap();
        }
        let model = paths.model_path("ggml-small.bin");
        std::fs::write(&model, b"weights").unwrap();
        (paths, root)
    }

    #[test]
    fn writes_and_reads_wav_duration() {
        let dir = std::env::temp_dir().join(format!("wd-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.wav");
        let written = write_silence_wav(&path, 2.5).unwrap();
        assert!((written - 2.5).abs() < 0.01);
        let parsed = wav_duration_seconds(&path).unwrap();
        assert!((parsed - 2.5).abs() < 0.01);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 44 + 80_000);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn command_contains_model_audio_and_threads() {
        let command = build_command(
            PathBuf::from("/apps/backends/vulkan/whisper-cli"),
            Path::new("/models/ggml-small.bin"),
            Path::new("/data/bench.wav"),
            6,
            "auto",
        );
        assert_eq!(command.args[0], "-m");
        assert_eq!(command.args[1], "/models/ggml-small.bin");
        assert!(command.args.contains(&"-t".to_string()));
        assert!(command.args.contains(&"6".to_string()));
        assert_eq!(command.args.last().unwrap(), "auto");
    }

    #[test]
    fn detects_silent_cpu_fallback() {
        let (paths, root) = installed_paths("fallback", &["vulkan", "cpu"]);
        let runner = FakeRunner::new("whisper_backend_init: using CPU backend", 20.0);

        let report = verify_backend(
            &paths,
            BackendId::Vulkan,
            &paths.model_path("x.bin"),
            4,
            &runner,
        )
        .expect("debe ejecutar");
        assert_eq!(report.backend_seen, Some(BackendId::Cpu));
        assert!(!report.matches_plan);
        assert!((report.rtf - 0.5).abs() < 1e-9);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn walks_fallbacks_until_one_works() {
        let (paths, root) = installed_paths("walk", &["vulkan", "cpu"]);
        let plan = plan(BackendId::Vulkan, vec![BackendId::Cpu]);
        let model = paths.model_path("ggml-small.bin");

        // First call reports CPU (the Vulkan build silently fell back),
        // second call is the CPU backend actually running.
        struct Sequence {
            logs: RefCell<Vec<&'static str>>,
        }
        impl CliRunner for Sequence {
            fn run(&self, _: &BenchCommand) -> Result<RunOutput, BenchError> {
                let log = self.logs.borrow_mut().pop().unwrap_or("using CPU backend");
                Ok(RunOutput {
                    stdout: String::new(),
                    stderr: log.to_string(),
                    success: true,
                    elapsed_seconds: 10.0,
                })
            }
        }
        let runner = Sequence {
            logs: RefCell::new(vec!["using CPU backend", "using CPU backend"]),
        };

        let outcome = verify_install(&paths, &plan, &model, 4, &runner);
        assert_eq!(outcome.working, Some(BackendId::Cpu));
        assert_eq!(outcome.tried, vec![BackendId::Vulkan, BackendId::Cpu]);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reports_success_when_expected_backend_runs() {
        let (paths, root) = installed_paths("ok", &["cuda"]);
        let plan = plan(BackendId::Cuda, vec![BackendId::Vulkan]);
        let model = paths.model_path("ggml-small.bin");
        let runner = FakeRunner::new("whisper_backend_init_gpu: using CUDA0 backend", 1.0);

        let outcome = verify_install(&paths, &plan, &model, 4, &runner);
        assert_eq!(outcome.working, Some(BackendId::Cuda));
        assert_eq!(outcome.tried, vec![BackendId::Cuda]);
        let report = outcome.report.unwrap();
        assert!(report.matches_plan);
        assert!(report.rtf > 5.0);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn skips_backends_that_are_not_installed() {
        let (paths, root) = installed_paths("missing", &["cpu"]);
        let plan = plan(BackendId::Vulkan, vec![BackendId::Cpu]);
        let model = paths.model_path("ggml-small.bin");
        let runner = FakeRunner::new("using CPU backend", 10.0);

        let outcome = verify_install(&paths, &plan, &model, 4, &runner);
        assert_eq!(outcome.tried, vec![BackendId::Vulkan, BackendId::Cpu]);
        assert_eq!(outcome.working, Some(BackendId::Cpu));

        std::fs::remove_dir_all(&root).ok();
    }
}
