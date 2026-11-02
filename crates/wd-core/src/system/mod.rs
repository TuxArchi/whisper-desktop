//! Integration with the operating system: shortcuts, PATH, "Open with" and
//! their removal.
//!
//! The rules live in `scripts`, which is platform independent and therefore
//! testable everywhere. The dispatcher below picks the implementation.

pub mod scripts;

#[cfg(unix)]
mod linux;

#[cfg(windows)]
mod windows;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct IntegrationOptions {
    pub app_name: String,
    pub exec_path: PathBuf,
    /// Source icon used when the platform needs one copied into place.
    pub icon_path: Option<PathBuf>,
    pub create_start_menu: bool,
    pub create_desktop_shortcut: bool,
    pub add_to_path: bool,
    pub register_open_with: bool,
    /// Overrides `$HOME`. Only used by tests and by the `.run` installer,
    /// which may install for another prefix.
    pub home: Option<PathBuf>,
}

impl IntegrationOptions {
    pub fn new(app_name: impl Into<String>, exec_path: PathBuf) -> Self {
        Self {
            app_name: app_name.into(),
            exec_path,
            icon_path: None,
            create_start_menu: true,
            create_desktop_shortcut: false,
            add_to_path: true,
            register_open_with: true,
            home: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationReport {
    pub actions: Vec<String>,
    pub errors: Vec<String>,
}

impl IntegrationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn done(&mut self, message: impl Into<String>) {
        self.actions.push(message.into());
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }
}

/// Creates shortcuts and registry entries. Never aborts on a single failure:
/// a missing shortcut must not break an otherwise good installation.
pub fn integrate(options: &IntegrationOptions) -> IntegrationReport {
    #[cfg(unix)]
    {
        linux::integrate(options)
    }
    #[cfg(windows)]
    {
        windows::integrate(options)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = options;
        IntegrationReport::default()
    }
}

/// Removes everything `integrate` created. Safe to run twice.
pub fn unintegrate(options: &IntegrationOptions) -> IntegrationReport {
    #[cfg(unix)]
    {
        linux::unintegrate(options)
    }
    #[cfg(windows)]
    {
        windows::unintegrate(options)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = options;
        IntegrationReport::default()
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
