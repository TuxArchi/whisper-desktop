//! Linux integration: `.desktop` entry, icon, `~/.local/bin` symlink.
//!
//! Package installs get all of this from `/usr/share`, but the `.run`
//! installer and portable use still need it, and the user must be able to
//! remove it again.

use std::path::{Path, PathBuf};

use super::{scripts, IntegrationOptions, IntegrationReport};

/// Embedded so the `.run` installer can install the icon without carrying a
/// second file around.
const ICON: &[u8] = include_bytes!("../../assets/icon-256.png");

const DESKTOP_FILE: &str = "whisper-desktop.desktop";
const SYMLINK_NAME: &str = "whisper-desktop";

pub fn integrate(options: &IntegrationOptions) -> IntegrationReport {
    let mut report = IntegrationReport::default();
    let Some(home) = options.home.clone().or_else(super::home_dir) else {
        report.fail("no se pudo determinar el directorio personal");
        return report;
    };

    let applications = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/256x256/apps");
    let bin = home.join(".local/bin");

    if options.create_start_menu {
        let path = applications.join(DESKTOP_FILE);
        match write(&path, scripts::desktop_entry(options).as_bytes()) {
            Ok(()) => report.done(format!("Entrada de menú: {}", path.display())),
            Err(error) => report.fail(format!("no se pudo escribir {}: {error}", path.display())),
        }

        let icon = icons.join("whisper-desktop.png");
        let bytes = options
            .icon_path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_else(|| ICON.to_vec());
        match write(&icon, &bytes) {
            Ok(()) => report.done(format!("Icono: {}", icon.display())),
            Err(error) => report.fail(format!("no se pudo escribir {}: {error}", icon.display())),
        }
    }

    if options.add_to_path {
        let link = bin.join(SYMLINK_NAME);
        match symlink(&options.exec_path, &link) {
            Ok(()) => report.done(format!("Comando: {}", link.display())),
            Err(error) => report.fail(format!("no se pudo crear {}: {error}", link.display())),
        }
    }

    if options.create_desktop_shortcut {
        let desktop = home.join("Desktop");
        let path = desktop.join(DESKTOP_FILE);
        match write(&path, scripts::desktop_entry(options).as_bytes()) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
                }
                report.done(format!("Acceso directo: {}", path.display()))
            }
            Err(error) => report.fail(format!("no se pudo escribir {}: {error}", path.display())),
        }
    }

    report
}

pub fn unintegrate(options: &IntegrationOptions) -> IntegrationReport {
    let mut report = IntegrationReport::default();
    let Some(home) = options.home.clone().or_else(super::home_dir) else {
        report.fail("no se pudo determinar el directorio personal");
        return report;
    };

    let targets = [
        home.join(".local/share/applications").join(DESKTOP_FILE),
        home.join("Desktop").join(DESKTOP_FILE),
        home.join(".local/share/icons/hicolor/256x256/apps/whisper-desktop.png"),
        home.join(".local/bin").join(SYMLINK_NAME),
    ];

    for target in targets {
        match std::fs::remove_file(&target) {
            Ok(()) => report.done(format!("Eliminado: {}", target.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => report.fail(format!("no se pudo borrar {}: {error}", target.display())),
        }
    }

    report
}

fn write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Replace a stale link instead of failing.
    if std::fs::symlink_metadata(link).is_ok() {
        std::fs::remove_file(link)?;
    }
    let absolute: PathBuf = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(absolute, link)
    }
    #[cfg(not(unix))]
    {
        let _ = (absolute, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks no soportados",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wd-sys-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn options(home: &Path) -> IntegrationOptions {
        let mut options = IntegrationOptions::new(
            "Whisper Desktop",
            home.join(".local/share/whisper-desktop/WhisperDesktop"),
        );
        options.home = Some(home.to_path_buf());
        options.create_desktop_shortcut = true;
        options
    }

    #[test]
    fn installs_and_removes_everything() {
        let home = temp_home("roundtrip");
        let options = options(&home);

        let report = integrate(&options);
        assert!(report.ok(), "errores: {:?}", report.errors);

        let entry = home.join(".local/share/applications/whisper-desktop.desktop");
        assert!(entry.exists());
        let content = std::fs::read_to_string(&entry).unwrap();
        assert!(content.contains("Name=Whisper Desktop"));
        assert!(content.contains("Exec="));

        assert!(home
            .join(".local/share/icons/hicolor/256x256/apps/whisper-desktop.png")
            .exists());
        assert!(home.join("Desktop/whisper-desktop.desktop").exists());

        let link = home.join(".local/bin/whisper-desktop");
        let metadata = std::fs::symlink_metadata(&link).unwrap();
        assert!(metadata.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&link).unwrap(), options.exec_path);

        let removed = unintegrate(&options);
        assert!(removed.ok(), "errores: {:?}", removed.errors);
        assert!(!entry.exists());
        assert!(!link.exists());
        assert!(!home.join("Desktop/whisper-desktop.desktop").exists());
        assert!(!home
            .join(".local/share/icons/hicolor/256x256/apps/whisper-desktop.png")
            .exists());

        // Running again must not fail.
        let again = unintegrate(&options);
        assert!(again.ok());

        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn replaces_a_stale_symlink() {
        let home = temp_home("stale");
        let options = options(&home);
        let link = home.join(".local/bin/whisper-desktop");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, b"not a symlink").unwrap();

        let report = integrate(&options);
        assert!(report.ok(), "errores: {:?}", report.errors);
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());

        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn respects_disabled_options() {
        let home = temp_home("minimal");
        let mut options = options(&home);
        options.create_start_menu = false;
        options.create_desktop_shortcut = false;
        options.add_to_path = false;

        let report = integrate(&options);
        assert!(report.ok());
        assert!(!home
            .join(".local/share/applications/whisper-desktop.desktop")
            .exists());
        assert!(!home.join(".local/bin/whisper-desktop").exists());
        assert!(report.actions.is_empty());

        std::fs::remove_dir_all(&home).ok();
    }
}
