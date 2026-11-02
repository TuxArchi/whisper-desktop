//! Studio commands: queue execution, history and filesystem helpers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use wd_core::config::{Config, Task};
use wd_core::history::{self, HistoryEntry};
use wd_core::job::JobSpec;
use wd_core::manifest::Manifest;
use wd_core::output::{self, Format};
use wd_core::transcriber::{self, JobOutcome, SystemRunner};

static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioState {
    pub models: Vec<String>,
    pub default_model: String,
    pub output_dir: String,
    pub formats: Vec<String>,
    pub pattern: String,
    pub task: String,
    pub language: String,
    pub threads: u32,
    pub backend: String,
    pub can_run: bool,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEntry {
    pub path: String,
    /// Subdirectory relative to the scanned root; empty for picked files.
    #[serde(default)]
    pub relative: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRequest {
    pub entries: Vec<JobEntry>,
    pub task: Task,
    pub language: String,
    pub model: String,
    pub formats: Vec<String>,
    pub pattern: String,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum JobEvent {
    Started {
        index: usize,
        total: usize,
        source: String,
    },
    Progress {
        index: usize,
        percent: u8,
    },
    Finished {
        index: usize,
        ok: bool,
        outputs: Vec<String>,
        error: Option<String>,
    },
    Done {
        ok: usize,
        failed: usize,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub ok: usize,
    pub failed: usize,
    pub outputs: Vec<String>,
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolved() -> wd_core::paths::AppPaths {
    wd_core::paths::resolve_configured(&app_dir())
}

#[tauri::command]
pub fn studio_state() -> StudioState {
    let paths = resolved();
    let config = Config::load_or_default(&paths.config_file);
    let manifest = Manifest::load(&paths.manifest_file).ok();
    let report = wd_core::detect::detect();

    let models = manifest
        .as_ref()
        .map(|manifest| manifest.models.clone())
        .unwrap_or_default();

    let backend = manifest
        .as_ref()
        .map(|manifest| manifest.backend.as_str().to_string())
        .unwrap_or_default();

    let mut problem = None;
    if manifest.is_none() {
        problem = Some("la instalación no está completa".to_string());
    } else if models.is_empty() {
        problem = Some("no hay ningún modelo descargado".to_string());
    } else if !paths.ffmpeg().exists() {
        problem = Some("falta ffmpeg; reinstala para poder leer vídeo y m4a".to_string());
    }

    StudioState {
        default_model: models
            .first()
            .cloned()
            .unwrap_or_else(|| "ggml-base.bin".to_string()),
        models,
        output_dir: if config.defaults.output_dir.is_empty() {
            wd_core::paths::default_output_dir()
                .to_string_lossy()
                .to_string()
        } else {
            config.defaults.output_dir.clone()
        },
        formats: if config.defaults.formats.is_empty() {
            vec!["txt".to_string(), "srt".to_string()]
        } else {
            config.defaults.formats.clone()
        },
        pattern: config.defaults.name_pattern.clone(),
        task: match config.defaults.task {
            Task::Transcribe => "transcribe",
            Task::TranslateEn => "translateEn",
            Task::Both => "both",
        }
        .to_string(),
        language: config.defaults.language.clone(),
        threads: report.cpu.thread_count(),
        backend,
        can_run: problem.is_none(),
        problem,
    }
}

/// Preview of the files a job would produce. Lets the UI show the real name
/// (with tokens resolved and collisions handled) before starting.
#[tauri::command]
pub fn preview_output(
    source: String,
    model: String,
    language: String,
    pattern: String,
    output_dir: String,
    formats: Vec<String>,
) -> Vec<String> {
    let config = Config::load_or_default(&resolved().config_file);
    let spec = build_spec(
        &source,
        "",
        &model,
        &language,
        &pattern,
        &output_dir,
        &formats,
        &config,
        Task::Transcribe,
    );
    let now = now_seconds();
    let (date, time) = output::timestamp_parts(now);
    spec.map(|spec| {
        spec.output_plan(&date, &time)
            .files
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    })
    .unwrap_or_default()
}

#[tauri::command]
pub fn list_history(limit: usize) -> Vec<HistoryEntry> {
    history::recent(&resolved().history_file(), limit)
}

#[tauri::command]
pub fn cancel_job() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return Err("abrir carpetas no está soportado en esta plataforma".to_string());

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        command
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("no se pudo abrir {path}: {error}"))
    }
}

#[tauri::command]
pub async fn start_job(app: tauri::AppHandle, request: JobRequest) -> Result<JobSummary, String> {
    CANCEL.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || run_request(&app, request))
        .await
        .map_err(|error| error.to_string())?
}

fn run_request(app: &tauri::AppHandle, request: JobRequest) -> Result<JobSummary, String> {
    let paths = resolved();
    let config = Config::load_or_default(&paths.config_file);
    let total = request.entries.len();

    let mut summary = JobSummary {
        ok: 0,
        failed: 0,
        outputs: Vec::new(),
    };

    if total == 0 {
        return Ok(summary);
    }

    let work_dir = paths.data.join("work");
    let (date, time) = output::timestamp_parts(now_seconds());

    for (index, item) in request.entries.iter().enumerate() {
        if CANCEL.load(Ordering::SeqCst) {
            break;
        }

        let _ = app.emit(
            "job:event",
            &JobEvent::Started {
                index,
                total,
                source: item.path.clone(),
            },
        );

        let Some(spec) = build_spec(
            &item.path,
            &item.relative,
            &request.model,
            &request.language,
            &request.pattern,
            &request.output_dir,
            &request.formats,
            &config,
            request.task,
        ) else {
            let _ = app.emit(
                "job:event",
                &JobEvent::Finished {
                    index,
                    ok: false,
                    outputs: Vec::new(),
                    error: Some("no se pudo preparar el trabajo".to_string()),
                },
            );
            summary.failed += 1;
            continue;
        };

        let plan = spec.output_plan(&date, &time);
        let audio = work_dir.join(format!("job-{index}.wav"));
        let mut spec = spec;
        spec.audio = audio.clone();

        let started = std::time::Instant::now();
        let emitter = app.clone();
        let result = transcriber::run_job(
            &spec,
            &plan,
            Some(&paths.ffmpeg()),
            &SystemRunner,
            &CANCEL,
            &mut |percent| {
                let _ = emitter.emit("job:event", &JobEvent::Progress { index, percent });
            },
        );

        // Measured before deleting the normalised file: the history needs to
        // show how much audio was processed and how fast.
        let audio_seconds = wd_core::bench::wav_duration_seconds(&audio).unwrap_or(0.0);
        let elapsed = started.elapsed().as_secs_f64();
        let rtf = if elapsed > 0.0 { audio_seconds / elapsed } else { 0.0 };
        std::fs::remove_file(&audio).ok();

        let entry = match result {
            Ok(JobOutcome {
                ok,
                outputs,
                backend,
                ..
            }) => {
                if ok {
                    summary.ok += 1;
                } else {
                    summary.failed += 1;
                }
                for output_path in &outputs {
                    summary
                        .outputs
                        .push(output_path.to_string_lossy().to_string());
                }

                let mut entry = HistoryEntry::new(next_id(index), item.path.clone());
                entry.outputs = outputs
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                entry.task = task_label(request.task).to_string();
                entry.model = spec.model.clone();
                entry.backend = backend.map(|backend| backend.as_str().to_string());
                entry.finished_at = now_seconds();
                entry.audio_seconds = audio_seconds;
                entry.elapsed_seconds = elapsed;
                entry.rtf = rtf;
                entry.ok = ok;

                let _ = app.emit(
                    "job:event",
                    &JobEvent::Finished {
                        index,
                        ok,
                        outputs: entry.outputs.clone(),
                        error: None,
                    },
                );
                entry
            }
            Err(error) => {
                summary.failed += 1;
                let message = error.to_string();
                let _ = app.emit(
                    "job:event",
                    &JobEvent::Finished {
                        index,
                        ok: false,
                        outputs: Vec::new(),
                        error: Some(message.clone()),
                    },
                );

                let mut entry = HistoryEntry::new(next_id(index), item.path.clone());
                entry.task = task_label(request.task).to_string();
                entry.model = spec.model.clone();
                entry.finished_at = now_seconds();
                entry.error = Some(message);
                entry.audio_seconds = audio_seconds;
                entry.elapsed_seconds = elapsed;
                entry.rtf = rtf;
                entry.ok = false;
                entry
            }
        };

        let _ = history::append(&paths.history_file(), &entry);
    }

    let _ = app.emit(
        "job:event",
        &JobEvent::Done {
            ok: summary.ok,
            failed: summary.failed,
        },
    );

    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn build_spec(
    source: &str,
    relative: &str,
    model_file: &str,
    language: &str,
    pattern: &str,
    output_dir: &str,
    formats: &[String],
    config: &Config,
    task: Task,
) -> Option<JobSpec> {
    let paths = resolved();
    let source_path = PathBuf::from(source);
    let model_path = paths.model_path(model_file);
    if !model_path.exists() {
        return None;
    }

    let manifest = Manifest::load(&paths.manifest_file).ok()?;
    let backend_dir = paths.backend_dir(manifest.backend.as_str());

    let mut parsed: Vec<Format> = formats.iter().filter_map(|f| Format::parse(f)).collect();
    if parsed.is_empty() {
        parsed = vec![Format::Txt];
    }

    Some(JobSpec {
        source: source_path,
        audio: paths.data.join("work/job.wav"),
        task,
        language: language.to_string(),
        model: strip_model(model_file),
        model_path,
        backend_dir,
        output_dir: PathBuf::from(output_dir),
        relative_dir: PathBuf::from(relative),
        formats: parsed,
        pattern: pattern.to_string(),
        on_collision: config.defaults.on_collision,
        threads: wd_core::detect::detect().cpu.thread_count(),
    })
}

fn strip_model(file: &str) -> String {
    file.trim_start_matches("ggml-")
        .trim_end_matches(".bin")
        .to_string()
}

fn task_label(task: Task) -> &'static str {
    match task {
        Task::Transcribe => "transcribe",
        Task::TranslateEn => "translateEn",
        Task::Both => "both",
    }
}

fn next_id(index: usize) -> String {
    format!("{}-{index}", now_seconds())
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFileView {
    pub path: String,
    pub relative: String,
}

/// Recursively lists media files under a folder, keeping the relative path so
/// batches can mirror the source tree.
#[tauri::command]
pub fn scan_folder(path: String) -> Vec<MediaFileView> {
    wd_core::job::scan_media(&PathBuf::from(&path))
        .into_iter()
        .map(|file| MediaFileView {
            path: file.path.to_string_lossy().to_string(),
            relative: file.relative_dir.to_string_lossy().to_string(),
        })
        .collect()
}

/// Media files passed on the command line, i.e. "Open with Whisper Desktop".
pub fn media_from_args<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .filter(|arg| !arg.starts_with('-'))
        .filter(|arg| wd_core::job::is_media(std::path::Path::new(arg)))
        .collect()
}

#[tauri::command]
pub fn initial_files() -> Vec<String> {
    media_from_args(std::env::args().skip(1))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfoView {
    pub dir: String,
    pub files: usize,
    pub bytes: u64,
}

#[tauri::command]
pub fn data_info() -> DataInfoView {
    let info = wd_core::relocate::data_info(&resolved());
    DataInfoView {
        dir: info.dir.to_string_lossy().to_string(),
        files: info.files,
        bytes: info.bytes,
    }
}

#[tauri::command]
pub async fn relocate_data(
    app: tauri::AppHandle,
    target: String,
    keep_copy: bool,
) -> Result<DataInfoView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = resolved();
        let emitter = app.clone();
        let mut last_emit = std::time::Instant::now();

        wd_core::relocate::relocate(
            &paths,
            std::path::Path::new(&target),
            keep_copy,
            &wd_core::paths::location_file(),
            &mut |done, total| {
                // Throttle: one event every 200 ms is plenty for a progress bar.
                if last_emit.elapsed().as_millis() >= 200 {
                    last_emit = std::time::Instant::now();
                    let _ = emitter.emit("data:progress", (done, total));
                }
            },
        )
        .map_err(|error| error.to_string())?;

        let info = wd_core::relocate::data_info(&paths);
        Ok(DataInfoView {
            dir: info.dir.to_string_lossy().to_string(),
            files: info.files,
            bytes: info.bytes,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
