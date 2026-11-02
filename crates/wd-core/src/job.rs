//! Job definitions: how one source file becomes one or two whisper.cpp runs.

use std::path::{Path, PathBuf};

use crate::config::OnCollision;
use crate::output::{Format, NameContext, OutputPlan};

/// Extensions the Studio accepts. ffmpeg is always installed, so video and
/// m4a are first-class inputs.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mkv", "mov", "webm", "avi",
    "m4v", "ts",
];

pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let lower = extension.to_ascii_lowercase();
            MEDIA_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub path: PathBuf,
    /// Directory relative to the scanned root. Empty for top-level files.
    pub relative_dir: PathBuf,
}

/// Recursively collects media files. Hidden directories and symlinks are
/// skipped so a stray link cannot make the scan loop.
pub fn scan_media(root: &Path) -> Vec<MediaFile> {
    let mut out = Vec::new();
    walk(root, root, &mut out, 0);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<MediaFile>, depth: usize) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            let hidden = path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with('.'))
                .unwrap_or(false);
            if hidden {
                continue;
            }
            walk(root, &path, out, depth + 1);
        } else if is_media(&path) {
            let relative_dir = path
                .parent()
                .and_then(|parent| parent.strip_prefix(root).ok())
                .map(|parent| parent.to_path_buf())
                .unwrap_or_default();
            out.push(MediaFile { path, relative_dir });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Task {
    #[default]
    #[serde(rename = "transcribe")]
    Transcribe,
    #[serde(rename = "translateEn")]
    TranslateEn,
    #[serde(rename = "both")]
    Both,
}

impl Task {
    /// Token used in the `{tarea}` placeholder.
    pub fn token(self) -> &'static str {
        match self {
            Task::Transcribe => "transcripcion",
            Task::TranslateEn => "traduccion",
            Task::Both => "transcripcion",
        }
    }

    pub fn is_translate(self) -> bool {
        matches!(self, Task::TranslateEn | Task::Both)
    }

    pub fn runs(self) -> u32 {
        if self == Task::Both {
            2
        } else {
            1
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhisperCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Base path passed to `-of`; whisper appends the format extension.
    pub output_stem: PathBuf,
    /// Language tag for history and naming.
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct JobSpec {
    pub source: PathBuf,
    pub audio: PathBuf,
    pub task: Task,
    pub language: String,
    pub model: String,
    pub model_path: PathBuf,
    pub backend_dir: PathBuf,
    pub output_dir: PathBuf,
    /// Subdirectory mirrored from the source tree; empty means "output root".
    pub relative_dir: PathBuf,
    pub formats: Vec<Format>,
    pub pattern: String,
    pub on_collision: OnCollision,
    pub threads: u32,
}

impl JobSpec {
    pub fn name_context(&self, date: &str, time: &str) -> NameContext {
        NameContext {
            name: stem_of(&self.source),
            model: self.model.clone(),
            language: self.language.clone(),
            task: self.task.token().to_string(),
            date: date.to_string(),
            time: time.to_string(),
        }
    }

    /// Output root plus the mirrored subdirectory, if any.
    pub fn effective_output_dir(&self) -> PathBuf {
        if self.relative_dir.as_os_str().is_empty() {
            self.output_dir.clone()
        } else {
            self.output_dir.join(&self.relative_dir)
        }
    }

    pub fn output_plan(&self, date: &str, time: &str) -> OutputPlan {
        crate::output::plan_outputs(
            &self.effective_output_dir(),
            &self.formats,
            &self.pattern,
            &self.name_context(date, time),
            self.on_collision,
        )
    }

    pub fn binary(&self) -> PathBuf {
        crate::bench::backend_binary(&self.backend_dir)
    }
}

/// Builds the whisper.cpp invocations for this job.
///
/// `Both` produces two runs: the transcript and the English translation. They
/// share the audio decode but not the inference, so it costs twice the time;
/// the UI says so before starting.
pub fn build_commands(spec: &JobSpec, plan: &OutputPlan) -> Vec<WhisperCommand> {
    let program = spec.binary();

    let mut base = vec![
        "-m".to_string(),
        spec.model_path.to_string_lossy().to_string(),
        "-f".to_string(),
        spec.audio.to_string_lossy().to_string(),
        "-t".to_string(),
        spec.threads.max(1).to_string(),
        "-l".to_string(),
        spec.language.clone(),
        "--print-progress".to_string(),
    ];
    for format in &spec.formats {
        base.push(match format {
            Format::Txt => "-otxt".to_string(),
            Format::Srt => "-osrt".to_string(),
            Format::Vtt => "-ovtt".to_string(),
            Format::Json => "-oj".to_string(),
        });
    }

    // `-of` is what tells whisper where to write. Without it the files land
    // next to the input audio with the audio's own name.
    let args_for = |stem: &Path| -> Vec<String> {
        let mut args = base.clone();
        args.push("-of".to_string());
        args.push(stem.to_string_lossy().to_string());
        args
    };

    let main_stem = plan.dir.join(&plan.stem);

    match spec.task {
        Task::Transcribe => vec![WhisperCommand {
            program,
            args: args_for(&main_stem),
            output_stem: main_stem,
            language: spec.language.clone(),
        }],
        Task::TranslateEn => {
            let mut args = args_for(&main_stem);
            args.push("-tr".to_string());
            vec![WhisperCommand {
                program,
                args,
                output_stem: main_stem,
                language: "en".to_string(),
            }]
        }
        Task::Both => {
            let translated_stem = plan.dir.join(format!("{}_en", plan.stem));
            let mut translated_args = args_for(&translated_stem);
            translated_args.push("-tr".to_string());
            vec![
                WhisperCommand {
                    program: program.clone(),
                    args: args_for(&main_stem),
                    output_stem: main_stem,
                    language: spec.language.clone(),
                },
                WhisperCommand {
                    program,
                    args: translated_args,
                    output_stem: translated_stem,
                    language: "en".to_string(),
                },
            ]
        }
    }
}

/// whisper.cpp prints `whisper_print_progress_callback: progress =  42%`.
pub fn parse_progress(line: &str) -> Option<u8> {
    let marker = "progress =";
    let index = line.find(marker)?;
    let rest = &line[index + marker.len()..];
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse::<u8>().ok()
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "audio".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OnCollision;

    fn spec(task: Task, formats: Vec<Format>) -> JobSpec {
        JobSpec {
            source: PathBuf::from("/audio/reunion.mp3"),
            audio: PathBuf::from("/tmp/reunion.wav"),
            task,
            language: "auto".into(),
            model: "small".into(),
            model_path: PathBuf::from("/models/ggml-small.bin"),
            backend_dir: PathBuf::from("/app/backends/vulkan"),
            output_dir: PathBuf::from("/out"),
            relative_dir: PathBuf::new(),
            formats,
            pattern: "{nombre}".into(),
            on_collision: OnCollision::Rename,
            threads: 8,
        }
    }

    #[test]
    fn transcribe_produces_one_command() {
        let job = spec(Task::Transcribe, vec![Format::Txt, Format::Srt]);
        let commands = build_commands(&job, &job.output_plan("2026-11-01", "1930"));
        assert_eq!(commands.len(), 1);

        let command = &commands[0];
        assert!(command.args.contains(&"-otxt".to_string()));
        assert!(command.args.contains(&"-osrt".to_string()));
        assert!(!command.args.contains(&"-tr".to_string()));
        assert_eq!(command.output_stem, PathBuf::from("/out/reunion"));
        // Without -of, whisper writes next to the input audio.
        let position = command
            .args
            .iter()
            .position(|arg| arg == "-of")
            .expect("-of");
        assert_eq!(
            command.args[position + 1],
            plan.dir.join(&plan.stem).to_string_lossy()
        );
        assert_eq!(command.language, "auto");
    }

    #[test]
    fn translate_uses_native_flag() {
        let job = spec(Task::TranslateEn, vec![Format::Txt]);
        let commands = build_commands(&job, &job.output_plan("2026-11-01", "1930"));
        assert_eq!(commands.len(), 1);
        assert!(commands[0].args.contains(&"-tr".to_string()));
        assert_eq!(commands[0].language, "en");
    }

    #[test]
    fn both_runs_twice_with_distinct_stems() {
        let job = spec(Task::Both, vec![Format::Txt]);
        let commands = build_commands(&job, &job.output_plan("2026-11-01", "1930"));
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].output_stem, PathBuf::from("/out/reunion"));
        assert_eq!(commands[1].output_stem, PathBuf::from("/out/reunion_en"));
        assert!(commands[1].args.contains(&"-tr".to_string()));
    }

    #[test]
    fn progress_is_parsed() {
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress =  42%"),
            Some(42)
        );
        assert_eq!(parse_progress("progress = 100%"), Some(100));
        assert_eq!(parse_progress("progress = 0%"), Some(0));
        assert_eq!(parse_progress("sin progreso"), None);
    }

    #[test]
    fn name_context_uses_file_stem() {
        let job = spec(Task::Transcribe, vec![Format::Txt]);
        let context = job.name_context("2026-11-01", "1930");
        assert_eq!(context.name, "reunion");
        assert_eq!(context.task, "transcripcion");
    }
    #[test]
    fn detects_media_extensions() {
        assert!(is_media(Path::new("a.MP3")));
        assert!(is_media(Path::new("video.mkv")));
        assert!(is_media(Path::new("grabacion.m4a")));
        assert!(!is_media(Path::new("notas.txt")));
        assert!(!is_media(Path::new("sin_extension")));
    }

    #[test]
    fn scans_folders_recursively_with_relative_dirs() {
        let root = std::env::temp_dir().join(format!("wd-scan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub/deep")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("a.mp3"), b"x").unwrap();
        std::fs::write(root.join("sub/b.wav"), b"x").unwrap();
        std::fs::write(root.join("sub/deep/c.mp4"), b"x").unwrap();
        std::fs::write(root.join("notas.txt"), b"x").unwrap();
        std::fs::write(root.join(".hidden/d.mp3"), b"x").unwrap();

        let files = scan_media(&root);
        assert_eq!(files.len(), 3, "solo audio/vídeo y sin carpetas ocultas");

        let b = files.iter().find(|f| f.path.ends_with("b.wav")).unwrap();
        assert_eq!(b.relative_dir, PathBuf::from("sub"));
        let c = files.iter().find(|f| f.path.ends_with("c.mp4")).unwrap();
        assert_eq!(c.relative_dir, PathBuf::from("sub/deep"));
        let a = files.iter().find(|f| f.path.ends_with("a.mp3")).unwrap();
        assert!(a.relative_dir.as_os_str().is_empty());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn output_mirrors_relative_dir() {
        let mut job = spec(Task::Transcribe, vec![Format::Txt]);
        job.relative_dir = PathBuf::from("sub/deep");
        assert_eq!(job.effective_output_dir(), PathBuf::from("/out/sub/deep"));

        let plan = job.output_plan("2026-11-01", "1930");
        assert_eq!(plan.dir, PathBuf::from("/out/sub/deep"));
        assert_eq!(plan.files[0], PathBuf::from("/out/sub/deep/reunion.txt"));
    }
}
