//! Archive extraction with path traversal protection.
//!
//! Backend bundles are `.zip` on Windows and `.tar.gz` on Linux. Every entry
//! is checked to stay inside the destination directory (zip-slip).

use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::manifest::ManifestFile;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("archivo comprimido inválido: {0}")]
    Archive(String),
    #[error("entrada insegura en el archivo: {0}")]
    UnsafeEntry(String),
    #[error("formato no soportado: {0}")]
    Unsupported(String),
}

impl From<zip::result::ZipError> for ExtractError {
    fn from(error: zip::result::ZipError) -> Self {
        ExtractError::Archive(error.to_string())
    }
}

/// Extracts `archive` into `dest`, creating it if needed.
pub fn extract(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    std::fs::create_dir_all(dest)?;
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else {
        Err(ExtractError::Unsupported(name))
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        // `enclosed_name` rejects absolute paths and `..` components.
        let safe_name = entry
            .enclosed_name()
            .ok_or_else(|| ExtractError::UnsafeEntry(entry.name().to_string()))?
            .to_path_buf();
        let out_path = dest.join(safe_name);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;
        out.flush()?;
        apply_mode(&out_path, entry.unix_mode());
    }

    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), ExtractError> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(true);

    for entry in archive
        .entries()
        .map_err(|e| ExtractError::Archive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| ExtractError::Archive(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| ExtractError::Archive(e.to_string()))?
            .to_path_buf();
        if !is_safe_relative(&path) {
            return Err(ExtractError::UnsafeEntry(path.display().to_string()));
        }
        let out_path = dest.join(&path);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // `unpack` writes the entry and preserves the mode.
        entry.unpack(&out_path)?;
    }

    Ok(())
}

fn apply_mode(path: &Path, mode: Option<u32>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = mode {
            let permissions = std::fs::Permissions::from_mode(mode & 0o777);
            std::fs::set_permissions(path, permissions).ok();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

fn is_safe_relative(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Walks `dir` and returns every file as a manifest entry with paths relative
/// to `base`.
pub fn collect_files(dir: &Path, base: &Path) -> Result<Vec<ManifestFile>, ExtractError> {
    let mut out = Vec::new();
    collect_into(dir, base, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect_into(dir: &Path, base: &Path, out: &mut Vec<ManifestFile>) -> Result<(), ExtractError> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_into(&path, base, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(ManifestFile {
            path: relative,
            sha256: crate::download::sha256_file(&path)?,
            size: metadata.len(),
        });
    }
    Ok(())
}

/// Removes a directory if it exists. Used before re-installing a backend.
pub fn clear_dir(dir: &Path) -> Result<(), ExtractError> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("whisper-desktop-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
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
    fn extracts_tar_gz_and_collects_files() {
        let dir = temp_dir("extract-tar");
        let archive = dir.join("backend.tar.gz");
        build_tar_gz(
            &archive,
            &[("whisper-cli", b"binary"), ("lib/libggml.so", b"lib")],
        );

        let out = dir.join("out");
        extract(&archive, &out).unwrap();
        assert_eq!(std::fs::read(out.join("whisper-cli")).unwrap(), b"binary");
        assert!(out.join("lib/libggml.so").exists());

        let files = collect_files(&out, &out).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.path == "whisper-cli"));
        assert_eq!(
            files.iter().find(|f| f.path == "whisper-cli").unwrap().size,
            6
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_traversal_paths() {
        // `tar::Builder` refuses to create the malicious archive, so the gate
        // itself is tested directly: this is the check every entry must pass.
        assert!(!is_safe_relative(Path::new("../escaped")));
        assert!(!is_safe_relative(Path::new("/etc/passwd")));
        assert!(!is_safe_relative(Path::new("a/../../b")));
        assert!(is_safe_relative(Path::new("whisper-cli")));
        assert!(is_safe_relative(Path::new("lib/libggml.so")));
        assert!(is_safe_relative(Path::new("./bin/ffmpeg")));
    }

    #[test]
    fn unknown_extension_is_rejected() {
        let dir = temp_dir("extract-unknown");
        let archive = dir.join("thing.rar");
        File::create(&archive).unwrap().write_all(b"x").unwrap();
        let error = extract(&archive, &dir.join("out")).unwrap_err();
        assert!(matches!(error, ExtractError::Unsupported(_)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
