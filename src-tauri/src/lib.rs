//! Tauri shell. Kept intentionally thin: every decision and every piece of
//! filesystem knowledge lives in `wd-core`.

mod studio;

use std::path::PathBuf;

use serde::Serialize;
use tauri::{Emitter, Manager};
use wd_core::catalog::{Catalog, Platform};
use wd_core::installer::{install, InstallEvent, InstallRequest};
use wd_core::manifest::Manifest;
use wd_core::paths::{self, AppPaths};

/// Stable URL served by GitHub Pages, then the release that published it.
const CATALOG_URLS: &[&str] = &[
    "https://tuxarchi.github.io/whisper-desktop/catalog.json",
    "https://github.com/TuxArchi/whisper-desktop/releases/download/backends-r0/catalog.json",
];

/// Last resort when there is no network and no cache. Still verified.
const EMBEDDED_CATALOG: &str = include_str!("../../catalog/catalog.json");
const EMBEDDED_SIGNATURE: &str = include_str!("../../catalog/catalog.json.sig");

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStateView {
    installed: bool,
    app_version: String,
    data_dir: String,
    platform: String,
    portable: bool,
    zip_preview: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogStatus {
    ok: bool,
    backends: usize,
    /// Every model the catalogue publishes, so the user can choose.
    models: Vec<ModelChoice>,
    /// Model the hardware suggests.
    recommended: String,
    errors: Vec<String>,
    /// Differences between what the hardware suggests and what is published.
    notes: Vec<wd_core::Reason>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelChoice {
    id: String,
    size: u64,
    vram_mb: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyView {
    working: Option<String>,
    rtf: f64,
    tried: Vec<String>,
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolved_paths() -> AppPaths {
    paths::resolve_configured(&app_dir())
}

fn load_catalog() -> Result<Catalog, String> {
    let cache_dir = resolved_paths().data.join("cache");
    let urls: Vec<String> = CATALOG_URLS.iter().map(|url| url.to_string()).collect();

    match wd_core::catalog_source::load(
        &wd_core::catalog_source::HttpTextFetcher,
        &urls,
        &cache_dir,
    ) {
        Ok(loaded) => Ok(loaded.catalog),
        Err(remote_error) => {
            // Offline or hosting broken: fall back to the copy compiled into
            // the binary, which is signed with the same key.
            match wd_core::catalog_source::verify_and_parse(EMBEDDED_CATALOG, EMBEDDED_SIGNATURE) {
                Ok(catalog) => Ok(catalog),
                Err(_) => Err(format!("no se pudo obtener el catálogo: {remote_error}")),
            }
        }
    }
}

#[tauri::command]
fn get_state() -> AppStateView {
    let root = app_dir();
    let app_paths = resolved_paths();

    let installed = Manifest::load(&app_paths.manifest_file)
        .map(|manifest| manifest.is_installed(&root))
        .unwrap_or(false);

    AppStateView {
        installed,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: app_paths.data.to_string_lossy().to_string(),
        platform: Platform::current().as_str().to_string(),
        portable: app_paths.data == root.join("data"),
        zip_preview: paths::looks_like_zip_preview(&root),
    }
}

#[tauri::command]
fn detect_hardware() -> wd_core::HardwareReport {
    wd_core::detect::detect()
}

#[tauri::command]
fn build_plan() -> wd_core::InstallPlan {
    wd_core::plan(&wd_core::detect::detect())
}

#[tauri::command]
async fn catalog_status() -> CatalogStatus {
    // Fetching can take seconds; never block the UI thread.
    tauri::async_runtime::spawn_blocking(catalog_status_blocking)
        .await
        .unwrap_or_else(|error| CatalogStatus {
            ok: false,
            backends: 0,
            models: Vec::new(),
            recommended: String::new(),
            errors: vec![error.to_string()],
            notes: Vec::new(),
        })
}

fn catalog_status_blocking() -> CatalogStatus {
    match load_catalog() {
        Ok(catalog) => {
            let errors = catalog.validate(env!("CARGO_PKG_VERSION"));
            let fit = wd_core::installer::fit_plan_to_catalog(
                &wd_core::plan(&wd_core::detect::detect()),
                &catalog,
                Platform::current(),
            );

            let mut models: Vec<ModelChoice> = catalog
                .models
                .iter()
                .map(|model| ModelChoice {
                    id: model.id.clone(),
                    size: model.size,
                    vram_mb: model.vram_mb,
                })
                .collect();
            models.sort_by_key(|model| model.size);

            CatalogStatus {
                ok: errors.is_empty(),
                backends: catalog.backends.len(),
                models,
                recommended: fit.plan.model.clone(),
                errors,
                notes: fit.notes,
            }
        }
        Err(error) => CatalogStatus {
            ok: false,
            backends: 0,
            models: Vec::new(),
            recommended: String::new(),
            errors: vec![error],
            notes: Vec::new(),
        },
    }
}

#[tauri::command]
async fn start_install(
    app: tauri::AppHandle,
    model: Option<String>,
) -> Result<Manifest, String> {
    tauri::async_runtime::spawn_blocking(move || install_blocking(&app, model))
        .await
        .map_err(|error| error.to_string())?
}

fn install_blocking(
    app: &tauri::AppHandle,
    model: Option<String>,
) -> Result<Manifest, String> {
    let app_paths = resolved_paths();
    let catalog = load_catalog()?;

    let errors = catalog.validate(env!("CARGO_PKG_VERSION"));
    if !errors.is_empty() {
        return Err(format!(
            "el catálogo de descargas no está listo:\n{}",
            errors.join("\n")
        ));
    }

    let report = wd_core::detect::detect();
    // Install what the catalogue actually publishes, not what we wish it did.
    let fit = wd_core::installer::fit_plan_to_catalog(
        &wd_core::plan(&report),
        &catalog,
        Platform::current(),
    );
    let mut plan = fit.plan;

    // The user may prefer a different published model than the recommended one.
    if let Some(chosen) = model {
        if catalog.model(&chosen).is_some() {
            plan.model = chosen;
        }
    }

    let request = InstallRequest {
        plan: &plan,
        paths: &app_paths,
        catalog: &catalog,
        platform: Platform::current(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let mut emit = |event: InstallEvent| {
        let _ = app.emit("install:event", &event);
    };

    install(&request, &wd_core::download::HttpFetcher, &mut emit)
        .map_err(|error| error.to_string())
}

/// Re-downloads the installed backend from the current catalogue. Used when the
/// engine is broken or was published with a bad build.
#[tauri::command]
async fn repair_engine(app: tauri::AppHandle) -> Result<Manifest, String> {
    tauri::async_runtime::spawn_blocking(move || repair_blocking(&app))
        .await
        .map_err(|error| error.to_string())?
}

fn repair_blocking(app: &tauri::AppHandle) -> Result<Manifest, String> {
    let app_paths = resolved_paths();
    let catalog = load_catalog()?;
    let manifest = Manifest::load(&app_paths.manifest_file).map_err(|error| error.to_string())?;

    let mut emit = |event: InstallEvent| {
        let _ = app.emit("install:event", &event);
    };

    let refreshed = wd_core::installer::refresh_backend(
        &app_paths,
        &catalog,
        manifest.backend,
        Platform::current(),
        env!("CARGO_PKG_VERSION"),
        &wd_core::download::HttpFetcher,
        &mut emit,
    )
    .map_err(|error| error.to_string())?;

    let mut config = wd_core::config::Config::load_or_default(&app_paths.config_file);
    config.install.backend = Some(refreshed.backend.as_str().to_string());
    config.install.backend_dir = Some(format!("backends/{}", refreshed.backend.as_str()));
    config.save(&app_paths.config_file).ok();

    Ok(refreshed)
}

#[tauri::command]
async fn verify_installation() -> Result<VerifyView, String> {
    tauri::async_runtime::spawn_blocking(verify_blocking)
        .await
        .map_err(|error| error.to_string())?
}

fn verify_blocking() -> Result<VerifyView, String> {
    let app_paths = resolved_paths();
    let manifest = Manifest::load(&app_paths.manifest_file).map_err(|error| error.to_string())?;
    let mut config = wd_core::config::Config::load_or_default(&app_paths.config_file);

    let report = wd_core::detect::detect();
    let mut plan = wd_core::plan(&report);
    // Verify what is actually installed, not what we would choose today.
    plan.backend = manifest.backend;

    let model_name = manifest
        .models
        .first()
        .ok_or_else(|| "el manifest no registra ningún modelo".to_string())?;
    let model = app_paths.model_path(model_name);
    let threads = report.cpu.thread_count();

    let outcome = wd_core::bench::verify_install(
        &app_paths,
        &plan,
        &model,
        threads,
        &wd_core::bench::SystemRunner,
    );

    if let Some(backend) = outcome.working {
        config.install.verified_backend = Some(backend.as_str().to_string());
        config.install.verified_rtf = outcome
            .report
            .as_ref()
            .map(|report| format!("{:.2}", report.rtf));
        config.save(&app_paths.config_file).ok();
    }

    Ok(VerifyView {
        working: outcome.working.map(|backend| backend.as_str().to_string()),
        rtf: outcome.report.map(|report| report.rtf).unwrap_or(0.0),
        tried: outcome
            .tried
            .iter()
            .map(|backend| backend.as_str().to_string())
            .collect(),
    })
}

#[tauri::command]
fn integrate_system() -> wd_core::system::IntegrationReport {
    integration_options().map_or_else(
        || wd_core::system::IntegrationReport {
            actions: Vec::new(),
            errors: vec!["no se pudo resolver la ruta del ejecutable".to_string()],
        },
        |options| wd_core::system::integrate(&options),
    )
}

#[tauri::command]
fn unintegrate_system() -> wd_core::system::IntegrationReport {
    integration_options().map_or_else(
        || wd_core::system::IntegrationReport {
            actions: Vec::new(),
            errors: vec!["no se pudo resolver la ruta del ejecutable".to_string()],
        },
        |options| wd_core::system::unintegrate(&options),
    )
}

fn integration_options() -> Option<wd_core::system::IntegrationOptions> {
    let exe = std::env::current_exe().ok()?;
    let mut options = wd_core::system::IntegrationOptions::new("Whisper Desktop", exe);
    options.create_desktop_shortcut = true;
    Some(options)
}

/// Handles invocations that must not open a window. Returns the exit code in
/// that case, or `None` when the GUI should start.
pub fn handle_cli(args: &[String]) -> Option<i32> {
    if args.iter().any(|arg| arg == "--detect-only") {
        return Some(wd_core::cli::run_code(args.to_vec()));
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("Whisper Desktop {}", env!("CARGO_PKG_VERSION"));
        return Some(0);
    }
    if args.iter().any(|arg| arg == "--integrate") {
        return Some(run_integration(true));
    }
    if args.iter().any(|arg| arg == "--unintegrate") {
        return Some(run_integration(false));
    }
    None
}

fn run_integration(install: bool) -> i32 {
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("no se pudo resolver la ruta del ejecutable");
        return 1;
    };

    let mut options = wd_core::system::IntegrationOptions::new("Whisper Desktop", exe);
    options.create_desktop_shortcut = true;

    let report = if install {
        wd_core::system::integrate(&options)
    } else {
        wd_core::system::unintegrate(&options)
    };

    for action in &report.actions {
        println!("{action}");
    }
    for error in &report.errors {
        eprintln!("{error}");
    }

    if report.errors.is_empty() {
        0
    } else {
        1
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: it decides whether this process owns the
        // app or just forwards its arguments to the running one.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let files = studio::media_from_args(argv);
            if !files.is_empty() {
                let _ = app.emit("open-files", files);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            detect_hardware,
            build_plan,
            catalog_status,
            start_install,
            verify_installation,
            integrate_system,
            unintegrate_system,
            studio::studio_state,
            studio::preview_output,
            studio::start_job,
            studio::cancel_job,
            studio::list_history,
            studio::open_path,
            studio::scan_folder,
            studio::initial_files,
            studio::data_info,
            studio::relocate_data,
            repair_engine
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Whisper Desktop");
}
