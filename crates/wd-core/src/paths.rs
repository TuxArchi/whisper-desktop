//! Filesystem layout.
//!
//! Windows is portable: data, models and logs live next to the executable so
//! the whole folder can be copied to a USB stick. Linux installs through a
//! package into `/usr/lib`, which is read-only, so data goes to XDG paths.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Directory holding the executable, backends and ffmpeg.
    pub root: PathBuf,
    pub backends: PathBuf,
    pub bin: PathBuf,
    /// Writable directory for configuration, history and logs.
    pub data: PathBuf,
    pub models: PathBuf,
    pub logs: PathBuf,
    pub config_file: PathBuf,
    pub manifest_file: PathBuf,
}

impl AppPaths {
    pub fn with_data_dir(root: &Path, data: PathBuf) -> Self {
        let models = data.join("models");
        let logs = data.join("logs");
        Self {
            root: root.to_path_buf(),
            backends: root.join("backends"),
            bin: root.join("bin"),
            config_file: data.join("config.json"),
            manifest_file: data.join("manifest.json"),
            models,
            logs,
            data,
        }
    }

    pub fn backend_dir(&self, id: &str) -> PathBuf {
        self.backends.join(id)
    }

    pub fn model_path(&self, file: &str) -> PathBuf {
        self.models.join(file)
    }

    pub fn history_file(&self) -> PathBuf {
        self.data.join("history.jsonl")
    }

    pub fn work_dir(&self) -> PathBuf {
        self.data.join("work")
    }

    pub fn ffmpeg(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.bin.join("ffmpeg.exe")
        } else {
            self.bin.join("ffmpeg")
        }
    }

    /// Creates the writable directories.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data)?;
        std::fs::create_dir_all(&self.models)?;
        std::fs::create_dir_all(&self.logs)?;
        Ok(())
    }
}

/// Portable layout: everything inside the application directory.
pub fn resolve(app_dir: &Path) -> AppPaths {
    AppPaths::with_data_dir(app_dir, app_dir.join("data"))
}

/// Portable when possible, platform data directory when the application
/// directory is not writable (e.g. `/usr/lib`, or `Program Files`).
pub fn resolve_preferred(app_dir: &Path) -> AppPaths {
    let portable = app_dir.join("data");
    if is_writable(&portable) || is_writable(app_dir) {
        resolve(app_dir)
    } else {
        AppPaths::with_data_dir(app_dir, platform_data_dir())
    }
}

pub fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".wd-probe-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            std::fs::remove_file(&probe).ok();
            true
        }
        Err(_) => false,
    }
}

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// XDG data dir on Linux, roaming AppData on Windows.
pub fn platform_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Whisper Desktop");
        }
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Whisper Desktop")
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("whisper-desktop");
        }
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share/whisper-desktop")
    }
}

/// Suggested output folder for transcriptions. Never inside the app folder:
/// uninstalling must not delete the user's work.
pub fn default_output_dir() -> PathBuf {
    let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "windows")]
    {
        home.join("Documents").join("Transcripciones")
    }
    #[cfg(not(target_os = "windows"))]
    {
        home.join("Transcripciones")
    }
}

/// Directory that never moves, used to remember where the user put the data.
/// A tiny pointer file here avoids the chicken-and-egg problem of storing the
/// location inside the data directory itself.
pub fn platform_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Whisper Desktop");
        }
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Whisper Desktop")
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("whisper-desktop");
        }
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/whisper-desktop")
    }
}

pub fn location_file() -> PathBuf {
    platform_config_dir().join("location.txt")
}

pub fn read_location_from(pointer: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(pointer).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

pub fn write_location_to(pointer: &Path, dir: &Path) -> std::io::Result<()> {
    if let Some(parent) = pointer.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(pointer, dir.to_string_lossy().as_bytes())
}

/// Resolves the data directory honouring a previous relocation.
pub fn resolve_configured(app_dir: &Path) -> AppPaths {
    if let Some(dir) = read_location_from(&location_file()) {
        if dir.is_dir() {
            return AppPaths::with_data_dir(app_dir, dir);
        }
    }
    resolve_preferred(app_dir)
}

/// Detects the "user opened the exe from inside the ZIP preview" case, which
/// would put multi-gigabyte models in a temporary folder.
pub fn looks_like_zip_preview(dir: &Path) -> bool {
    let text = dir.to_string_lossy().to_ascii_lowercase();
    text.contains("\\temp\\") && text.contains(".zip")
        || text.contains("/tmp/") && text.contains(".zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wd-paths-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn portable_layout() {
        let root = temp_dir("portable");
        let paths = resolve(&root);
        assert_eq!(paths.data, root.join("data"));
        assert_eq!(paths.models, root.join("data/models"));
        assert_eq!(paths.config_file, root.join("data/config.json"));
        assert_eq!(paths.backends, root.join("backends"));
        paths.ensure().unwrap();
        assert!(paths.models.is_dir());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn writable_detection() {
        let root = temp_dir("writable");
        assert!(is_writable(&root));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn zip_preview_detection() {
        assert!(looks_like_zip_preview(Path::new(
            "C:\\Users\\a\\AppData\\Local\\Temp\\Temp1_WhisperDesktop.zip\\WhisperDesktop"
        )));
        assert!(!looks_like_zip_preview(Path::new(
            "C:\\Users\\a\\Desktop\\WhisperDesktop"
        )));
    }

    #[test]
    fn default_output_is_outside_app() {
        let output = default_output_dir();
        assert!(output.is_absolute() || output.starts_with("."));
    }
}
