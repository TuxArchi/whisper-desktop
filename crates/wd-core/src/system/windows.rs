//! Windows integration through PowerShell.
//!
//! Everything is HKCU-scoped: no elevation, no machine-wide changes. The
//! script content lives in `scripts::windows_*` so it can be asserted in
//! tests on any operating system.

use std::process::Command;

use super::{scripts, IntegrationOptions, IntegrationReport};

pub fn integrate(options: &IntegrationOptions) -> IntegrationReport {
    run(&scripts::windows_integrate_script(options), "integrada")
}

pub fn unintegrate(options: &IntegrationOptions) -> IntegrationReport {
    run(&scripts::windows_unintegrate_script(options), "eliminada")
}

fn run(script: &str, label: &str) -> IntegrationReport {
    let mut report = IntegrationReport::default();

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            report.actions.push(format!("Integración {label}"));
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            report
                .errors
                .push(format!("PowerShell devolvió un error: {}", stderr.trim()));
        }
        Err(error) => {
            report
                .errors
                .push(format!("no se pudo ejecutar PowerShell: {error}"));
        }
    }

    report
}
