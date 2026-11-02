//! Verified downloads.
//!
//! Every byte that ends up on disk is checked against the SHA-256 published in
//! the signed catalogue. A mismatch deletes the file: nothing unverified is
//! ever executed.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("error de red: {0}")]
    Network(String),
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("hash incorrecto en {path}\n  esperado: {expected}\n  obtenido: {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("el servidor respondió {0}")]
    Status(u16),
}

impl From<ureq::Error> for DownloadError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::Status(code, _) => DownloadError::Status(code),
            ureq::Error::Transport(transport) => DownloadError::Network(transport.to_string()),
        }
    }
}

/// Abstraction over "get me this URL". The real implementation speaks HTTP;
/// tests provide local files.
pub trait Fetcher {
    fn fetch(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: &str,
        on_progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), DownloadError>;
}

pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), DownloadError> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        fs::remove_file(path).ok();
        Err(DownloadError::HashMismatch {
            path: path.to_string_lossy().to_string(),
            expected: expected.to_string(),
            actual,
        })
    }
}

/// HTTP implementation with resume support.
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: &str,
        on_progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), DownloadError> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut offset = fs::metadata(dest).map(|meta| meta.len()).unwrap_or(0);
        let mut request = ureq::get(url);
        if offset > 0 {
            request = request.set("Range", &format!("bytes={offset}-"));
        }

        let response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(416, _)) if offset > 0 => {
                // Already complete: fall through to the hash check.
                return verify_sha256(dest, expected_sha256);
            }
            Err(error) => return Err(error.into()),
        };

        // Servers are allowed to ignore Range and send the whole file.
        if offset > 0 && response.status() != 206 {
            offset = 0;
        }

        let content_length = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let total = content_length + offset;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(offset > 0)
            .truncate(offset == 0)
            .open(dest)?;

        let mut reader = response.into_reader();
        let mut buffer = [0u8; 64 * 1024];
        let mut written = offset;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])?;
            written += read as u64;
            on_progress(written, total);
        }
        file.flush()?;

        verify_sha256(dest, expected_sha256)
    }
}

/// Copies from local files. Used by tests and by the offline installer build.
pub struct LocalFetcher {
    pub root: PathBuf,
}

impl Fetcher for LocalFetcher {
    fn fetch(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: &str,
        on_progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), DownloadError> {
        let file_name = url.rsplit('/').next().unwrap_or(url);
        let source = self.root.join(file_name);
        let bytes = fs::read(&source)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dest, &bytes)?;
        on_progress(bytes.len() as u64, bytes.len() as u64);
        verify_sha256(dest, expected_sha256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_content() {
        let path = std::env::temp_dir().join(format!("wd-sha-{}", std::process::id()));
        fs::write(&path, b"hello").unwrap();
        // SHA-256 of "hello"
        assert_eq!(
            sha256_file(&path).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        fs::remove_file(&path).ok();
    }

    #[test]
    fn rejects_and_deletes_corrupt_file() {
        let path = std::env::temp_dir().join(format!("wd-sha-bad-{}", std::process::id()));
        fs::write(&path, b"tampered").unwrap();
        let error = verify_sha256(&path, &"0".repeat(64)).unwrap_err();
        assert!(matches!(error, DownloadError::HashMismatch { .. }));
        assert!(!path.exists(), "el archivo corrupto debe borrarse");
    }

    #[test]
    fn local_fetcher_verifies() {
        let dir = std::env::temp_dir().join(format!("wd-local-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("model.bin");
        fs::write(&source, b"payload").unwrap();
        let expected = sha256_file(&source).unwrap();

        let dest = dir.join("out/model.bin");
        let fetcher = LocalFetcher { root: dir.clone() };
        let mut last = (0, 0);
        fetcher
            .fetch(
                "https://example.com/model.bin",
                &dest,
                &expected,
                &mut |done, total| last = (done, total),
            )
            .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"payload");
        assert_eq!(last, (7, 7));

        fs::remove_dir_all(&dir).ok();
    }
}
