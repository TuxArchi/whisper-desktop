//! Where the catalogue comes from.
//!
//! Order of preference: remote (signed) → local cache (signed) → error.
//! Nothing is ever returned without a valid signature.

use std::path::{Path, PathBuf};

use crate::catalog::Catalog;
use crate::trust;

#[derive(Debug, thiserror::Error)]
pub enum CatalogSourceError {
    #[error("no se pudo descargar el catálogo: {0}")]
    Network(String),
    #[error(transparent)]
    Trust(#[from] trust::TrustError),
    #[error("catálogo inválido: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("no hay ningún catálogo disponible")]
    Unavailable,
}

pub trait TextFetcher {
    fn get(&self, url: &str) -> Result<String, String>;
}

/// HTTP implementation. `ureq` is already a dependency for downloads.
pub struct HttpTextFetcher;

impl TextFetcher for HttpTextFetcher {
    fn get(&self, url: &str) -> Result<String, String> {
        let response = ureq::get(url)
            .timeout(std::time::Duration::from_secs(20))
            .call()
            .map_err(|error| error.to_string())?;
        response.into_string().map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub struct LoadedCatalog {
    pub catalog: Catalog,
    /// URL it came from, or `cache`.
    pub origin: String,
}

/// Verifies and parses a catalogue document with an explicit key.
pub fn verify_and_parse_with(
    public_key: &str,
    raw: &str,
    signature: &str,
) -> Result<Catalog, CatalogSourceError> {
    trust::verify_with(public_key, raw.as_bytes(), signature)?;
    Ok(serde_json::from_str(raw)?)
}

/// Verifies with the key embedded in the application.
pub fn verify_and_parse(raw: &str, signature: &str) -> Result<Catalog, CatalogSourceError> {
    verify_and_parse_with(trust::CATALOG_PUBLIC_KEY, raw, signature)
}

/// Tries every URL in order, then the cache.
pub fn load<F: TextFetcher>(
    fetcher: &F,
    urls: &[String],
    cache_dir: &Path,
) -> Result<LoadedCatalog, CatalogSourceError> {
    load_with_key(fetcher, urls, cache_dir, trust::CATALOG_PUBLIC_KEY)
}

pub fn load_with_key<F: TextFetcher>(
    fetcher: &F,
    urls: &[String],
    cache_dir: &Path,
    public_key: &str,
) -> Result<LoadedCatalog, CatalogSourceError> {
    let mut last_error = None;

    for url in urls {
        match fetch_remote(fetcher, url, public_key) {
            Ok((raw, signature, catalog)) => {
                save_cache(cache_dir, &raw, &signature).ok();
                return Ok(LoadedCatalog {
                    catalog,
                    origin: url.clone(),
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(loaded) = load_cache_with_key(cache_dir, public_key) {
        return Ok(loaded);
    }

    Err(last_error.unwrap_or(CatalogSourceError::Unavailable))
}

fn fetch_remote<F: TextFetcher>(
    fetcher: &F,
    url: &str,
    public_key: &str,
) -> Result<(String, String, Catalog), CatalogSourceError> {
    let raw = fetcher.get(url).map_err(CatalogSourceError::Network)?;
    let signature = fetcher
        .get(&format!("{url}.sig"))
        .map_err(CatalogSourceError::Network)?;
    let catalog = verify_and_parse_with(public_key, &raw, &signature)?;
    Ok((raw, signature, catalog))
}

pub fn cache_paths(cache_dir: &Path) -> (PathBuf, PathBuf) {
    (
        cache_dir.join("catalog.json"),
        cache_dir.join("catalog.json.sig"),
    )
}

fn save_cache(cache_dir: &Path, raw: &str, signature: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let (catalog_path, signature_path) = cache_paths(cache_dir);
    std::fs::write(catalog_path, raw)?;
    std::fs::write(signature_path, signature)?;
    Ok(())
}

/// Reads the cache and verifies it again: a tampered cache is discarded just
/// like a tampered download.
pub fn load_cache(cache_dir: &Path) -> Option<LoadedCatalog> {
    load_cache_with_key(cache_dir, trust::CATALOG_PUBLIC_KEY)
}

pub fn load_cache_with_key(cache_dir: &Path, public_key: &str) -> Option<LoadedCatalog> {
    let (catalog_path, signature_path) = cache_paths(cache_dir);
    let raw = std::fs::read_to_string(catalog_path).ok()?;
    let signature = std::fs::read_to_string(signature_path).ok()?;
    let catalog = verify_and_parse_with(public_key, &raw, &signature).ok()?;
    Some(LoadedCatalog {
        catalog,
        origin: "cache".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::TEST_PUBLIC_KEY;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    struct FakeFetcher {
        files: BTreeMap<String, String>,
    }

    impl FakeFetcher {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
            }
        }

        fn serve(mut self, url: &str, body: &str) -> Self {
            self.files.insert(url.to_string(), body.to_string());
            self
        }
    }

    impl TextFetcher for FakeFetcher {
        fn get(&self, url: &str) -> Result<String, String> {
            self.files
                .get(url)
                .cloned()
                .ok_or_else(|| format!("404 {url}"))
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wd-cat-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn signed() -> (String, String) {
        (
            include_str!("../../../tests/fixtures/catalog-signed.json").to_string(),
            include_str!("../../../tests/fixtures/catalog-signed.sig")
                .trim()
                .to_string(),
        )
    }

    #[test]
    fn loads_verifies_and_caches() {
        let dir = temp_dir("load");
        let (raw, sig) = signed();
        let fetcher = FakeFetcher::new()
            .serve("https://example.com/catalog.json", &raw)
            .serve("https://example.com/catalog.json.sig", &sig);

        let loaded = load_with_key(
            &fetcher,
            &["https://example.com/catalog.json".into()],
            &dir,
            TEST_PUBLIC_KEY,
        )
        .unwrap();
        assert_eq!(loaded.origin, "https://example.com/catalog.json");
        assert_eq!(loaded.catalog.backends.len(), 1);

        // Written to the cache and still valid.
        let cached = load_cache_with_key(&dir, TEST_PUBLIC_KEY).expect("cache");
        assert_eq!(cached.origin, "cache");
        assert_eq!(cached.catalog.schema, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn falls_back_to_the_cache_when_the_network_fails() {
        let dir = temp_dir("fallback");
        let (raw, sig) = signed();
        let fetcher = FakeFetcher::new()
            .serve("https://example.com/catalog.json", &raw)
            .serve("https://example.com/catalog.json.sig", &sig);
        load_with_key(
            &fetcher,
            &["https://example.com/catalog.json".into()],
            &dir,
            TEST_PUBLIC_KEY,
        )
        .unwrap();

        // Network down: no URLs served.
        let offline = FakeFetcher::new();
        let loaded = load_with_key(
            &offline,
            &["https://example.com/catalog.json".into()],
            &dir,
            TEST_PUBLIC_KEY,
        )
        .unwrap();
        assert_eq!(loaded.origin, "cache");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_tampered_catalogue_and_does_not_cache_it() {
        let dir = temp_dir("tampered");
        let (raw, sig) = signed();
        let tampered = raw.replace("example.com", "evil.example").replace("e", "E");
        let fetcher = FakeFetcher::new()
            .serve("https://example.com/catalog.json", &tampered)
            .serve("https://example.com/catalog.json.sig", &sig);

        let error = load_with_key(
            &fetcher,
            &["https://example.com/catalog.json".into()],
            &dir,
            TEST_PUBLIC_KEY,
        )
        .unwrap_err();
        assert!(matches!(error, CatalogSourceError::Trust(_)));
        assert!(load_cache_with_key(&dir, TEST_PUBLIC_KEY).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_tampered_cache() {
        let dir = temp_dir("badcache");
        let (raw, sig) = signed();
        let fetcher = FakeFetcher::new()
            .serve("https://example.com/catalog.json", &raw)
            .serve("https://example.com/catalog.json.sig", &sig);
        load_with_key(
            &fetcher,
            &["https://example.com/catalog.json".into()],
            &dir,
            TEST_PUBLIC_KEY,
        )
        .unwrap();

        let (catalog_path, _) = cache_paths(&dir);
        std::fs::write(catalog_path, raw.replace("example.com", "evil.example")).unwrap();

        assert!(
            load_cache_with_key(&dir, TEST_PUBLIC_KEY).is_none(),
            "la caché manipulada se descarta"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
