//! Model catalogue: sizes and file naming for whisper.cpp GGML models.

use serde::{Deserialize, Serialize};

/// Precision variant of a model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelVariant {
    #[default]
    #[serde(rename = "fp16")]
    Fp16,
    #[serde(rename = "q5_0")]
    Q5_0,
}

impl ModelVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelVariant::Fp16 => "fp16",
            ModelVariant::Q5_0 => "q5_0",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            ModelVariant::Fp16 => "",
            ModelVariant::Q5_0 => "-q5_0",
        }
    }
}

pub struct ModelSpec {
    pub id: &'static str,
    pub fp16_bytes: u64,
    pub q5_bytes: u64,
    pub vram_mb: u32,
}

/// Approximate sizes of the upstream GGML files. Used for the download
/// estimate shown in the wizard; the authoritative size comes from the
/// signed catalogue.
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny",
        fp16_bytes: 77_691_713,
        q5_bytes: 32_151_155,
        vram_mb: 512,
    },
    ModelSpec {
        id: "base",
        fp16_bytes: 147_951_465,
        q5_bytes: 59_707_625,
        vram_mb: 640,
    },
    ModelSpec {
        id: "small",
        fp16_bytes: 487_601_967,
        q5_bytes: 190_085_487,
        vram_mb: 1024,
    },
    ModelSpec {
        id: "medium",
        fp16_bytes: 1_533_763_059,
        q5_bytes: 539_212_115,
        vram_mb: 2560,
    },
    ModelSpec {
        id: "large-v3-turbo",
        fp16_bytes: 1_624_555_275,
        q5_bytes: 574_000_000,
        vram_mb: 3072,
    },
    ModelSpec {
        id: "large-v3",
        fp16_bytes: 3_095_033_483,
        q5_bytes: 1_100_000_000,
        vram_mb: 5120,
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// Upstream file name, e.g. `ggml-large-v3-turbo-q5_0.bin`.
pub fn file_name(id: &str, variant: ModelVariant) -> String {
    format!("ggml-{}{}.bin", id, variant.suffix())
}

pub fn download_size(id: &str, variant: ModelVariant) -> u64 {
    spec(id)
        .map(|m| match variant {
            ModelVariant::Fp16 => m.fp16_bytes,
            ModelVariant::Q5_0 => m.q5_bytes,
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(file_name("small", ModelVariant::Fp16), "ggml-small.bin");
        assert_eq!(
            file_name("large-v3-turbo", ModelVariant::Q5_0),
            "ggml-large-v3-turbo-q5_0.bin"
        );
    }

    #[test]
    fn catalogue_is_sorted_by_size() {
        let sizes: Vec<u64> = MODELS.iter().map(|m| m.fp16_bytes).collect();
        assert!(sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
