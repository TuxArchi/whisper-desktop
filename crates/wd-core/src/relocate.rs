//! Relocating the user data directory.
//!
//! Moving 1.8 GB of models is a destructive operation if it is done naively.
//! This module copies first, verifies every byte with SHA-256, and only then
//! removes the original. Any failure leaves the old location untouched.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::paths::AppPaths;

#[derive(Debug, thiserror::Error)]
pub enum RelocateError {
    #[error("no se puede mover la carpeta dentro de sí misma")]
    NestedTarget,
    #[error("el destino no se puede escribir: {0}")]
    NotWritable(String),
    #[error("espacio insuficiente en el destino: {needed_mb} MB necesarios, {free_mb} MB libres")]
    NotEnoughSpace { needed_mb: u64, free_mb: u64 },
    #[error("la copia de {file} no coincide (hash distinto)")]
    HashMismatch { file: String },
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelocateReport {
    pub target: PathBuf,
    pub moved_files: usize,
    pub moved_bytes: u64,
    pub kept_copy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataInfo {
    pub dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

pub fn data_info(paths: &AppPaths) -> DataInfo {
    let mut files = 0;
    let mut bytes = 0;
    walk(&paths.data, &mut |path| {
        if let Ok(metadata) = std::fs::metadata(path) {
            files += 1;
            bytes += metadata.len();
        }
    });
    DataInfo {
        dir: paths.data.clone(),
        files,
        bytes,
    }
}

/// Copies everything under `paths.data` to `target`, verifies it, writes the
/// location pointer and finally removes the original.
pub fn relocate(
    paths: &AppPaths,
    target: &Path,
    keep_copy: bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<RelocateReport, RelocateError> {
    let target = normalize(target);

    if target.starts_with(&paths.data) || paths.data.starts_with(&target) {
        return Err(RelocateError::NestedTarget);
    }

    std::fs::create_dir_all(&target)
        .map_err(|error| RelocateError::NotWritable(format!("{}: {error}", target.display())))?;

    let mut sources: Vec<PathBuf> = Vec::new();
    walk(&paths.data, &mut |path| {
        if path.is_file() {
            sources.push(path.to_path_buf());
        }
    });

    let total: u64 = sources
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .sum();

    if let Some(free_mb) = free_mb_for(&target) {
        let needed_mb = total / 1_048_576 + 1;
        // 10% margin for filesystem metadata and rounding.
        if needed_mb + needed_mb / 10 > free_mb {
            return Err(RelocateError::NotEnoughSpace { needed_mb, free_mb });
        }
    }

    let mut copied: Vec<PathBuf> = Vec::new();
    let mut done = 0u64;

    for source in &sources {
        let relative = source.strip_prefix(&paths.data).unwrap_or(source);
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let source_hash = copy_and_hash(source, &destination, &mut |chunk| {
            done += chunk;
            progress(done, total);
        })?;

        let destination_hash = crate::download::sha256_file(&destination)?;
        if source_hash != destination_hash {
            // Undo everything copied so far: the user must keep a working
            // installation at the original location.
            for path in &copied {
                std::fs::remove_file(path).ok();
            }
            std::fs::remove_file(&destination).ok();
            return Err(RelocateError::HashMismatch {
                file: relative.to_string_lossy().to_string(),
            });
        }

        copied.push(destination);
    }

    // From here on the copy is verified. Point the application at the new
    // location before removing anything.
    crate::paths::write_location_to(&crate::paths::location_file(), &target)?;

    if !keep_copy {
        for source in &sources {
            std::fs::remove_file(source).ok();
        }
        remove_empty_dirs(&paths.data);
    }

    Ok(RelocateReport {
        target,
        moved_files: copied.len(),
        moved_bytes: total,
        kept_copy: keep_copy,
    })
}

fn normalize(path: &Path) -> PathBuf {
    // Absolute, without a trailing separator, so `starts_with` comparisons are
    // meaningful. Does not resolve symlinks: the user chose this path.
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn copy_and_hash(
    source: &Path,
    destination: &Path,
    on_chunk: &mut dyn FnMut(u64),
) -> Result<String, std::io::Error> {
    let mut reader = std::fs::File::open(source)?;
    let mut writer = std::fs::File::create(destination)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
        on_chunk(read as u64);
    }
    writer.flush()?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn walk(dir: &Path, on_file: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            walk(&path, on_file);
        } else if file_type.is_file() {
            on_file(&path);
        }
    }
}

fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dirs(&path);
            let _ = std::fs::remove_dir(&path);
        }
    }
}

/// Best effort: uses the disk list from the detector, so no extra dependency.
fn free_mb_for(target: &Path) -> Option<u64> {
    let report = crate::detect::detect();
    report
        .disks
        .iter()
        .filter(|disk| target.starts_with(&disk.mount))
        .max_by_key(|disk| disk.mount.len())
        .map(|disk| disk.free_mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{read_location_from, write_location_to};

    fn setup(tag: &str) -> (AppPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!("wd-relocate-{tag}-{}", std::process::id()));
        let app = root.join("app");
        let paths = crate::paths::resolve(&app);
        paths.ensure().unwrap();

        std::fs::write(paths.config_file.clone(), b"{\"schema\":1}").unwrap();
        std::fs::create_dir_all(&paths.models).unwrap();
        std::fs::write(paths.models.join("ggml-small.bin"), b"weights-small").unwrap();
        std::fs::write(paths.models.join("ggml-base.bin"), b"weights-base").unwrap();
        std::fs::write(paths.history_file(), b"{}\n").unwrap();

        (paths, root)
    }

    #[test]
    fn moves_and_verifies_everything() {
        let (paths, root) = setup("move");
        let target = root.join("nuevo-datos");

        let mut progress_values = Vec::new();
        let report = relocate(&paths, &target, false, &mut |done, total| {
            progress_values.push((done, total));
        })
        .unwrap();

        assert_eq!(report.moved_files, 4);
        assert_eq!(report.moved_bytes, 12 + 13 + 12 + 3);
        assert!(!report.kept_copy);
        assert!(target.join("models/ggml-small.bin").exists());
        assert!(target.join("config.json").exists());
        assert!(target.join("history.jsonl").exists());

        // Original removed, but the application directory itself survives.
        assert!(!paths.models.join("ggml-small.bin").exists());
        assert!(paths.root.exists());

        // Progress ends at 100%.
        let last = progress_values.last().copied().unwrap();
        assert_eq!(last.0, last.1);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn keeps_the_original_when_asked() {
        let (paths, root) = setup("keep");
        let target = root.join("copia");

        let report = relocate(&paths, &target, true, &mut |_, _| {}).unwrap();
        assert!(report.kept_copy);
        assert!(paths.models.join("ggml-small.bin").exists());
        assert!(target.join("models/ggml-small.bin").exists());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_a_nested_target() {
        let (paths, root) = setup("nested");
        let error =
            relocate(&paths, &paths.data.join("dentro"), false, &mut |_, _| {}).unwrap_err();
        assert!(matches!(error, RelocateError::NestedTarget));

        let error = relocate(&paths, &root, false, &mut |_, _| {}).unwrap_err();
        assert!(matches!(error, RelocateError::NestedTarget));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reports_data_size() {
        let (paths, root) = setup("info");
        let info = data_info(&paths);
        assert_eq!(info.files, 4);
        assert_eq!(info.bytes, 12 + 13 + 12 + 3);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn writes_and_reads_the_location_pointer() {
        let root = std::env::temp_dir().join(format!("wd-loc-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let pointer = root.join("location.txt");
        let target = root.join("datos");

        write_location_to(&pointer, &target).unwrap();
        assert_eq!(read_location_from(&pointer), Some(target));
        assert_eq!(read_location_from(&root.join("nada.txt")), None);

        std::fs::remove_dir_all(&root).ok();
    }
}
