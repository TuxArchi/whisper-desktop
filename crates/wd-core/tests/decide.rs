//! Fixture driven tests for the installation planner.
//!
//! Each file in `tests/cases` describes a machine and the decision we expect.
//! Running these on Linux and Windows CI covers the whole matrix without any
//! real hardware.

use std::path::PathBuf;

use serde::Deserialize;
use wd_core::hardware::HardwareReport;

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    report: HardwareReport,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
struct Expect {
    backend: String,
    model: String,
    #[serde(default)]
    variant: Option<String>,
}

fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cases")
}

#[test]
fn decision_matrix() {
    let dir = cases_dir();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
        panic!("no se pudo leer {}: {e}", dir.display());
    });

    let mut checked = 0usize;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    for path in paths {
        let raw = std::fs::read_to_string(&path).expect("read case");
        let case: Case = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{}: JSON inválido: {e}", path.display()));

        let plan = wd_core::plan(&case.report);

        assert_eq!(
            plan.backend.as_str(),
            case.expect.backend,
            "{}: backend inesperado (motivos: {:?})",
            case.name,
            plan.reasons
        );
        assert_eq!(
            plan.model, case.expect.model,
            "{}: modelo inesperado",
            case.name
        );
        if let Some(variant) = case.expect.variant {
            assert_eq!(
                plan.variant.as_str(),
                variant,
                "{}: variante inesperada",
                case.name
            );
        }

        checked += 1;
    }

    assert!(
        checked >= 8,
        "se esperaban al menos 8 casos, se corrieron {checked}"
    );
}

#[test]
fn cpu_only_has_no_gpu_fallback() {
    let raw = std::fs::read_to_string(cases_dir().join("vm-nogpu-linux.json")).expect("fixture");
    let case: Case = serde_json::from_str(&raw).expect("parse");
    let plan = wd_core::plan(&case.report);
    assert!(plan.fallbacks.is_empty());
    assert_eq!(plan.threads, 4);
}
