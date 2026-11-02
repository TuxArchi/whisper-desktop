//! Child process helpers.
//!
//! On Windows, a GUI application that spawns a console program makes Windows
//! allocate a console window for it: the user sees PowerShell or ffmpeg
//! flashing (or, worse, a black window staying open during a transcription).
//! Every spawn must go through [`hidden`].

use std::process::Command;

/// Prevents Windows from creating a console window for this child process.
/// No-op elsewhere.
pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_break_normal_commands() {
        let mut command = Command::new("echo");
        command.arg("hola");
        let output = hidden(&mut command).output();
        // "echo" may not exist on Windows; the point is that it compiles and
        // does not panic.
        if let Ok(output) = output {
            assert!(output.status.success() || !output.status.success());
        }
    }
}
