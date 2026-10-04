//! File name templates shared by batch rename, export and import.
//!
//! A template is literal text with `{token}` placeholders. Rename renders in
//! strict mode, where an unknown token is an error; export and import render
//! leniently and keep unknown tokens as written, as they always have.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::image_processing::ImageMetadata;
use crate::tagging::COLOR_TAG_PREFIX;

pub const TOKENS: &[&str] = &[
    "original_filename",
    "sequence",
    "YYYY",
    "MM",
    "DD",
    "hh",
    "mm",
    "title",
    "author",
    "copyright",
    "comments",
    "rating",
    "stars",
    "camera",
    "lens",
    "iso",
    "focal",
    "folder",
    "label",
    "group",
    "member",
];

const MIN_GROUP_WIDTH: usize = 4;
const MIN_MEMBER_WIDTH: usize = 2;

/// Metadata of one photo, read only when a template asks for it.
pub struct PhotoFacts {
    source_path: std::path::PathBuf,
    sidecar: OnceCell<ImageMetadata>,
    exif: OnceCell<HashMap<String, String>>,
}

impl PhotoFacts {
    pub fn new(source_path: &Path) -> Self {
        PhotoFacts {
            source_path: source_path.to_path_buf(),
            sidecar: OnceCell::new(),
            exif: OnceCell::new(),
        }
    }

    #[cfg(test)]
    pub fn with_values(
        source_path: &Path,
        sidecar: ImageMetadata,
        exif: HashMap<String, String>,
    ) -> Self {
        PhotoFacts {
            source_path: source_path.to_path_buf(),
            sidecar: OnceCell::from(sidecar),
            exif: OnceCell::from(exif),
        }
    }

    fn sidecar(&self) -> &ImageMetadata {
        self.sidecar.get_or_init(|| {
            crate::exif_processing::load_sidecar(&crate::exif_processing::get_primary_sidecar_path(
                &self.source_path,
            ))
        })
    }

    pub fn exif(&self) -> &HashMap<String, String> {
        self.exif.get_or_init(|| {
            if let Some(exif) = self.sidecar().exif.clone() {
                return exif;
            }
            crate::exif_processing::load_sidecar_with_exif(
                &crate::exif_processing::get_primary_sidecar_path(&self.source_path),
                &self.source_path,
            )
            .exif
            .unwrap_or_default()
        })
    }

    pub fn rating(&self) -> u8 {
        crate::exif_processing::resolve_rating(&self.source_path, self.sidecar()).min(5)
    }

    pub fn color_label(&self) -> Option<String> {
        self.sidecar()
            .tags
            .as_ref()?
            .iter()
            .find_map(|tag| tag.strip_prefix(COLOR_TAG_PREFIX))
            .map(str::to_string)
    }

    fn exif_value(&self, keys: &[&str]) -> Option<&str> {
        let exif = self.exif();
        keys.iter()
            .find_map(|k| exif.get(*k))
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
    }
}

pub struct NamingContext<'a> {
    pub source_path: &'a Path,
    pub sequence: usize,
    pub total: usize,
    pub date: DateTime<Utc>,
    /// `(group, member)`, both 1-based. Only batch rename has groups.
    pub group: Option<(usize, usize)>,
    pub group_count: usize,
    /// Size of the largest group, for the width of `{member}`.
    pub member_count: usize,
    pub facts: &'a PhotoFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    UnknownToken(String),
    GroupTokenOutsideRename(String),
    UnclosedBrace,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::UnknownToken(token) => write!(f, "Unknown token {{{}}}", token),
            TemplateError::GroupTokenOutsideRename(token) => {
                write!(f, "{{{}}} is only available when renaming", token)
            }
            TemplateError::UnclosedBrace => write!(f, "A {{ has no matching }}"),
        }
    }
}

/// Make a metadata value safe to use as part of a file name: strip characters
/// that are invalid on Windows/Unix or could escape the target directory,
/// collapse whitespace, trim trailing dots, and cap the length.
pub fn sanitize_filename_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c| c == '.' || c == ' ')
        .chars()
        .take(100)
        .collect()
}

/// Every `{token}` in a template that is not a known token.
pub fn unknown_tokens(template: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        let name = &after[..close];
        if !TOKENS.contains(&name) && !unknown.iter().any(|u| u == name) {
            unknown.push(name.to_string());
        }
        rest = &after[close + 1..];
    }
    unknown
}

pub fn render_strict(template: &str, ctx: &NamingContext) -> Result<String, TemplateError> {
    render(template, ctx, true)
}

pub fn render_lenient(template: &str, ctx: &NamingContext) -> String {
    render(template, ctx, false).unwrap_or_else(|_| template.to_string())
}

fn render(template: &str, ctx: &NamingContext, strict: bool) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            if strict {
                return Err(TemplateError::UnclosedBrace);
            }
            out.push_str(&rest[open..]);
            return Ok(out);
        };
        let name = &after[..close];
        match token_value(name, ctx) {
            Ok(value) => out.push_str(&value),
            Err(e) if strict => return Err(e),
            Err(_) => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn token_value(name: &str, ctx: &NamingContext) -> Result<String, TemplateError> {
    let local_date = ctx.date.with_timezone(&chrono::Local);
    let facts = ctx.facts;
    let text = |keys: &[&str]| {
        facts
            .exif_value(keys)
            .map(sanitize_filename_component)
            .unwrap_or_default()
    };
    let value = match name {
        "original_filename" => ctx
            .source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string(),
        "sequence" => pad(ctx.sequence, ctx.total.to_string().len()),
        "YYYY" => local_date.format("%Y").to_string(),
        "MM" => local_date.format("%m").to_string(),
        "DD" => local_date.format("%d").to_string(),
        "hh" => local_date.format("%H").to_string(),
        "mm" => local_date.format("%M").to_string(),
        "title" => text(&["ImageDescription", "XPTitle"]),
        "author" => text(&["Artist"]),
        "copyright" => text(&["Copyright"]),
        "comments" => text(&["UserComment", "XPComment"]),
        "rating" => facts.rating().to_string(),
        "stars" => format!("{}star", facts.rating()),
        "camera" => text(&["Model"]),
        "lens" => text(&["LensModel"]),
        "iso" => facts
            .exif_value(&[
                "PhotographicSensitivity",
                "ISOSpeedRatings",
                "ISOSpeed",
                "ISO",
            ])
            .and_then(leading_number)
            .map(|n| format_number(n.trunc()))
            .unwrap_or_default(),
        "focal" => facts
            .exif_value(&["FocalLength"])
            .and_then(leading_number)
            .map(|n| format!("{}mm", format_number(n)))
            .unwrap_or_default(),
        "folder" => ctx
            .source_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| sanitize_filename_component(&n.to_string_lossy()))
            .unwrap_or_default(),
        "label" => facts
            .color_label()
            .map(|l| sanitize_filename_component(&l))
            .unwrap_or_default(),
        "group" | "member" => {
            let Some((group, member)) = ctx.group else {
                return Err(TemplateError::GroupTokenOutsideRename(name.to_string()));
            };
            if name == "group" {
                pad(
                    group,
                    MIN_GROUP_WIDTH.max(ctx.group_count.to_string().len()),
                )
            } else {
                pad(
                    member,
                    MIN_MEMBER_WIDTH.max(ctx.member_count.to_string().len()),
                )
            }
        }
        _ => return Err(TemplateError::UnknownToken(name.to_string())),
    };
    Ok(value)
}

fn pad(value: usize, width: usize) -> String {
    format!("{:0width$}", value, width = width.max(1))
}

fn leading_number(value: &str) -> Option<f64> {
    let start = value.find(|c: char| c.is_ascii_digit())?;
    let number: String = value[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    number.trim_end_matches('.').parse().ok()
}

fn format_number(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{:.1}", rounded)
    }
}

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks a rendered name (without extension) before any file is touched.
pub fn validate_stem(stem: &str) -> Result<(), String> {
    if stem.trim().is_empty() {
        return Err("The new name is empty".to_string());
    }
    if stem == "." || stem == ".." {
        return Err(format!("\"{}\" is not a valid file name", stem));
    }
    if let Some(c) = stem.chars().find(|c| {
        matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
    }) {
        return Err(format!(
            "\"{}\" contains {:?}, which is not allowed in file names",
            stem, c
        ));
    }
    if stem.ends_with(' ') || stem.starts_with(' ') {
        return Err(format!("\"{}\" starts or ends with a space", stem));
    }
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(stem))
    {
        return Err(format!("\"{}\" is a reserved name on Windows", stem));
    }
    if stem.len() > 200 {
        return Err(format!(
            "\"{}…\" is too long",
            stem.chars().take(40).collect::<String>()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn facts(rating: u8, tags: &[&str], exif: &[(&str, &str)]) -> PhotoFacts {
        let sidecar = ImageMetadata {
            rating,
            rating_is_explicit: true,
            tags: Some(tags.iter().map(|t| t.to_string()).collect()),
            ..ImageMetadata::default()
        };
        let exif = exif
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        PhotoFacts::with_values(Path::new("/trips/Iceland 2025/IMG_0042.ARW"), sidecar, exif)
    }

    fn render_with(
        template: &str,
        facts: &PhotoFacts,
        group: Option<(usize, usize)>,
    ) -> Result<String, TemplateError> {
        let path = PathBuf::from("/trips/Iceland 2025/IMG_0042.ARW");
        let date = chrono::Local
            .with_ymd_and_hms(2025, 10, 9, 14, 5, 33)
            .unwrap()
            .with_timezone(&Utc);
        render_strict(
            template,
            &NamingContext {
                source_path: &path,
                sequence: 7,
                total: 120,
                date,
                group,
                group_count: 3,
                member_count: 12,
                facts,
            },
        )
    }

    fn sample() -> PhotoFacts {
        facts(
            3,
            &["user:trip", "color:red"],
            &[
                ("Model", "ILCE-7M4"),
                ("LensModel", "FE 24-70mm F2.8 GM II"),
                ("PhotographicSensitivity", "400"),
                ("FocalLength", "35"),
                ("ImageDescription", "Glacier: lagoon/ice"),
                ("Artist", "Jane Doe"),
                ("Copyright", "© 2025 Jane"),
                ("UserComment", "first light"),
            ],
        )
    }

    #[test]
    fn renders_every_token() {
        let f = sample();
        let cases = [
            ("{original_filename}", "IMG_0042"),
            ("{sequence}", "007"),
            ("{YYYY}{MM}{DD}-{hh}{mm}", "20251009-1405"),
            ("{title}", "Glacier lagoon ice"),
            ("{author}", "Jane Doe"),
            ("{copyright}", "© 2025 Jane"),
            ("{comments}", "first light"),
            ("{rating}", "3"),
            ("{stars}", "3star"),
            ("{camera}", "ILCE-7M4"),
            ("{lens}", "FE 24-70mm F2.8 GM II"),
            ("{iso}", "400"),
            ("{focal}", "35mm"),
            ("{folder}", "Iceland 2025"),
            ("{label}", "red"),
            ("{group}-{member}", "0002-05"),
        ];
        for (template, expected) in cases {
            assert_eq!(
                render_with(template, &f, Some((2, 5))).unwrap(),
                expected,
                "{}",
                template
            );
        }
        let all: String = TOKENS.iter().map(|t| format!("{{{}}}", t)).collect();
        assert!(render_with(&all, &f, Some((1, 1))).is_ok());
    }

    #[test]
    fn missing_metadata_renders_empty_and_unrated_is_zero_stars() {
        let f = facts(0, &[], &[]);
        assert_eq!(
            render_with("a{camera}{lens}{iso}{focal}{label}{title}b", &f, None).unwrap(),
            "ab"
        );
        assert_eq!(
            render_with("{rating}_{stars}", &f, None).unwrap(),
            "0_0star"
        );
    }

    #[test]
    fn number_formats() {
        let f = facts(
            0,
            &[],
            &[("FocalLength", "24.5 mm"), ("ISOSpeed", "ISO 3200")],
        );
        assert_eq!(
            render_with("{focal}_{iso}", &f, None).unwrap(),
            "24.5mm_3200"
        );
        let f = facts(0, &[], &[("FocalLength", "50.0")]);
        assert_eq!(render_with("{focal}", &f, None).unwrap(), "50mm");
    }

    #[test]
    fn unknown_tokens_are_errors_in_strict_mode_and_literal_otherwise() {
        let f = sample();
        assert_eq!(
            render_with("{sequense}", &f, None),
            Err(TemplateError::UnknownToken("sequense".into()))
        );
        assert_eq!(
            render_with("a{b", &f, None),
            Err(TemplateError::UnclosedBrace)
        );
        assert_eq!(
            render_with("{group}", &f, None),
            Err(TemplateError::GroupTokenOutsideRename("group".into()))
        );
        let path = PathBuf::from("/x/IMG.jpg");
        let ctx = NamingContext {
            source_path: &path,
            sequence: 1,
            total: 1,
            date: Utc::now(),
            group: None,
            group_count: 0,
            member_count: 0,
            facts: &f,
        };
        assert_eq!(
            render_lenient("{original_filename}_{dcp_title}", &ctx),
            "IMG_{dcp_title}"
        );
        assert_eq!(render_lenient("{original_filename}_{", &ctx), "IMG_{");
        assert_eq!(unknown_tokens("{a}{sequence}{b}{a}"), vec!["a", "b"]);
    }

    #[test]
    fn metadata_values_cannot_escape_the_folder() {
        let f = facts(
            0,
            &[],
            &[("Model", "../../etc/passwd"), ("Artist", "a\\b:c*d?\u{0}e")],
        );
        let name = render_with("{camera}_{author}", &f, None).unwrap();
        assert!(validate_stem(&name).is_ok(), "{}", name);
        assert_eq!(name, "etc passwd_a b c d e");
    }

    #[test]
    fn validates_rendered_names() {
        assert!(validate_stem("trip_001").is_ok());
        assert!(validate_stem("").is_err());
        assert!(validate_stem("  ").is_err());
        assert!(validate_stem("..").is_err());
        assert!(validate_stem("a/b").is_err());
        assert!(validate_stem("a:b").is_err());
        assert!(validate_stem("trailing ").is_err());
        assert!(validate_stem("con").is_err());
        assert!(validate_stem(&"x".repeat(201)).is_err());
    }
}
