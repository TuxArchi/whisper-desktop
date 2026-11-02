//! Vendor and microarchitecture heuristics.
//!
//! We intentionally avoid a giant "PCI id -> exact model" table: board
//! partners reuse ids, and a wrong entry silently produces a wrong backend.
//! Vendors come from the PCI vendor id (reliable); architecture families are
//! derived from the marketing name, which is stable across drivers.

use crate::hardware::{ArchFamily, GpuVendor};

/// Accepts `1002:73df`, `0x1002:0x73df` or a raw vendor id.
pub fn vendor_from_pci_id(pci_id: &str) -> GpuVendor {
    let raw = pci_id
        .split(':')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches("0x")
        .to_ascii_lowercase();
    match raw.as_str() {
        "10de" => GpuVendor::Nvidia,
        "1002" | "1022" => GpuVendor::Amd,
        "8086" => GpuVendor::Intel,
        "106b" => GpuVendor::Apple,
        _ => GpuVendor::Other,
    }
}

pub fn vendor_from_u16(vendor: u16) -> GpuVendor {
    match vendor {
        0x10de => GpuVendor::Nvidia,
        0x1002 | 0x1022 => GpuVendor::Amd,
        0x8086 => GpuVendor::Intel,
        0x106b => GpuVendor::Apple,
        _ => GpuVendor::Other,
    }
}

/// Best-effort architecture family from a human readable device name.
/// Order matters: the most specific patterns are checked first.
pub fn family_from_name(vendor: GpuVendor, name: &str) -> ArchFamily {
    let n = name.to_ascii_lowercase();
    match vendor {
        GpuVendor::Amd => {
            if n.contains("ryzen ai") || n.contains("radeon 80") || n.contains("radeon 89") {
                return ArchFamily::RyzenAi;
            }
            if n.contains("instinct") || n.contains("mi2") || n.contains("mi3") {
                return ArchFamily::Cdna;
            }
            if n.contains("rx 9") {
                return ArchFamily::Rdna4;
            }
            if n.contains("rx 7") {
                return ArchFamily::Rdna3;
            }
            if n.contains("rx 6") {
                return ArchFamily::Rdna2;
            }
            if n.contains("rx 5") && n.contains("700") || n.contains("rx 5") && n.contains("600") {
                return ArchFamily::Rdna1;
            }
            if n.contains("vega") || n.contains("radeon vii") {
                return ArchFamily::Vega;
            }
            if n.contains("rx 4") || n.contains("rx 5") || n.contains("polaris") {
                return ArchFamily::Gcn;
            }
            ArchFamily::Unknown
        }
        GpuVendor::Nvidia => {
            if n.contains("rtx 50") || n.contains("b100") || n.contains("b200") {
                return ArchFamily::Blackwell;
            }
            if n.contains("rtx 40") {
                return ArchFamily::Ada;
            }
            if n.contains("rtx 30") || n.contains("a100") || n.contains("a10") {
                return ArchFamily::Ampere;
            }
            if n.contains("rtx 20") || n.contains("gtx 16") {
                return ArchFamily::Turing;
            }
            ArchFamily::Unknown
        }
        GpuVendor::Intel => {
            if n.contains("arc") {
                return ArchFamily::Arc;
            }
            ArchFamily::Unknown
        }
        _ => ArchFamily::Unknown,
    }
}

/// Integrated AMD graphics report the APU name, not a marketing SKU.
pub fn is_amd_apu(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("ryzen") || n.contains("radeon graphics") || n.contains("vega 8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendors() {
        assert_eq!(vendor_from_pci_id("1002:73df"), GpuVendor::Amd);
        assert_eq!(vendor_from_pci_id("0x10DE:0x2504"), GpuVendor::Nvidia);
        assert_eq!(vendor_from_pci_id("8086:56a0"), GpuVendor::Intel);
        assert_eq!(vendor_from_pci_id("garbage"), GpuVendor::Other);
    }

    #[test]
    fn families() {
        assert_eq!(
            family_from_name(GpuVendor::Amd, "AMD Radeon RX 6700 XT"),
            ArchFamily::Rdna2
        );
        assert_eq!(
            family_from_name(GpuVendor::Amd, "AMD Radeon RX 7900 XTX"),
            ArchFamily::Rdna3
        );
        assert_eq!(
            family_from_name(GpuVendor::Amd, "AMD Radeon RX 9070 XT"),
            ArchFamily::Rdna4
        );
        assert_eq!(
            family_from_name(GpuVendor::Amd, "AMD RYZEN AI MAX+ 395 w/ Radeon 8060S"),
            ArchFamily::RyzenAi
        );
        assert_eq!(
            family_from_name(GpuVendor::Nvidia, "NVIDIA GeForce RTX 4090"),
            ArchFamily::Ada
        );
        assert_eq!(
            family_from_name(GpuVendor::Intel, "Intel Arc A770"),
            ArchFamily::Arc
        );
    }
}
