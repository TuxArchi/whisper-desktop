//! Output naming and layout.
//!
//! Users care about where their transcripts land and what they are called.
//! This module is pure: given a pattern and a source file it returns the exact
//! paths that will be written, including collision handling.

use std::path::{Path, PathBuf};

use crate::config::OnCollision;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Txt,
    Srt,
    Vtt,
    Json,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Txt => "txt",
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::Json => "json",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "txt" => Some(Format::Txt),
            "srt" => Some(Format::Srt),
            "vtt" => Some(Format::Vtt),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameContext {
    pub name: String,
    pub model: String,
    pub language: String,
    pub task: String,
    pub date: String,
    pub time: String,
}

/// Replaces `{token}` placeholders. Unknown tokens are left untouched so a
/// typo is visible instead of silently producing an empty name.
pub fn render_pattern(pattern: &str, ctx: &NameContext) -> String {
    let mut out = pattern.to_string();
    for (token, value) in [
        ("{nombre}", &ctx.name),
        ("{modelo}", &ctx.model),
        ("{idioma}", &ctx.language),
        ("{tarea}", &ctx.task),
        ("{fecha}", &ctx.date),
        ("{hora}", &ctx.time),
    ] {
        out = out.replace(token, value);
    }
    sanitize(&out)
}

/// Removes characters that are illegal on Windows and control characters on
/// everything else, collapses whitespace and trims trailing dots.
pub fn sanitize(value: &str) -> String {
    let mut cleaned: String = value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect();

    cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    while cleaned.ends_with('.') || cleaned.ends_with(' ') {
        cleaned.pop();
    }
    if cleaned.is_empty() {
        "transcripcion".to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPlan {
    pub dir: PathBuf,
    pub stem: String,
    /// Every file that will be produced, one per requested format.
    pub files: Vec<PathBuf>,
}

/// Computes the output files. When any target already exists and the policy is
/// `Rename`, a `_1`, `_2`… suffix is appended until the whole set is free.
pub fn plan_outputs(
    output_dir: &Path,
    formats: &[Format],
    pattern: &str,
    ctx: &NameContext,
    on_collision: OnCollision,
) -> OutputPlan {
    let stem = render_pattern(pattern, ctx);
    let formats = if formats.is_empty() {
        vec![Format::Txt]
    } else {
        formats.to_vec()
    };

    let mut candidate = stem.clone();
    let mut attempts = 0;
    if on_collision != OnCollision::Overwrite {
        while attempts < 500 && any_exists(output_dir, &candidate, &formats) {
            attempts += 1;
            candidate = format!("{stem}_{attempts}");
        }
    }

    let files = formats
        .iter()
        .map(|format| output_dir.join(format!("{candidate}.{}", format.extension())))
        .collect();

    OutputPlan {
        dir: output_dir.to_path_buf(),
        stem: candidate,
        files,
    }
}

fn any_exists(dir: &Path, stem: &str, formats: &[Format]) -> bool {
    formats
        .iter()
        .any(|format| dir.join(format!("{stem}.{}", format.extension())).exists())
}

/// `YYYY-MM-DD` and `HHMM` without pulling a date crate.
pub fn timestamp_parts(unix_seconds: u64) -> (String, String) {
    let days = unix_seconds / 86_400;
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{hour:02}{minute:02}"),
    )
}

/// Howard Hinnant's `civil_from_days`, valid for any proleptic Gregorian date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> NameContext {
        NameContext {
            name: "reunion equipo".into(),
            model: "large-v3-turbo".into(),
            language: "es".into(),
            task: "transcripcion".into(),
            date: "2026-11-01".into(),
            time: "1930".into(),
        }
    }

    #[test]
    fn replaces_tokens_and_sanitises() {
        let rendered = render_pattern("{nombre}_{modelo}_{fecha}", &context());
        assert_eq!(rendered, "reunion equipo_large-v3-turbo_2026-11-01");
    }

    #[test]
    fn strips_illegal_characters() {
        assert_eq!(sanitize("a/b:c*d?e\"f<g>h|i"), "a_b_c_d_e_f_g_h_i");
        assert_eq!(sanitize("  nombre...  "), "nombre");
        assert_eq!(sanitize(""), "transcripcion");
    }

    #[test]
    fn unknown_tokens_stay_visible() {
        let rendered = render_pattern("{nombre}_{desconocido}", &context());
        assert_eq!(rendered, "reunion equipo_{desconocido}");
    }

    #[test]
    fn plans_one_file_per_format() {
        let dir = std::env::temp_dir().join(format!("wd-out-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let plan = plan_outputs(
            &dir,
            &[Format::Txt, Format::Srt],
            "{nombre}",
            &context(),
            OnCollision::Rename,
        );
        assert_eq!(plan.stem, "reunion equipo");
        assert_eq!(plan.files.len(), 2);
        assert!(plan.files[1].ends_with("reunion equipo.srt"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renames_instead_of_overwriting() {
        let dir = std::env::temp_dir().join(format!("wd-out-ren-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hola.txt"), b"old").unwrap();

        let plan = plan_outputs(
            &dir,
            &[Format::Txt],
            "{nombre}",
            &NameContext {
                name: "hola".into(),
                ..Default::default()
            },
            OnCollision::Rename,
        );
        assert_eq!(plan.stem, "hola_1");

        let overwrite = plan_outputs(
            &dir,
            &[Format::Txt],
            "{nombre}",
            &NameContext {
                name: "hola".into(),
                ..Default::default()
            },
            OnCollision::Overwrite,
        );
        assert_eq!(overwrite.stem, "hola");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn formats_roundtrip() {
        for format in [Format::Txt, Format::Srt, Format::Vtt, Format::Json] {
            assert_eq!(Format::parse(format.extension()), Some(format));
        }
        assert_eq!(Format::parse("SRT"), Some(Format::Srt));
        assert_eq!(Format::parse("docx"), None);
    }

    #[test]
    fn timestamp_conversion() {
        // 2026-11-01T19:30:00Z
        let (date, time) = timestamp_parts(1_793_561_400);
        assert_eq!(date, "2026-11-01");
        assert_eq!(time, "1930");

        let (epoch_date, _) = timestamp_parts(0);
        assert_eq!(epoch_date, "1970-01-01");
    }
}
