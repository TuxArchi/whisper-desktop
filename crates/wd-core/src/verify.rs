//! Post-install verification.
//!
//! The wizard does not trust the plan: it runs a short clip, reads which
//! backend whisper.cpp actually used and how fast it was. If the backend is
//! not the expected one, the caller tries the next fallback.

use crate::decide::{BackendId, InstallPlan};

#[derive(Debug, Clone, PartialEq)]
pub struct VerifyReport {
    pub backend_seen: Option<BackendId>,
    pub rtf: f64,
    pub matches_plan: bool,
}

impl VerifyReport {
    pub fn is_good(&self) -> bool {
        self.matches_plan && self.rtf >= 0.5
    }
}

/// Reads the backend line whisper.cpp writes to stderr, e.g.
/// `whisper_backend_init_gpu: using Vulkan0 backend`.
pub fn parse_backend(log: &str) -> Option<BackendId> {
    for line in log.lines() {
        let line = line.to_ascii_lowercase();
        if !line.contains("backend") {
            continue;
        }
        if line.contains("using cuda") {
            return Some(BackendId::Cuda);
        }
        if line.contains("using vulkan") || line.contains("using vulkan0") {
            return Some(BackendId::Vulkan);
        }
        if line.contains("using rocm") || line.contains("using hip") {
            return Some(BackendId::Rocm);
        }
        if line.contains("using cpu") {
            return Some(BackendId::Cpu);
        }
    }
    None
}

/// Real time factor: audio seconds processed per second of wall clock.
pub fn realtime_factor(audio_seconds: f64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 || audio_seconds <= 0.0 {
        return 0.0;
    }
    audio_seconds / elapsed_seconds
}

pub fn evaluate(
    plan: &InstallPlan,
    log: &str,
    audio_seconds: f64,
    elapsed_seconds: f64,
) -> VerifyReport {
    let backend_seen = parse_backend(log);
    VerifyReport {
        backend_seen,
        rtf: realtime_factor(audio_seconds, elapsed_seconds),
        matches_plan: backend_seen == Some(plan.backend),
    }
}

/// Next backend to try when verification failed, skipping already tried ones.
pub fn next_fallback<'a>(plan: &'a InstallPlan, tried: &[BackendId]) -> Option<&'a BackendId> {
    plan.fallbacks
        .iter()
        .find(|candidate| !tried.contains(candidate) && **candidate != plan.backend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decide::{InstallPlan, ModelVariant};

    fn plan(backend: BackendId, fallbacks: Vec<BackendId>) -> InstallPlan {
        InstallPlan {
            backend,
            fallbacks,
            model: "small".into(),
            variant: ModelVariant::Fp16,
            threads: 6,
            reasons: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn parses_real_backend_lines() {
        assert_eq!(
            parse_backend("whisper_backend_init_gpu: using Vulkan0 backend"),
            Some(BackendId::Vulkan)
        );
        assert_eq!(
            parse_backend("whisper_backend_init_gpu: using CUDA0 backend"),
            Some(BackendId::Cuda)
        );
        assert_eq!(
            parse_backend("[INFO] using CPU backend"),
            Some(BackendId::Cpu)
        );
        assert_eq!(parse_backend("no backend mentioned"), None);
    }

    #[test]
    fn detects_silent_gpu_fallback() {
        let expected = plan(BackendId::Vulkan, vec![BackendId::Cpu]);
        let log = "whisper_backend_init: using CPU backend";
        let report = evaluate(&expected, log, 30.0, 60.0);
        assert_eq!(report.backend_seen, Some(BackendId::Cpu));
        assert!(!report.matches_plan);
        assert!((report.rtf - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn picks_next_untried_fallback() {
        let expected = plan(BackendId::Cuda, vec![BackendId::Vulkan, BackendId::Cpu]);
        assert_eq!(
            next_fallback(&expected, &[BackendId::Cuda]),
            Some(&BackendId::Vulkan)
        );
        assert_eq!(
            next_fallback(&expected, &[BackendId::Cuda, BackendId::Vulkan]),
            Some(&BackendId::Cpu)
        );
        assert_eq!(
            next_fallback(
                &expected,
                &[BackendId::Cuda, BackendId::Vulkan, BackendId::Cpu]
            ),
            None
        );
    }

    #[test]
    fn rejects_zero_elapsed() {
        assert_eq!(realtime_factor(30.0, 0.0), 0.0);
    }
}
