//! Windows integration through PowerShell.
//!
//! Everything is HKCU-scoped: no elevation, no machine-wide changes. The
//! script content lives in `scripts::windows_*` so it can be asserted in
//! tests on any operating system.

use std::process::Command;

use super::{scripts, IntegrationOptions, IntegrationReport};
use crate::process::hidden;

pub fn integrate(options: &IntegrationOptions) -> IntegrationReport {
    run(&scripts::windows_integrate_script(options), "integrada")
}

pub fn unintegrate(options: &IntegrationOptions) -> IntegrationReport {
    run(&scripts::windows_unintegrate_script(options), "eliminada")
}

fn run(script: &str, label: &str) -> IntegrationReport {
    let mut report = IntegrationReport::default();

    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);
    let output = hidden(&mut command).output();

    match output {
        Ok(output) if output.status.success() => {
            report.done(format!("Integración {label}"));
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            report.fail(format!("PowerShell devolvió un error: {}", stderr.trim()));
        }
        Err(error) => {
            report.fail(format!("no se pudo ejecutar PowerShell: {error}"));
        }
    }

    report
}
