use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::Emitter;

use crate::export_processing::TiffBitDepth;
use crate::output_sharpening::{OutputSharpening, SharpenAmount, SharpenTarget};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditSession {
    pub source: String,
    pub output: String,
    pub format: String,
    pub jpeg_quality: u8,
}

// Format, quality, bit depth and metadata are `None` when not given, so an
// export preset can supply them; explicit flags win over the preset.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadlessExportSession {
    pub source: String,
    pub output: String,
    pub format: Option<String>,
    pub quality: Option<u8>,
    pub tiff_bit_depth: Option<TiffBitDepth>,
    pub keep_metadata: Option<bool>,
    pub adjustments_override: Option<String>,
    pub preset: Option<String>,
    pub sharpening: Option<Option<OutputSharpening>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaunchRequest {
    None,
    OpenFile(String),
    EditSession(ExternalEditSession),
    HeadlessExport(HeadlessExportSession),
    InvalidHeadless(String),
}

static PROCESS_EXIT_CODE: AtomicI32 = AtomicI32::new(0);

pub fn set_process_exit_code(code: i32) {
    PROCESS_EXIT_CODE.store(code, Ordering::SeqCst);
}

pub fn process_exit_code() -> i32 {
    PROCESS_EXIT_CODE.load(Ordering::SeqCst)
}

// `app_handle.exit(code)` arrives as `RunEvent::ExitRequested { code }`; a failure
// recorded with `set_process_exit_code` must win over a plain window-close exit.
pub fn resolve_exit_code(requested: Option<i32>, recorded: i32) -> i32 {
    match requested {
        Some(code) if code != 0 => code,
        _ => recorded,
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPayload {
    pub open_with_file: Option<String>,
    pub edit_session: Option<ExternalEditSession>,
}

// `none`, `screen`, `print`, optionally with `:low`, `:standard` or `:high`.
fn parse_sharpen_arg(value: &str) -> Result<Option<OutputSharpening>, String> {
    let invalid = || {
        format!(
            "Invalid --sharpen value '{}'; expected none, screen or print, optionally followed by :low, :standard or :high.",
            value
        )
    };
    let lower = value.to_lowercase();
    let (target, amount) = lower
        .split_once(':')
        .unwrap_or((lower.as_str(), "standard"));
    let target = match target {
        "none" if amount == "standard" && !lower.contains(':') => return Ok(None),
        "screen" => SharpenTarget::Screen,
        "print" => SharpenTarget::Print,
        _ => return Err(invalid()),
    };
    let amount = match amount {
        "low" => SharpenAmount::Low,
        "standard" => SharpenAmount::Standard,
        "high" => SharpenAmount::High,
        _ => return Err(invalid()),
    };
    Ok(Some(OutputSharpening { target, amount }))
}

pub fn parse_launch_args(args: &[String]) -> LaunchRequest {
    if args.first().map(|s| s.as_str()) == Some("export") {
        let mut iter = args.iter().skip(1);

        let mut source = String::new();
        let mut output = String::new();
        let mut format = None;
        let mut quality = None;
        let mut tiff_bit_depth = None;
        let mut keep_metadata = None;
        let mut adjustments_override = None;
        let mut preset = None;
        let mut sharpening = None;

        if let Some(src) = iter.next()
            && !src.starts_with('-')
        {
            source = src.clone();
        }

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--output" => {
                    if let Some(out) = iter.next() {
                        output = out.clone();
                    }
                }
                "--format" => {
                    if let Some(fmt) = iter.next() {
                        format = Some(fmt.clone());
                    }
                }
                "--quality" => {
                    if let Some(q) = iter.next() {
                        quality = Some(q.parse().unwrap_or(90));
                    }
                }
                "--tiff-bit-depth" => {
                    let Some(value) = iter.next() else {
                        return LaunchRequest::InvalidHeadless(
                            "Missing value for --tiff-bit-depth; expected 8 or 16.".to_string(),
                        );
                    };
                    let Ok(value) = value.parse::<u8>() else {
                        return LaunchRequest::InvalidHeadless(format!(
                            "Invalid TIFF bit depth '{}'; expected 8 or 16.",
                            value
                        ));
                    };
                    let Ok(value) = TiffBitDepth::try_from(value) else {
                        return LaunchRequest::InvalidHeadless(format!(
                            "Invalid TIFF bit depth '{}'; expected 8 or 16.",
                            value
                        ));
                    };
                    tiff_bit_depth = Some(value);
                }
                "--keep-metadata" => keep_metadata = Some(true),
                "--preset" => {
                    let Some(name) = iter.next() else {
                        return LaunchRequest::InvalidHeadless(
                            "Missing value for --preset; expected an export preset name or id."
                                .to_string(),
                        );
                    };
                    preset = Some(name.clone());
                }
                "--sharpen" => {
                    let Some(value) = iter.next() else {
                        return LaunchRequest::InvalidHeadless(
                            "Missing value for --sharpen; expected none, screen or print."
                                .to_string(),
                        );
                    };
                    match parse_sharpen_arg(value) {
                        Ok(value) => sharpening = Some(value),
                        Err(error) => return LaunchRequest::InvalidHeadless(error),
                    }
                }
                "--adjustments" => {
                    if let Some(adj) = iter.next() {
                        adjustments_override = Some(adj.clone());
                    }
                }
                _ => {}
            }
        }

        return LaunchRequest::HeadlessExport(HeadlessExportSession {
            source,
            output,
            format,
            quality,
            tiff_bit_depth,
            keep_metadata,
            adjustments_override,
            preset,
            sharpening,
        });
    }

    let mut edit: Option<String> = None;
    let mut output: Option<String> = None;
    let mut format: Option<String> = None;
    let mut quality: Option<u8> = None;
    let mut plain: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--edit" => edit = iter.next().cloned(),
            "--output" => output = iter.next().cloned(),
            "--format" => format = iter.next().cloned(),
            "--quality" => quality = iter.next().and_then(|q| q.parse().ok()),
            s if !s.starts_with('-') && plain.is_none() => plain = Some(s.to_string()),
            _ => {}
        }
    }

    match (edit, output) {
        (Some(source), Some(output)) => {
            let format = format.unwrap_or_else(|| {
                std::path::Path::new(&output)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_else(|| "jpg".to_string())
            });
            let format = match format.as_str() {
                "tif" => "tiff".to_string(),
                _ => format,
            };
            LaunchRequest::EditSession(ExternalEditSession {
                source,
                output,
                format,
                jpeg_quality: quality.unwrap_or(90),
            })
        }
        (Some(source), None) => LaunchRequest::OpenFile(source),
        _ => match plain {
            Some(path) => LaunchRequest::OpenFile(path),
            None => LaunchRequest::None,
        },
    }
}

fn handle_file_open(app_handle: &tauri::AppHandle, path: PathBuf) {
    if let Some(path_str) = path.to_str()
        && let Err(e) = app_handle.emit("open-with-file", path_str)
    {
        log::error!("Failed to emit open-with-file event: {}", e);
    }
}

pub fn emit_launch_request(app_handle: &tauri::AppHandle, request: LaunchRequest) {
    match request {
        LaunchRequest::EditSession(session) => {
            if let Err(e) = app_handle.emit("external-edit-session", &session) {
                log::error!("Failed to emit external-edit-session event: {}", e);
            }
        }
        LaunchRequest::OpenFile(path) => {
            handle_file_open(app_handle, PathBuf::from(path));
        }
        LaunchRequest::HeadlessExport(_) => {
            cli_println!(
                "Error: Headless export cannot be attached to an already running GUI instance."
            );
        }
        LaunchRequest::InvalidHeadless(error) => {
            log::error!("Invalid headless export request: {}", error);
        }
        LaunchRequest::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> LaunchRequest {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse_launch_args(&owned)
    }

    fn headless(args: &[&str]) -> HeadlessExportSession {
        match parse(args) {
            LaunchRequest::HeadlessExport(session) => session,
            other => panic!("expected a headless export, got {:?}", other),
        }
    }

    #[test]
    fn plain_export_leaves_preset_fields_unset() {
        let session = headless(&["export", "in.dng", "--output", "out"]);
        assert_eq!(session.format, None);
        assert_eq!(session.quality, None);
        assert_eq!(session.tiff_bit_depth, None);
        assert_eq!(session.keep_metadata, None);
        assert_eq!(session.preset, None);
        assert_eq!(session.sharpening, None);
    }

    #[test]
    fn preset_and_explicit_flags_are_parsed() {
        let session = headless(&[
            "export",
            "in.dng",
            "--preset",
            "Instagram Portrait 4:5 (1080×1350)",
            "--output",
            "out",
            "--quality",
            "85",
            "--keep-metadata",
        ]);
        assert_eq!(
            session.preset.as_deref(),
            Some("Instagram Portrait 4:5 (1080×1350)")
        );
        assert_eq!(session.quality, Some(85));
        assert_eq!(session.keep_metadata, Some(true));
        assert_eq!(session.format, None);
    }

    #[test]
    fn sharpen_flag_accepts_targets_and_amounts() {
        let sharpen = |value: &str| headless(&["export", "in.dng", "--sharpen", value]).sharpening;
        assert_eq!(
            sharpen("screen"),
            Some(Some(OutputSharpening {
                target: SharpenTarget::Screen,
                amount: SharpenAmount::Standard,
            }))
        );
        assert_eq!(
            sharpen("Print:High"),
            Some(Some(OutputSharpening {
                target: SharpenTarget::Print,
                amount: SharpenAmount::High,
            }))
        );
        assert_eq!(sharpen("none"), Some(None));
    }

    #[test]
    fn bad_preset_or_sharpen_values_are_rejected() {
        for args in [
            &["export", "in.dng", "--preset"][..],
            &["export", "in.dng", "--sharpen"][..],
            &["export", "in.dng", "--sharpen", "web"][..],
            &["export", "in.dng", "--sharpen", "screen:max"][..],
            &["export", "in.dng", "--sharpen", "none:low"][..],
        ] {
            assert!(
                matches!(parse(args), LaunchRequest::InvalidHeadless(_)),
                "{:?} should be rejected",
                args
            );
        }
    }

    #[test]
    fn failed_exit_request_keeps_its_code() {
        assert_eq!(resolve_exit_code(Some(1), 0), 1);
        assert_eq!(resolve_exit_code(Some(2), 1), 2);
    }

    #[test]
    fn recorded_failure_survives_a_plain_exit() {
        assert_eq!(resolve_exit_code(None, 1), 1);
        assert_eq!(resolve_exit_code(Some(0), 1), 1);
    }

    #[test]
    fn success_and_window_close_exit_zero() {
        assert_eq!(resolve_exit_code(Some(0), 0), 0);
        assert_eq!(resolve_exit_code(None, 0), 0);
    }
}
