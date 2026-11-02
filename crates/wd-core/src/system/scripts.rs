//! Pure text generation for system integration.
//!
//! Kept separate from the code that touches the filesystem or the registry so
//! the exact content (registry keys, Exec line, quoting) can be asserted in
//! tests on any platform.

use super::IntegrationOptions;

/// Extensions offered in "Open with". Video is included because ffmpeg is
/// always installed and the Studio accepts it directly.
pub const SUPPORTED_TYPES: &[&str] = &[
    ".mp3", ".wav", ".m4a", ".flac", ".ogg", ".opus", ".mp4", ".mkv", ".mov", ".webm",
];

/// `.desktop` file content. `Exec` uses `%F` so the file manager can pass
/// several selected files.
pub fn desktop_entry(options: &IntegrationOptions) -> String {
    let exec = options.exec_path.to_string_lossy();
    let quoted = quote_exec(&exec);
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={name}\n\
         Comment=Transcribe y traduce audio en local\n\
         Exec={quoted} %F\n\
         TryExec={quoted}\n\
         Icon=whisper-desktop\n\
         Terminal=false\n\
         Categories=AudioVideo;Audio;Utility;\n\
         MimeType=audio/*;video/*;\n\
         StartupWMClass=WhisperDesktop\n",
        name = options.app_name,
        quoted = quoted,
    )
}

/// Desktop entry quoting rules: quote when the path contains spaces.
fn quote_exec(path: &str) -> String {
    if path.contains(' ') || path.contains('\'') {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

/// Escapes a value for a single quoted PowerShell string.
fn ps(value: &str) -> String {
    value.replace('\'', "''")
}

/// `Path::file_name` only splits on the platform separator, and this script
/// can be generated (and tested) on any OS, so split on both.
fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("WhisperDesktop.exe")
        .to_string()
}

/// PowerShell that creates the Start Menu / Desktop shortcuts, the user PATH
/// entry and the "Open with" registration. All under HKCU: no admin needed.
pub fn windows_integrate_script(options: &IntegrationOptions) -> String {
    let exe = ps(&options.exec_path.to_string_lossy());
    let name = ps(&options.app_name);
    let short_name = ps(&base_name(&options.exec_path.to_string_lossy()));

    let mut script = String::from(
        "$ErrorActionPreference = 'SilentlyContinue'\n\
         $exe = '",
    );
    script.push_str(&exe);
    script.push_str(
        "'\n\
         $dir = Split-Path $exe\n\
         $ws = New-Object -ComObject WScript.Shell\n",
    );

    if options.create_start_menu {
        script.push_str(
            "\
$startMenu = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs'\n\
$lnk = $ws.CreateShortcut((Join-Path $startMenu '",
        );
        script.push_str(&name);
        script.push_str(
            ".lnk'))\n\
$lnk.TargetPath = $exe\n\
$lnk.WorkingDirectory = $dir\n\
$lnk.IconLocation = \"$exe,0\"\n\
$lnk.Save()\n",
        );
    }

    if options.create_desktop_shortcut {
        script.push_str(
            "\
$desktop = [Environment]::GetFolderPath('Desktop')\n\
$lnk = $ws.CreateShortcut((Join-Path $desktop '",
        );
        script.push_str(&name);
        script.push_str(
            ".lnk'))\n\
$lnk.TargetPath = $exe\n\
$lnk.WorkingDirectory = $dir\n\
$lnk.IconLocation = \"$exe,0\"\n\
$lnk.Save()\n",
        );
    }

    if options.add_to_path {
        script.push_str(
            "\
$current = [Environment]::GetEnvironmentVariable('Path','User')\n\
if (-not $current) { $current = '' }\n\
$parts = @($current -split ';' | Where-Object { $_ })\n\
if (-not ($parts -contains $dir)) {\n\
  $new = if ($parts.Count -gt 0) { ($parts -join ';') + ';' + $dir } else { $dir }\n\
  [Environment]::SetEnvironmentVariable('Path', $new, 'User')\n\
}\n",
        );
    }

    if options.register_open_with {
        script.push_str(
            "\
$appKey = 'HKCU:\\Software\\Classes\\Applications\\",
        );
        script.push_str(&short_name);
        script.push_str(
            "'\n\
New-Item -Path \"$appKey\\shell\\open\\command\" -Force | Out-Null\n\
Set-ItemProperty -Path $appKey -Name 'FriendlyAppName' -Value '",
        );
        script.push_str(&name);
        script.push_str(
            "'\n\
Set-ItemProperty -Path \"$appKey\\shell\\open\\command\" -Name '(default)' -Value ('\"' + $exe + '\" \"%1\"')\n\
foreach ($ext in @(",
        );
        for (index, extension) in SUPPORTED_TYPES.iter().enumerate() {
            if index > 0 {
                script.push(',');
            }
            script.push('\'');
            script.push_str(extension);
            script.push('\'');
        }
        script.push_str(
            ")) {\n\
  New-Item -Path \"$appKey\\SupportedTypes\" -Force | Out-Null\n\
  New-ItemProperty -Path \"$appKey\\SupportedTypes\" -Name $ext -Value '' -PropertyType String -Force | Out-Null\n\
}\n",
        );
    }

    script
}

/// PowerShell that undoes `windows_integrate_script`.
pub fn windows_unintegrate_script(options: &IntegrationOptions) -> String {
    let exe = ps(&options.exec_path.to_string_lossy());
    let name = ps(&options.app_name);
    let short_name = ps(&base_name(&options.exec_path.to_string_lossy()));

    format!(
        "$ErrorActionPreference = 'SilentlyContinue'\n\
         $exe = '{exe}'\n\
         $dir = Split-Path $exe\n\
         Remove-Item (Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\{name}.lnk') -Force\n\
         Remove-Item (Join-Path ([Environment]::GetFolderPath('Desktop')) '{name}.lnk') -Force\n\
         $current = [Environment]::GetEnvironmentVariable('Path','User')\n\
         if ($current) {{\n\
           $parts = @($current -split ';' | Where-Object {{ $_ -and $_ -ne $dir }})\n\
           [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')\n\
         }}\n\
         Remove-Item 'HKCU:\\Software\\Classes\\Applications\\{short_name}' -Recurse -Force\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn options() -> IntegrationOptions {
        IntegrationOptions::new(
            "Whisper Desktop",
            PathBuf::from("/opt/Whisper Desktop/WhisperDesktop"),
        )
    }

    #[test]
    fn desktop_entry_is_valid_and_quoted() {
        let entry = desktop_entry(&options());
        assert!(entry.starts_with("[Desktop Entry]"));
        assert!(entry.contains("Name=Whisper Desktop"));
        assert!(entry.contains("Exec=\"/opt/Whisper Desktop/WhisperDesktop\" %F"));
        assert!(entry.contains("TryExec=\"/opt/Whisper Desktop/WhisperDesktop\""));
        assert!(entry.contains("MimeType=audio/*;video/*;"));
        assert!(entry.contains("Icon=whisper-desktop"));
        assert!(entry.contains("Terminal=false"));
    }

    #[test]
    fn desktop_entry_does_not_quote_simple_paths() {
        let mut options = options();
        options.exec_path = PathBuf::from("/usr/bin/whisper-desktop");
        let entry = desktop_entry(&options);
        assert!(entry.contains("Exec=/usr/bin/whisper-desktop %F"));
    }

    #[test]
    fn windows_script_registers_shortcuts_path_and_open_with() {
        let mut options = options();
        options.exec_path = PathBuf::from("C:\\Apps\\WhisperDesktop\\WhisperDesktop.exe");
        options.create_desktop_shortcut = true;

        let script = windows_integrate_script(&options);
        assert!(script.contains("WScript.Shell"));
        assert!(script.contains("CreateShortcut"));
        assert!(script.contains("Start Menu"));
        assert!(script.contains("GetFolderPath('Desktop')"));
        assert!(script.contains("GetEnvironmentVariable('Path','User')"));
        assert!(script.contains("SetEnvironmentVariable('Path', $new, 'User')"));
        assert!(script.contains("HKCU:\\Software\\Classes\\Applications\\WhisperDesktop.exe"));
        assert!(script.contains("\"%1\""));
        for extension in SUPPORTED_TYPES {
            assert!(script.contains(extension), "falta {extension}");
        }
    }

    #[test]
    fn windows_script_respects_disabled_options() {
        let mut options = options();
        options.create_start_menu = false;
        options.create_desktop_shortcut = false;
        options.add_to_path = false;
        options.register_open_with = false;
        let script = windows_integrate_script(&options);
        assert!(!script.contains("CreateShortcut"));
        assert!(!script.contains("SetEnvironmentVariable"));
        assert!(!script.contains("Applications"));
    }

    #[test]
    fn windows_uninstall_removes_the_same_things() {
        let script = windows_unintegrate_script(&options());
        assert!(script.contains("Remove-Item"));
        assert!(script.contains("Start Menu"));
        assert!(script.contains("Where-Object { $_ -and $_ -ne $dir }"));
        assert!(script.contains("HKCU:\\Software\\Classes\\Applications"));
    }

    #[test]
    fn powershell_single_quotes_are_escaped() {
        let mut options = options();
        options.exec_path = PathBuf::from("C:\\O'Brien\\WhisperDesktop.exe");
        let script = windows_integrate_script(&options);
        assert!(script.contains("C:\\O''Brien\\WhisperDesktop.exe"));
    }
}
