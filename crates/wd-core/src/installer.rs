//! Install orchestration: catalogue in, working installation out.
//!
//! The order matters. The manifest is written **last**, so a failed install
//! never looks complete. Archives are verified before extraction and removed
//! afterwards.

use crate::catalog::{Catalog, Platform};
use crate::config::Config;
use crate::decide::{BackendId, InstallPlan};
use crate::download::Fetcher;
use crate::extract::{self, clear_dir};
use crate::manifest::Manifest;
use crate::paths::{default_output_dir, AppPaths};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("el catálogo no incluye {kind} para {target}")]
    MissingArtifact { kind: String, target: String },
    #[error(transparent)]
    Download(#[from] crate::download::DownloadError),
    #[error(transparent)]
    Extract(#[from] crate::extract::ExtractError),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum InstallEvent {
    Step { id: String, label: String },
    Progress { done: u64, total: u64 },
    Finished,
}

pub struct InstallRequest<'a> {
    pub plan: &'a InstallPlan,
    pub paths: &'a AppPaths,
    pub catalog: &'a Catalog,
    pub platform: Platform,
    pub app_version: String,
}

pub fn install<F: Fetcher>(
    request: &InstallRequest,
    fetcher: &F,
    emit: &mut dyn FnMut(InstallEvent),
) -> Result<Manifest, InstallError> {
    let paths = request.paths;
    let platform_name = request.platform.as_str().to_string();
    let backend_id = request.plan.backend.as_str();

    paths.ensure()?;
    let cache = paths.data.join(".cache");
    std::fs::create_dir_all(&cache)?;

    // --- 1. inference backend ------------------------------------------------
    let backend = request
        .catalog
        .backend(request.plan.backend, request.platform)
        .ok_or_else(|| InstallError::MissingArtifact {
            kind: "backend".to_string(),
            target: format!("{backend_id}/{platform_name}"),
        })?;

    emit(InstallEvent::Step {
        id: "backend".to_string(),
        label: format!("Motor {backend_id}"),
    });
    let backend_archive = cache.join(file_name_from_url(&backend.artifact.url));
    fetcher.fetch(
        &backend.artifact.url,
        &backend_archive,
        &backend.artifact.sha256,
        &mut |done, total| emit(InstallEvent::Progress { done, total }),
    )?;

    let backend_dir = paths.backend_dir(backend_id);
    clear_dir(&backend_dir)?;
    extract::extract(&backend_archive, &backend_dir)?;
    std::fs::remove_file(&backend_archive).ok();

    // --- 2. ffmpeg -----------------------------------------------------------
    if let Some(tool) = request.catalog.tool("ffmpeg-lgpl", request.platform) {
        emit(InstallEvent::Step {
            id: "ffmpeg".to_string(),
            label: "ffmpeg".to_string(),
        });
        let tool_archive = cache.join(file_name_from_url(&tool.artifact.url));
        fetcher.fetch(
            &tool.artifact.url,
            &tool_archive,
            &tool.artifact.sha256,
            &mut |done, total| emit(InstallEvent::Progress { done, total }),
        )?;
        // Tool archives ship a `bin/` prefix, so they unpack into the app root.
        extract::extract(&tool_archive, &paths.root)?;
        std::fs::remove_file(&tool_archive).ok();
    }

    // --- 3. model ------------------------------------------------------------
    let model = request.catalog.model(&request.plan.model).ok_or_else(|| {
        InstallError::MissingArtifact {
            kind: "modelo".to_string(),
            target: request.plan.model.clone(),
        }
    })?;

    emit(InstallEvent::Step {
        id: "model".to_string(),
        label: format!("Modelo {}", model.id),
    });
    let model_path = paths.model_path(&model.file);
    fetcher.fetch(
        &model.url,
        &model_path,
        &model.sha256,
        &mut |done, total| emit(InstallEvent::Progress { done, total }),
    )?;

    // --- 4. configuration ----------------------------------------------------
    emit(InstallEvent::Step {
        id: "config".to_string(),
        label: "Configuración".to_string(),
    });
    let mut config = Config::default();
    config.install.backend = Some(backend_id.to_string());
    config.install.backend_dir = Some(format!("backends/{backend_id}"));
    config.install.models = vec![request.plan.model.clone()];
    config.defaults.model = request.plan.model.clone();
    config.defaults.output_dir = default_output_dir().to_string_lossy().to_string();
    config.save(&paths.config_file)?;

    // --- 5. manifest (last) --------------------------------------------------
    let mut files = extract::collect_files(&paths.backend_dir(backend_id), &paths.root)?;
    files.extend(extract::collect_files(&paths.bin, &paths.root)?);

    let mut manifest = Manifest::new(request.app_version.clone(), request.plan.backend, files);
    manifest.models = vec![model.file.clone()];
    manifest.save(&paths.manifest_file)?;

    std::fs::remove_dir_all(&cache).ok();
    emit(InstallEvent::Finished);
    Ok(manifest)
}

/// The decision engine knows the hardware; the catalogue knows what has been
/// published. They drift (a Windows-only build, a model not released yet), so
/// the installer adapts instead of failing with "missing artifact".
pub struct PlanFit {
    pub plan: InstallPlan,
    pub notes: Vec<crate::hardware::Reason>,
}

pub fn fit_plan_to_catalog(plan: &InstallPlan, catalog: &Catalog, platform: Platform) -> PlanFit {
    let mut adjusted = plan.clone();
    let mut notes = Vec::new();

    if catalog.backend(plan.backend, platform).is_none() {
        let replacement = std::iter::once(plan.backend)
            .chain(plan.fallbacks.iter().copied())
            .find(|candidate| catalog.backend(*candidate, platform).is_some())
            .or_else(|| {
                catalog
                    .backends
                    .iter()
                    .filter(|backend| backend.platform == platform)
                    .max_by_key(|backend| backend.priority)
                    .and_then(|backend| match backend.id.as_str() {
                        "cpu" => Some(BackendId::Cpu),
                        "cuda" => Some(BackendId::Cuda),
                        "vulkan" => Some(BackendId::Vulkan),
                        "rocm" => Some(BackendId::Rocm),
                        _ => None,
                    })
            });

        if let Some(backend) = replacement {
            adjusted.backend = backend;
            adjusted.fallbacks.retain(|candidate| {
                *candidate != backend && catalog.backend(*candidate, platform).is_some()
            });
            notes.push(
                crate::hardware::Reason::new("catalog.backend_fallback")
                    .arg("wanted", plan.backend.as_str())
                    .arg("used", backend.as_str()),
            );
        }
    }

    if catalog.model(&plan.model).is_none() {
        if let Some(model) = catalog.models.iter().max_by_key(|model| model.size) {
            adjusted.model = model.id.clone();
            notes.push(
                crate::hardware::Reason::new("catalog.model_fallback")
                    .arg("wanted", plan.model.clone())
                    .arg("used", model.id.clone()),
            );
        }
    }

    PlanFit {
        plan: adjusted,
        notes,
    }
}

fn file_name_from_url(url: &str) -> String {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    without_query
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("artifact.bin")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Artifact, CatalogBackend, CatalogModel};
    use crate::decide::{BackendId, ModelVariant};
    use crate::download::LocalFetcher;
    use std::path::Path;

    fn build_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *content).unwrap();
        }
        builder.finish().unwrap();
    }

    #[test]
    fn full_install_from_local_sources() {
        let dir = std::env::temp_dir().join(format!("wd-install-{}", std::process::id()));
        let downloads = dir.join("downloads");
        let app = dir.join("app");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&app).unwrap();

        // Backend bundle: two files, one executable.
        let backend_archive = downloads.join("backend.tar.gz");
        build_tar_gz(
            &backend_archive,
            &[("whisper-cli", b"binary"), ("lib/libggml.so", b"lib")],
        );

        // Tool bundle: `bin/ffmpeg`, relative to the app root.
        let tool_archive = downloads.join("ffmpeg.tar.gz");
        build_tar_gz(&tool_archive, &[("bin/ffmpeg", b"tool")]);

        // Model: a plain file.
        let model_file = downloads.join("ggml-small.bin");
        std::fs::write(&model_file, b"weights").unwrap();

        let sha = |path: &Path| crate::download::sha256_file(path).unwrap();

        let catalog = Catalog {
            schema: 1,
            channel: "test".into(),
            min_app_version: None,
            backends: vec![CatalogBackend {
                id: "vulkan".into(),
                platform: Platform::LinuxX64,
                priority: 20,
                artifact: Artifact {
                    url: "https://example.com/backend.tar.gz".into(),
                    sha256: sha(&backend_archive),
                    size: 1024,
                },
                env: Default::default(),
            }],
            tools: vec![crate::catalog::CatalogTool {
                id: "ffmpeg-lgpl".into(),
                platform: Platform::LinuxX64,
                version: None,
                artifact: Artifact {
                    url: "https://example.com/ffmpeg.tar.gz".into(),
                    sha256: sha(&tool_archive),
                    size: 1024,
                },
            }],
            models: vec![CatalogModel {
                id: "small".into(),
                file: "ggml-small.bin".into(),
                url: "https://example.com/ggml-small.bin".into(),
                sha256: sha(&model_file),
                size: 7,
                vram_mb: 1024,
                multilingual: true,
            }],
        };

        let plan = InstallPlan {
            backend: BackendId::Vulkan,
            fallbacks: vec![BackendId::Cpu],
            model: "small".into(),
            variant: ModelVariant::Fp16,
            threads: 6,
            reasons: vec![],
            warnings: vec![],
        };

        let paths = crate::paths::resolve(&app);
        paths.ensure().unwrap();

        let request = InstallRequest {
            plan: &plan,
            paths: &paths,
            catalog: &catalog,
            platform: Platform::LinuxX64,
            app_version: "0.1.0".into(),
        };

        let fetcher = LocalFetcher {
            root: downloads.clone(),
        };
        let mut steps: Vec<String> = Vec::new();
        let manifest = install(&request, &fetcher, &mut |event| {
            if let InstallEvent::Step { id, .. } = event {
                steps.push(id);
            }
        })
        .unwrap();

        assert_eq!(steps, vec!["backend", "ffmpeg", "model", "config"]);
        assert!(app.join("backends/vulkan/whisper-cli").exists());
        assert!(app.join("bin/ffmpeg").exists());
        assert!(paths.model_path("ggml-small.bin").exists());
        assert!(paths.config_file.exists());
        assert!(paths.manifest_file.exists());

        // Models live outside the manifest's file list but are tracked by name.
        assert_eq!(manifest.models, vec!["ggml-small.bin"]);
        assert!(paths.model_path(&manifest.models[0]).exists());

        let loaded = Manifest::load(&paths.manifest_file).unwrap();
        assert!(loaded.is_installed(&app));
        assert_eq!(loaded.backend, BackendId::Vulkan);

        let config = Config::load(&paths.config_file).unwrap();
        assert_eq!(config.install.backend.as_deref(), Some("vulkan"));
        assert_eq!(config.install.models, vec!["small"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_backend_is_an_error() {
        let dir = std::env::temp_dir().join(format!("wd-install-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = Catalog {
            schema: 1,
            channel: "test".into(),
            min_app_version: None,
            backends: vec![],
            tools: vec![],
            models: vec![],
        };
        let plan = InstallPlan {
            backend: BackendId::Cuda,
            fallbacks: vec![],
            model: "small".into(),
            variant: ModelVariant::Fp16,
            threads: 4,
            reasons: vec![],
            warnings: vec![],
        };
        let paths = crate::paths::resolve(&dir);
        paths.ensure().unwrap();
        let request = InstallRequest {
            plan: &plan,
            paths: &paths,
            catalog: &catalog,
            platform: Platform::LinuxX64,
            app_version: "0.1.0".into(),
        };
        let error =
            install(&request, &LocalFetcher { root: dir.clone() }, &mut |_| {}).unwrap_err();
        assert!(matches!(error, InstallError::MissingArtifact { .. }));
        assert!(!paths.manifest_file.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_name_extraction() {
        assert_eq!(file_name_from_url("https://a/b/c.zip?x=1"), "c.zip");
        assert_eq!(file_name_from_url("https://a/b/whisper-cli"), "whisper-cli");
        // A URL without a file name falls back to a safe default.
        assert_eq!(file_name_from_url("https://a/b/"), "artifact.bin");
        assert_eq!(file_name_from_url("nonsense"), "nonsense");
    }
    #[test]
    fn adapts_plan_to_published_catalog() {
        use crate::catalog::{Artifact, CatalogBackend, CatalogModel};
        use crate::decide::{BackendId, ModelVariant};

        let catalog = Catalog {
            schema: 1,
            channel: "test".into(),
            min_app_version: None,
            backends: vec![CatalogBackend {
                id: "cpu".into(),
                platform: Platform::LinuxX64,
                priority: 10,
                artifact: Artifact {
                    url: "https://example.com/cpu.tar.gz".into(),
                    sha256: "a".repeat(64),
                    size: 10,
                },
                env: Default::default(),
            }],
            tools: vec![],
            models: vec![CatalogModel {
                id: "base".into(),
                file: "ggml-base.bin".into(),
                url: "https://example.com/ggml-base.bin".into(),
                sha256: "b".repeat(64),
                size: 100,
                vram_mb: 640,
                multilingual: true,
            }],
        };

        let plan = InstallPlan {
            backend: BackendId::Vulkan,
            fallbacks: vec![BackendId::Cpu],
            model: "large-v3-turbo".into(),
            variant: ModelVariant::Fp16,
            threads: 8,
            reasons: vec![],
            warnings: vec![],
        };

        let fit = fit_plan_to_catalog(&plan, &catalog, Platform::LinuxX64);
        assert_eq!(fit.plan.backend, BackendId::Cpu);
        assert_eq!(fit.plan.model, "base");
        assert!(fit
            .plan
            .fallbacks
            .iter()
            .all(|candidate| *candidate != BackendId::Cpu));
        assert_eq!(fit.notes.len(), 2);
        assert_eq!(fit.notes[0].code, "catalog.backend_fallback");
        assert_eq!(fit.notes[1].code, "catalog.model_fallback");
    }

    #[test]
    fn leaves_a_matching_plan_untouched() {
        use crate::catalog::{Artifact, CatalogBackend, CatalogModel};
        use crate::decide::{BackendId, ModelVariant};

        let catalog = Catalog {
            schema: 1,
            channel: "test".into(),
            min_app_version: None,
            backends: vec![CatalogBackend {
                id: "vulkan".into(),
                platform: Platform::LinuxX64,
                priority: 20,
                artifact: Artifact {
                    url: "https://example.com/vk.tar.gz".into(),
                    sha256: "c".repeat(64),
                    size: 10,
                },
                env: Default::default(),
            }],
            tools: vec![],
            models: vec![CatalogModel {
                id: "large-v3-turbo".into(),
                file: "ggml-large-v3-turbo.bin".into(),
                url: "https://example.com/m.bin".into(),
                sha256: "d".repeat(64),
                size: 100,
                vram_mb: 3072,
                multilingual: true,
            }],
        };

        let plan = InstallPlan {
            backend: BackendId::Vulkan,
            fallbacks: vec![BackendId::Cpu],
            model: "large-v3-turbo".into(),
            variant: ModelVariant::Fp16,
            threads: 8,
            reasons: vec![],
            warnings: vec![],
        };

        let fit = fit_plan_to_catalog(&plan, &catalog, Platform::LinuxX64);
        assert_eq!(fit.plan, plan);
        assert!(fit.notes.is_empty());
    }
}
