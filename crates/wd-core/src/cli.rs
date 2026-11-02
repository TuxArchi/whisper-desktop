//! Command line entry point (`--detect-only`).
//!
//! Also used by the GUI binary for the same diagnostics, so it lives in the
//! core instead of the CLI crate.

use std::process::ExitCode;

use crate::decide::{plan, InstallPlan};
use crate::hardware::HardwareReport;
use crate::i18n::{render, Lang};

pub fn run(args: Vec<String>) -> ExitCode {
    ExitCode::from(u8::try_from(run_code(args)).unwrap_or(1))
}

/// Same as [`run`] but returns a plain exit code, for binaries that need to
/// decide between CLI and GUI.
pub fn run_code(args: Vec<String>) -> i32 {
    let mut detect_only = false;
    let mut json = false;
    let mut fixture: Option<String> = None;
    let mut lang = Lang::Es;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--detect-only" => detect_only = true,
            "--json" => json = true,
            "--fixture" => {
                index += 1;
                match args.get(index) {
                    Some(path) => fixture = Some(path.clone()),
                    None => {
                        eprintln!("--fixture necesita una ruta");
                        return 2;
                    }
                }
            }
            "--lang" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    lang = Lang::parse(value);
                }
            }
            "--version" | "-V" => {
                println!("whisper-desktop {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            other => {
                eprintln!("argumento desconocido: {other}");
                print_help();
                return 2;
            }
        }
        index += 1;
    }

    if !detect_only {
        print_help();
        return 0;
    }

    let report = match fixture {
        Some(path) => match load_report(&path) {
            Ok(report) => report,
            Err(message) => {
                eprintln!("{message}");
                return 1;
            }
        },
        None => crate::detect::detect(),
    };

    let install_plan = plan(&report);

    if json {
        let payload = serde_json::json!({
            "report": report,
            "plan": install_plan,
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                eprintln!("no se pudo serializar: {error}");
                return 1;
            }
        }
    } else {
        print_human(&report, &install_plan, lang);
    }

    0
}

fn load_report(path: &str) -> Result<HardwareReport, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("no se pudo leer {path}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON inválido en {path}: {e}"))?;
    let report_value = value.get("report").cloned().unwrap_or(value);
    serde_json::from_value(report_value)
        .map_err(|e| format!("estructura de hardware inválida en {path}: {e}"))
}

fn print_help() {
    println!(
        "\
Whisper Desktop — diagnóstico de hardware

USO:
    whisper-desktop --detect-only [OPCIONES]

OPCIONES:
    --detect-only         Detecta el hardware y calcula el plan de instalación
    --json                Salida JSON (report + plan)
    --fixture <ruta>      Usa un informe de hardware guardado en lugar de detectar
    --lang <es|en>        Idioma de los mensajes (por defecto es)
    -V, --version         Muestra la versión
    -h, --help            Muestra esta ayuda

EJEMPLOS:
    whisper-desktop --detect-only
    whisper-desktop --detect-only --json > hardware.json
    whisper-desktop --detect-only --fixture tests/cases/rx6700xt-windows.json
"
    );
}

fn print_human(report: &HardwareReport, plan: &InstallPlan, lang: Lang) {
    let es = lang == Lang::Es;
    println!(
        "Whisper Desktop — {}",
        if es { "diagnóstico" } else { "diagnostics" }
    );
    println!();

    println!("  {:<10}{}", "Sistema", os_line(report));
    println!("  {:<10}{}", "CPU", cpu_line(report));
    println!("  {:<10}{} MB", "RAM", report.ram_mb);

    for gpu in &report.gpus {
        let vram = gpu
            .vram_mb
            .map(|mb| format!("{mb} MB"))
            .unwrap_or_else(|| "VRAM ?".to_string());
        println!(
            "  {:<10}{} — {} ({})",
            "GPU",
            gpu.label(),
            vram,
            gpu.family.as_str()
        );
    }

    if let Some(nv) = &report.nvidia {
        println!(
            "  {:<10}driver {}",
            "NVIDIA",
            nv.driver_version.clone().unwrap_or_else(|| "?".to_string())
        );
    }

    if let Some(vk) = &report.vulkan {
        println!(
            "  {:<10}{} ({} dispositivo(s))",
            "Vulkan",
            vk.api_version.clone().unwrap_or_else(|| "?".to_string()),
            vk.device_count
        );
    }

    println!();
    println!(
        "{}",
        if es {
            "Plan de instalación"
        } else {
            "Install plan"
        }
    );
    println!("  {:<10}{}", "Backend", plan.backend.as_str());
    if !plan.fallbacks.is_empty() {
        let list: Vec<&str> = plan.fallbacks.iter().map(|b| b.as_str()).collect();
        println!("  {:<10}{}", "Fallback", list.join(" → "));
    }
    println!(
        "  {:<10}{} ({}) — {}",
        if es { "Modelo" } else { "Model" },
        plan.model,
        plan.variant.as_str(),
        human_size(plan.download_size())
    );
    println!(
        "  {:<10}{}",
        if es { "Hilos" } else { "Threads" },
        plan.threads
    );

    if !plan.reasons.is_empty() {
        println!();
        println!("{}", if es { "Motivos" } else { "Reasons" });
        for reason in &plan.reasons {
            println!("  • {}", render(reason, lang));
        }
    }

    if !plan.warnings.is_empty() {
        println!();
        println!("{}", if es { "Avisos" } else { "Warnings" });
        for warning in &plan.warnings {
            println!("  ! {}", render(warning, lang));
        }
    }
}

fn os_line(report: &HardwareReport) -> String {
    let version = report
        .os
        .version
        .clone()
        .unwrap_or_else(|| report.os.family.clone());
    format!("{} ({})", version, report.os.arch)
}

fn cpu_line(report: &HardwareReport) -> String {
    let model = report
        .cpu
        .model
        .clone()
        .unwrap_or_else(|| "CPU".to_string());
    format!(
        "{} — {} núcleos / {} hilos",
        model, report.cpu.physical_cores, report.cpu.logical_cores
    )
}

fn human_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * MB;
    let value = bytes as f64;
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", value / GB)
    } else {
        format!("{:.0} MB", value / MB)
    }
}
