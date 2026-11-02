//! Job execution: audio normalisation and streaming whisper.cpp runs.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::job::{parse_progress, JobSpec, WhisperCommand};
use crate::output::{Format, OutputPlan};
use crate::process::hidden;
use crate::verify::parse_backend;

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("no se pudo ejecutar {program}: {detail}")]
    Spawn { program: String, detail: String },
    #[error("el motor terminó con código {0}")]
    Failed(i32),
    #[error("cancelado por el usuario")]
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct RunOutcome {
    pub ok: bool,
    pub log: String,
}

/// Injectable so the whole job flow can be tested without whisper.cpp.
pub trait ProcessRunner {
    fn run(
        &self,
        command: &WhisperCommand,
        on_progress: &mut dyn FnMut(u8),
        cancel: &AtomicBool,
    ) -> Result<RunOutcome, TranscribeError>;
}

pub struct SystemRunner;

impl ProcessRunner for SystemRunner {
    fn run(
        &self,
        command: &WhisperCommand,
        on_progress: &mut dyn FnMut(u8),
        cancel: &AtomicBool,
    ) -> Result<RunOutcome, TranscribeError> {
        let mut process = Command::new(&command.program);
        process
            .args(&command.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = hidden(&mut process)
            .spawn()
            .map_err(|error| TranscribeError::Spawn {
                program: command.program.to_string_lossy().to_string(),
                detail: error.to_string(),
            })?;

        let mut stderr = child.stderr.take().ok_or_else(|| TranscribeError::Spawn {
            program: command.program.to_string_lossy().to_string(),
            detail: "sin stderr".to_string(),
        })?;

        let mut log = String::new();
        let mut pending = String::new();
        let mut buffer = [0u8; 8192];

        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TranscribeError::Cancelled);
            }

            let read = stderr.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            pending.push_str(&String::from_utf8_lossy(&buffer[..read]));

            // whisper.cpp separates progress with `\r`, text with `\n`.
            while let Some(position) = pending.find(['\n', '\r']) {
                let chunk: String = pending.drain(..=position).collect();
                let line = chunk.trim_end_matches(['\n', '\r']);
                handle_line(line, &mut log, on_progress);
            }
        }
        if !pending.is_empty() {
            handle_line(
                pending.trim_end_matches(['\n', '\r']),
                &mut log,
                on_progress,
            );
        }

        let status = child.wait()?;
        Ok(RunOutcome {
            ok: status.success(),
            log,
        })
    }
}

fn handle_line(line: &str, log: &mut String, on_progress: &mut dyn FnMut(u8)) {
    if line.trim().is_empty() {
        return;
    }
    log.push_str(line);
    log.push('\n');
    if let Some(percent) = parse_progress(line) {
        on_progress(percent.min(100));
    }
}

/// Converts any input to the 16 kHz mono PCM that whisper.cpp expects.
pub fn normalize_audio(ffmpeg: &Path, source: &Path, dest: &Path) -> Result<(), TranscribeError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut process = Command::new(ffmpeg);
    let status = hidden(
        process
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(source)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(dest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .status()
    .map_err(|error| TranscribeError::Spawn {
        program: ffmpeg.to_string_lossy().to_string(),
        detail: error.to_string(),
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(TranscribeError::Failed(status.code().unwrap_or(-1)))
    }
}

/// Files the job is expected to produce, including the `_en` variants of
/// `Both`.
pub fn expected_files(spec: &JobSpec, plan: &OutputPlan) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = plan.files.clone();
    if spec.task == crate::job::Task::Both {
        for format in &spec.formats {
            files.push(
                plan.dir
                    .join(format!("{}_en.{}", plan.stem, extension_of(*format))),
            );
        }
    }
    files
}

fn extension_of(format: Format) -> &'static str {
    format.extension()
}

#[derive(Debug, Clone, Default)]
pub struct JobOutcome {
    pub ok: bool,
    pub outputs: Vec<PathBuf>,
    pub backend: Option<crate::decide::BackendId>,
    pub log: String,
}

/// Runs the whole job: normalise, then one command per task run.
/// `ffmpeg` is optional so the flow can be tested without an external binary;
/// production always passes it.
pub fn run_job(
    spec: &JobSpec,
    plan: &OutputPlan,
    ffmpeg: Option<&Path>,
    runner: &dyn ProcessRunner,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(u8),
) -> Result<JobOutcome, TranscribeError> {
    // whisper does not create directories: a missing output folder would
    // silently drop every result.
    std::fs::create_dir_all(&plan.dir)?;
    if let Some(ffmpeg) = ffmpeg {
        normalize_audio(ffmpeg, &spec.source, &spec.audio)?;
    }

    let commands = crate::job::build_commands(spec, plan);
    let total = commands.len().max(1);
    let mut outcome = JobOutcome::default();

    for (index, command) in commands.iter().enumerate() {
        let base = index as u32 * 100 / total as u32;
        let span = 100 / total as u32;
        let mut report = |percent: u8| {
            let overall = base + (u32::from(percent) * span / 100).min(span);
            on_progress(overall.min(100) as u8);
        };

        let result = runner.run(command, &mut report, cancel)?;
        outcome.log.push_str(&result.log);
        if let Some(backend) = parse_backend(&result.log) {
            outcome.backend = Some(backend);
        }
        if !result.ok {
            outcome.ok = false;
            return Ok(outcome);
        }
    }

    outcome.ok = true;
    outcome.outputs = expected_files(spec, plan)
        .into_iter()
        .filter(|path| path.exists())
        .collect();
    on_progress(100);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OnCollision;
    use crate::job::Task;
    use std::sync::Arc;

    struct FakeRunner {
        backend_line: &'static str,
        write_translation: bool,
    }

    impl FakeRunner {
        fn write_outputs(&self, command: &WhisperCommand) {
            let stem = command.output_stem.to_string_lossy().to_string();
            for flag in ["-otxt", "-osrt"] {
                if command.args.iter().any(|arg| arg == flag) {
                    let extension = flag.trim_start_matches("-o");
                    std::fs::write(format!("{stem}.{extension}"), b"texto").unwrap();
                }
            }
            if self.write_translation && command.args.iter().any(|arg| arg == "-tr") {
                // The translation command already writes to its own stem.
            }
        }
    }

    impl ProcessRunner for FakeRunner {
        fn run(
            &self,
            command: &WhisperCommand,
            on_progress: &mut dyn FnMut(u8),
            cancel: &AtomicBool,
        ) -> Result<RunOutcome, TranscribeError> {
            if cancel.load(Ordering::Relaxed) {
                return Err(TranscribeError::Cancelled);
            }
            on_progress(50);
            self.write_outputs(command);
            Ok(RunOutcome {
                ok: true,
                log: format!("whisper_backend_init: {}\n", self.backend_line),
            })
        }
    }

    /// Each test gets its own directory: they run in parallel and sharing one
    /// made the collision-renaming logic produce different file names.
    fn spec(tag: &str, task: Task, formats: Vec<Format>) -> JobSpec {
        let root = std::env::temp_dir().join(format!("wd-run-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        JobSpec {
            source: root.join("reunion.mp3"),
            audio: root.join("work/reunion.wav"),
            task,
            language: "es".into(),
            model: "small".into(),
            model_path: root.join("models/ggml-small.bin"),
            backend_dir: root.join("backends/vulkan"),
            output_dir: root.join("out"),
            relative_dir: PathBuf::new(),
            formats,
            pattern: "{nombre}".into(),
            on_collision: OnCollision::Rename,
            threads: 4,
        }
    }

    #[test]
    fn collects_expected_outputs() {
        let job = spec("collects", Task::Transcribe, vec![Format::Txt, Format::Srt]);
        std::fs::create_dir_all(&job.output_dir).unwrap();
        let plan = job.output_plan("2026-11-01", "1930");

        let runner = FakeRunner {
            backend_line: "using Vulkan0 backend",
            write_translation: false,
        };
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();

        let outcome = run_job(&job, &plan, None, &runner, &cancel, &mut |percent| {
            progress.push(percent)
        })
        .unwrap();

        assert!(outcome.ok);
        assert_eq!(outcome.outputs.len(), 2);
        assert!(outcome.outputs[0].ends_with("reunion.txt"));
        assert!(outcome.outputs[1].ends_with("reunion.srt"));
        assert_eq!(progress.last(), Some(&100));
        assert_eq!(outcome.backend, Some(crate::decide::BackendId::Vulkan));

        std::fs::remove_dir_all(job.backend_dir.parent().unwrap()).ok();
    }

    #[test]
    fn both_produces_four_files() {
        let job = spec("both", Task::Both, vec![Format::Txt, Format::Srt]);
        std::fs::create_dir_all(&job.output_dir).unwrap();
        let plan = job.output_plan("2026-11-01", "1930");

        let runner = FakeRunner {
            backend_line: "using CPU backend",
            write_translation: false,
        };
        let cancel = AtomicBool::new(false);
        let outcome = run_job(&job, &plan, None, &runner, &cancel, &mut |_| {}).unwrap();

        assert_eq!(outcome.outputs.len(), 4);
        assert!(outcome
            .outputs
            .iter()
            .any(|p| p.ends_with("reunion_en.txt")));

        std::fs::remove_dir_all(job.backend_dir.parent().unwrap()).ok();
    }

    #[test]
    fn cancellation_propagates() {
        let job = spec("cancel", Task::Transcribe, vec![Format::Txt]);
        let plan = job.output_plan("2026-11-01", "1930");
        let runner = FakeRunner {
            backend_line: "using CPU backend",
            write_translation: false,
        };
        let cancel = Arc::new(AtomicBool::new(true));

        let error = run_job(&job, &plan, None, &runner, &cancel, &mut |_| {}).unwrap_err();
        assert!(matches!(error, TranscribeError::Cancelled));

        std::fs::remove_dir_all(job.backend_dir.parent().unwrap()).ok();
    }
}
