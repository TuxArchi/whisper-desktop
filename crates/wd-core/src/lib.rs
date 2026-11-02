//! Whisper Desktop core: hardware detection, installation planning and the
//! install/verify plumbing.
//!
//! Kept free of GUI dependencies so it can be unit tested on CI runners with
//! no GPU, no WebView2 and no webkit2gtk.

pub mod bench;
pub mod catalog;
pub mod catalog_source;
pub mod cli;
pub mod config;
pub mod decide;
pub mod detect;
pub mod download;
pub mod extract;
pub mod hardware;
pub mod history;
pub mod i18n;
pub mod installer;
pub mod job;
pub mod manifest;
pub mod output;
pub mod paths;
pub mod process;
pub mod relocate;
pub mod system;
pub mod transcriber;
pub mod trust;
pub mod verify;

pub use decide::{plan, BackendId, InstallPlan, ModelVariant};
pub use hardware::{HardwareReport, Reason};
