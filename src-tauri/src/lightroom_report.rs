//! Original CRS import reporting. Values and source paths never enter this report.
use crate::{file_management::Preset, preset_converter::BASIC_MAPPINGS, xmp::PhotoProperty};
use serde::Serialize;

const MAX_SETTINGS: usize = 128;
const MAX_FIELD_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Mapped,
    Approximated,
    Ignored,
}

#[derive(Debug, Clone, Serialize)]
pub struct Setting {
    pub field: String,
    pub outcome: Outcome,
    pub reason: &'static str,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub settings: Vec<Setting>,
    pub mapped: usize,
    pub approximated: usize,
    pub ignored: usize,
    pub omitted: usize,
}

fn bookkeeping(field: &str) -> bool {
    matches!(
        field,
        "Version"
            | "ProcessVersion"
            | "HasSettings"
            | "AlreadyApplied"
            | "Name"
            | "ShortName"
            | "SortName"
            | "Description"
            | "Group"
            | "UUID"
            | "PresetType"
            | "Cluster"
            | "Copyright"
            | "ContactInfo"
            | "CameraModelRestriction"
            | "CameraProfileDigest"
            | "RawFileName"
            | "OriginalRawFileName"
            | "Snapshot"
            | "Look"
            | "LookTable"
            | "LookTableDigest"
            | "ToneCurveName"
            | "ToneCurveName2012"
            | "AsShotTemperature"
            | "AsShotTint"
            | "CropConstrainAspectRatio"
            | "CropWidth"
            | "CropHeight"
            | "CropUnits"
            | "LensProfileName"
            | "LensProfileFilename"
            | "LensProfileDigest"
            | "LensProfileSetup"
            | "LensProfileMatchKeyCameraMake"
            | "LensProfileMatchKeyCameraModel"
            | "LensProfileMatchKeyLensInfo"
            | "LensProfileMatchKeyLensID"
            | "LensProfileMatchKeyLensName"
            | "LensProfileMatchKeyIsRaw"
            | "LensProfileMatchKeySensorFormatFactor"
            | "MaskGroupBasedCorrections"
            | "PaintBasedCorrections"
            | "GradientBasedCorrections"
            | "CircularGradientBasedCorrections"
            | "EnhanceDenoiseAlreadyApplied"
    ) || field.starts_with("Supports")
        || field.starts_with("DefringePurpleHue") && field.ends_with("Default")
}

fn unsupported_reason(field: &str) -> &'static str {
    if field.starts_with("Sharpen") {
        "sharpenCompanion"
    } else if field.starts_with("LuminanceNoise") || field.starts_with("ColorNoise") {
        "noiseCompanion"
    } else if ["Red", "Green", "Blue"].iter().any(|color| {
        field
            .strip_prefix(color)
            .is_some_and(|suffix| matches!(suffix, "Hue" | "Saturation"))
    }) || matches!(field, "ShadowTint" | "CameraCalibration" | "Calibration")
    {
        "calibration"
    } else if field.starts_with("Perspective") || field.starts_with("Upright") {
        "perspective"
    } else if field.starts_with("Defringe") {
        "defringe"
    } else if field.starts_with("Lens") || field.starts_with("Vignette") {
        "manualLens"
    } else if field.starts_with("PostCropVignette") {
        "vignetteCompanion"
    } else if field.starts_with("Retouch") || field.starts_with("Spot") {
        "retouch"
    } else if field.starts_with("Parametric") {
        "parametricCurve"
    } else if field == "CameraProfile" {
        "cameraProfile"
    } else if field.starts_with("Enhance") {
        "aiEnhancement"
    } else if field == "PointColors" {
        "pointColor"
    } else {
        "unsupported"
    }
}

fn target(field: &str) -> Option<String> {
    if let Some((_, key)) = BASIC_MAPPINGS.iter().find(|(name, _)| *name == field) {
        return Some(if *key == "blending" {
            "/colorGrading/blending".into()
        } else {
            format!("/{key}")
        });
    }
    for (prefix, member) in [
        ("HueAdjustment", "hue"),
        ("SaturationAdjustment", "saturation"),
        ("LuminanceAdjustment", "luminance"),
    ] {
        if let Some(color) = field.strip_prefix(prefix) {
            let color = match color {
                "Red" => "reds",
                "Orange" => "oranges",
                "Yellow" => "yellows",
                "Green" => "greens",
                "Aqua" => "aquas",
                "Blue" => "blues",
                "Purple" => "purples",
                "Magenta" => "magentas",
                _ => continue,
            };
            return Some(format!("/hsl/{color}/{member}"));
        }
    }
    if let Some(color) = field.strip_prefix("GrayMixer") {
        let color = match color {
            "Red" => "reds",
            "Orange" => "oranges",
            "Yellow" => "yellows",
            "Green" => "greens",
            "Aqua" => "aquas",
            "Blue" => "blues",
            "Purple" => "purples",
            "Magenta" => "magentas",
            _ => return None,
        };
        return Some(format!("/monochromeMix/{color}"));
    }
    Some(
        match field {
            "Sharpness" => "/sharpness",
            "SharpenEdgeMasking" => "/sharpnessThreshold",
            "Temperature" | "IncrementalTemperature" => "/temperature",
            "Tint" | "IncrementalTint" => "/tint",
            "WhiteBalance" => "/temperature",
            "Exposure" => "/exposure",
            "Brightness" => "/brightness",
            "Contrast" => "/contrast",
            "HighlightRecovery" => "/highlights",
            "FillLight" => "/shadows",
            "Shadows" => "/blacks",
            "Clarity" => "/clarity",
            "ConvertToGrayscale" => "/monochromeEnabled",
            "HasCrop" | "CropLeft" | "CropRight" | "CropTop" | "CropBottom" | "CropAngle" => {
                "/crop"
            }
            "LensProfileEnable" => "/lensCorrectionMode",
            "AutoLateralCA" => "/lensTcaEnabled",
            "LensProfileDistortionScale" => "/lensDistortionAmount",
            "LensProfileVignettingScale" => "/lensVignetteAmount",
            "SplitToningShadowHue" => "/colorGrading/shadows/hue",
            "SplitToningShadowSaturation" => "/colorGrading/shadows/saturation",
            "SplitToningHighlightHue" => "/colorGrading/highlights/hue",
            "SplitToningHighlightSaturation" => "/colorGrading/highlights/saturation",
            "SplitToningBalance" => "/colorGrading/balance",
            "ColorGradeMidtoneHue" => "/colorGrading/midtones/hue",
            "ColorGradeMidtoneSat" => "/colorGrading/midtones/saturation",
            "ColorGradeShadowLum" => "/colorGrading/shadows/luminance",
            "ColorGradeMidtoneLum" => "/colorGrading/midtones/luminance",
            "ColorGradeHighlightLum" => "/colorGrading/highlights/luminance",
            "ColorGradeGlobalHue" => "/colorGrading/global/hue",
            "ColorGradeGlobalSat" => "/colorGrading/global/saturation",
            "ColorGradeGlobalLum" => "/colorGrading/global/luminance",
            "ToneCurve" | "ToneCurvePV2012" => "/curves/luma",
            "ToneCurveRed" | "ToneCurvePV2012Red" => "/curves/red",
            "ToneCurveGreen" | "ToneCurvePV2012Green" => "/curves/green",
            "ToneCurveBlue" | "ToneCurvePV2012Blue" => "/curves/blue",
            _ => return None,
        }
        .into(),
    )
}

fn classify(field: &str, property: &PhotoProperty, preset: &Preset) -> (Outcome, &'static str) {
    let scalar = match property {
        PhotoProperty::Scalar(s) => Some(s.as_str()),
        _ => None,
    };
    if field == "WhiteBalance" && scalar.is_some_and(|s| s.eq_ignore_ascii_case("As Shot")) {
        return (Outcome::Ignored, "asShot");
    }
    if matches!(
        field,
        "HasCrop" | "LensProfileEnable" | "AutoLateralCA" | "ConvertToGrayscale"
    ) && scalar.is_some_and(|s| s.eq_ignore_ascii_case("false") || s == "0")
    {
        return (Outcome::Ignored, "disabled");
    }
    let Some(pointer) = target(field) else {
        return (Outcome::Ignored, unsupported_reason(field));
    };
    if matches!(property, PhotoProperty::Scalar(_))
        && field != "WhiteBalance"
        && !matches!(
            field,
            "HasCrop" | "LensProfileEnable" | "AutoLateralCA" | "ConvertToGrayscale"
        )
        && scalar
            .and_then(|s| s.parse::<f64>().ok())
            .is_none_or(|v| !v.is_finite())
    {
        return (Outcome::Ignored, "invalidValue");
    }
    if preset.adjustments.pointer(&pointer).is_none() {
        return (
            Outcome::Ignored,
            if matches!(
                field,
                "Temperature"
                    | "Tint"
                    | "WhiteBalance"
                    | "IncrementalTemperature"
                    | "IncrementalTint"
            ) {
                "whiteBalanceBaseline"
            } else if pointer == "/crop" {
                "cropUnavailable"
            } else if field.starts_with("LensProfile") || field == "AutoLateralCA" {
                "lensUnavailable"
            } else {
                "notConverted"
            },
        );
    }
    if field == "Sharpness" {
        (Outcome::Approximated, "sharpnessScale")
    } else if field == "SharpenEdgeMasking" {
        (Outcome::Approximated, "sharpenMasking")
    } else if field.starts_with("HueAdjustment") {
        (Outcome::Approximated, "hueScale")
    } else if field.starts_with("GrayMixer") || field == "ConvertToGrayscale" {
        (Outcome::Approximated, "monochrome")
    } else if field.starts_with("ToneCurve") {
        (Outcome::Approximated, "curveInterpolation")
    } else if field.starts_with("LensProfile") || field == "AutoLateralCA" {
        (Outcome::Approximated, "lensfun")
    } else if pointer == "/crop" {
        (Outcome::Mapped, "cropGeometry")
    } else {
        (Outcome::Approximated, "nativeResponse")
    }
}

pub fn settings_report(content: &str, preset: &Preset) -> Report {
    let Ok(properties) = crate::xmp::photo_properties(content, crate::xmp::CRS) else {
        return Report::default();
    };
    let mut fields: Vec<_> = properties.iter().collect();
    fields.sort_by_key(|(field, _)| *field);
    let crop_active = properties.get("HasCrop").is_some_and(|p| {
        matches!(p,
        PhotoProperty::Scalar(s) if s.eq_ignore_ascii_case("true") || s == "1")
    });
    let already_applied = preset.include_crop_transform == Some(true)
        && properties.get("AlreadyApplied").is_some_and(|property| {
            matches!(property, PhotoProperty::Scalar(value)
                if value.eq_ignore_ascii_case("true") || value == "1")
        });
    let mut report = Report::default();
    for (field, property) in fields {
        if bookkeeping(field)
            || (field.starts_with("Crop") && !crop_active)
            || matches!(property, PhotoProperty::Scalar(s) if s.trim().is_empty())
            || matches!(property, PhotoProperty::List(items) if items.is_empty())
        {
            continue;
        }
        let monochrome = properties.get("ConvertToGrayscale").is_some_and(|p| {
            matches!(p,
            PhotoProperty::Scalar(value) if value.eq_ignore_ascii_case("true") || value == "1")
        });
        let as_shot = properties.get("WhiteBalance").is_some_and(|p| {
            matches!(p,
            PhotoProperty::Scalar(value) if value.eq_ignore_ascii_case("As Shot"))
        });
        let newer = match field.as_str() {
            "Exposure" | "Brightness" => Some("Exposure2012"),
            "Contrast" => Some("Contrast2012"),
            "HighlightRecovery" => Some("Highlights2012"),
            "FillLight" => Some("Shadows2012"),
            "Shadows" => Some("Blacks2012"),
            "Clarity" => Some("Clarity2012"),
            _ => None,
        };
        let (outcome, reason) = if already_applied {
            (Outcome::Ignored, "alreadyApplied")
        } else if newer.is_some_and(|key| properties.contains_key(key)) {
            (Outcome::Ignored, "superseded")
        } else if (field.starts_with("GrayMixer") && !monochrome)
            || (monochrome
                && [
                    "HueAdjustment",
                    "SaturationAdjustment",
                    "LuminanceAdjustment",
                ]
                .iter()
                .any(|prefix| field.starts_with(prefix)))
        {
            (Outcome::Ignored, "inactive")
        } else if as_shot && matches!(field.as_str(), "Temperature" | "Tint") {
            (Outcome::Ignored, "asShot")
        } else {
            classify(field, property, preset)
        };
        match outcome {
            Outcome::Mapped => report.mapped += 1,
            Outcome::Approximated => report.approximated += 1,
            Outcome::Ignored => report.ignored += 1,
        }
        if report.settings.len() < MAX_SETTINGS {
            report.settings.push(Setting {
                field: field.chars().take(MAX_FIELD_CHARS).collect(),
                outcome,
                reason,
            });
        } else {
            report.omitted += 1;
        }
    }
    report
}

pub fn warning_lines(report: &Report) -> Vec<String> {
    report
        .settings
        .iter()
        .filter(|s| s.outcome != Outcome::Mapped)
        .map(|s| {
            format!(
                "{}: {} ({})",
                s.field,
                if s.outcome == Outcome::Ignored {
                    "ignored"
                } else {
                    "approximated"
                },
                reason_text(s.reason)
            )
        })
        .chain(
            (report.omitted > 0).then(|| format!("{} additional settings omitted", report.omitted)),
        )
        .collect()
}

pub fn reason_text(reason: &str) -> &'static str {
    match reason {
        "sharpenCompanion" => "Lightroom sharpening radius/detail has no matching operator",
        "noiseCompanion" => "Lightroom noise detail/contrast/smoothness has no matching operator",
        "calibration" => "Camera calibration primaries and shadow tint are not supported",
        "perspective" => "Lightroom perspective/Upright transforms are not imported",
        "defringe" => "Lightroom defringe controls are not supported",
        "manualLens" => "Manual Adobe lens corrections are not imported",
        "vignetteCompanion" => "Lightroom vignette style/highlights has no matching operator",
        "retouch" => "Lightroom retouch/spot removal is not imported",
        "parametricCurve" => "Lightroom parametric curves are not imported",
        "cameraProfile" => "Adobe camera profiles are not available in the engine",
        "aiEnhancement" => {
            "AI enhancement is not recreated; enhanced pixels may already contain it"
        }
        "pointColor" => "Lightroom Point Color is not imported",
        "invalidValue" => "Invalid or non-finite setting value",
        "whiteBalanceBaseline" => {
            "White balance was not converted; source baseline or supported mapping is missing"
        }
        "cropUnavailable" => {
            "Crop was not imported; geometry is unavailable or not included in presets"
        }
        "lensUnavailable" => "Lens correction was not enabled or is not included in presets",
        "notConverted" => "No corresponding adjustment was produced by the converter",
        "sharpnessScale" => "Amount is scaled from 0-150 to 0-100; the sharpening operator differs",
        "sharpenMasking" => "Masking is approximated by the native sharpening threshold",
        "hueScale" => "Hue is scaled by 0.75; engine colour response differs",
        "monochrome" => {
            "Transferred to the native Black & White mode; the film response differs from Lightroom"
        }
        "curveInterpolation" => "Curve points are transferred; engine interpolation differs",
        "lensfun" => {
            "Lensfun substitutes for the Adobe lens profile when a matching lens is available"
        }
        "nativeResponse" => {
            "Transferred to native controls; Lightroom rendering and units may differ"
        }
        "asShot" => "The source requests as-shot white balance; no native override is imported",
        "disabled" => "The source disables this feature; no native adjustment is applied",
        "superseded" => "The newer process-version setting takes precedence",
        "inactive" => "This setting is inactive in the source conversion mode",
        "cropGeometry" => "Converted to native crop geometry",
        _ => "No supported mapping for this CRS setting",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset_converter::{convert_xmp_sidecar_to_preset, convert_xmp_to_preset};
    fn report(fields: &str) -> Report {
        let xmp = format!("<root {fields}/>");
        settings_report(&xmp, &convert_xmp_to_preset(&xmp).unwrap())
    }
    #[test]
    fn unsupported_families_have_individual_reasons_including_explicit_zero() {
        let fields = [
            ("SharpenRadius", "sharpenCompanion"),
            ("SharpenDetail", "sharpenCompanion"),
            ("LuminanceNoiseReductionDetail", "noiseCompanion"),
            ("ColorNoiseReductionSmoothness", "noiseCompanion"),
            ("RedHue", "calibration"),
            ("ShadowTint", "calibration"),
            ("PerspectiveVertical", "perspective"),
            ("UprightVersion", "perspective"),
            ("LensManualDistortionAmount", "manualLens"),
            ("DefringePurpleAmount", "defringe"),
            ("PostCropVignetteStyle", "vignetteCompanion"),
            ("PostCropVignetteHighlightContrast", "vignetteCompanion"),
            ("RetouchInfo", "retouch"),
            ("CameraProfile", "cameraProfile"),
            ("ParametricShadows", "parametricCurve"),
        ];
        for (field, reason) in fields {
            let r = report(&format!("crs:{field}=\"0\""));
            assert_eq!(r.ignored, 1, "{field}");
            assert_eq!(r.settings[0].field, field);
            assert_eq!(r.settings[0].reason, reason);
        }
    }
    #[test]
    fn scope_blank_metadata_and_bookkeeping_do_not_warn() {
        let xmp = r#"<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:a="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:crs="urn:foreign">
          <r:Description r:about="photo" a:Name="preset" a:Version="16" a:ProcessVersion="11" a:SharpenRadius=" " a:HasSettings="True" crs:RedHue="1">
            <a:Look><r:Description a:SharpenDetail="9"/></a:Look>
          </r:Description><r:Description r:about="other" a:RedHue="9"/>
        </r:RDF>"#;
        assert!(
            settings_report(xmp, &convert_xmp_to_preset(xmp).unwrap())
                .settings
                .is_empty()
        );
    }
    #[test]
    fn structured_retouch_and_aliased_qualified_fields_are_reported() {
        let xmp = r#"<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:a="http://ns.adobe.com/camera-raw-settings/1.0/">
        <r:Description r:about=""><a:SharpenRadius r:parseType="Resource"><r:value>1</r:value></a:SharpenRadius>
        <a:RetouchAreas><r:Seq><r:li r:parseType="Resource" a:Exposure2012="8"/></r:Seq></a:RetouchAreas></r:Description></r:RDF>"#;
        let r = settings_report(xmp, &convert_xmp_to_preset(xmp).unwrap());
        assert_eq!(r.ignored, 2);
        assert!(
            r.settings
                .iter()
                .any(|s| s.field == "RetouchAreas" && s.reason == "retouch")
        );
        assert!(!r.settings.iter().any(|s| s.field == "Exposure2012"));
    }
    #[test]
    fn approximations_missing_mappings_and_false_are_honest() {
        let r = report(
            r#"crs:Sharpness="0" crs:HueAdjustmentRed="10" crs:WhiteBalance="As Shot" crs:Temperature="5000" crs:Tint="1" crs:HasCrop="False" crs:Exposure2012="nan""#,
        );
        assert_eq!(r.mapped, 0);
        assert!(
            r.settings
                .iter()
                .any(|s| s.field == "Sharpness" && s.reason == "sharpnessScale")
        );
        assert!(
            r.settings
                .iter()
                .any(|s| s.field == "HueAdjustmentRed" && s.reason == "hueScale")
        );
        assert!(
            r.settings
                .iter()
                .any(|s| s.field == "Temperature" && s.outcome == Outcome::Ignored)
        );
        assert!(
            r.settings
                .iter()
                .any(|s| s.field == "Exposure2012" && s.reason == "invalidValue")
        );
    }
    #[test]
    fn inactive_and_superseded_fields_do_not_claim_another_fields_output() {
        let r = report(
            r#"crs:Exposure="2" crs:Exposure2012="0" crs:GrayMixerRed="10" crs:LuminanceAdjustmentRed="0""#,
        );
        assert!(r.settings.iter().any(|s| s.field == "Exposure"
            && s.reason == "superseded"
            && s.outcome == Outcome::Ignored));
        assert!(r.settings.iter().any(|s| s.field == "GrayMixerRed"
            && s.reason == "inactive"
            && s.outcome == Outcome::Ignored));
        let bw_native = report(r#"crs:ConvertToGrayscale="True" crs:GrayMixerRed="-10""#);
        for field in ["ConvertToGrayscale", "GrayMixerRed"] {
            assert!(
                bw_native
                    .settings
                    .iter()
                    .any(|s| s.field == field && s.outcome != Outcome::Ignored),
                "{field} should be imported into the native B&W mode"
            );
        }
        let bw = report(
            r#"crs:ConvertToGrayscale="True" crs:GrayMixerRed="0" crs:LuminanceAdjustmentRed="10""#,
        );
        assert!(
            bw.settings
                .iter()
                .any(|s| s.field == "LuminanceAdjustmentRed"
                    && s.reason == "inactive"
                    && s.outcome == Outcome::Ignored)
        );
    }

    #[test]
    fn report_is_bounded_stable_and_does_not_mutate_adjustments() {
        let fields: String = (0..300)
            .map(|i| format!("crs:Future{i:03}=\"private value\" "))
            .collect();
        let xmp = format!("<root {fields} crs:Exposure2012=\"1\"/>");
        let preset = convert_xmp_sidecar_to_preset(&xmp).unwrap();
        let before = preset.adjustments.clone();
        let r = settings_report(&xmp, &preset);
        assert_eq!(r.settings.len(), MAX_SETTINGS);
        assert_eq!(r.omitted, 173);
        assert_eq!(preset.adjustments, before);
        let serialized = serde_json::to_string(&r).unwrap();
        assert!(!serialized.contains("private value"));
        assert!(r.settings.windows(2).all(|s| s[0].field < s[1].field));
        assert_eq!(r.ignored, 300);
        assert_eq!(r.approximated, 1);
        let long = "x".repeat(200);
        assert_eq!(
            report(&format!("crs:{long}=\"value\"")).settings[0]
                .field
                .len(),
            MAX_FIELD_CHARS
        );
    }
    #[test]
    fn sidecars_and_presets_report_geometry_only_when_converted() {
        let xmp = r#"<root crs:HasCrop="True" crs:CropLeft="0" crs:CropTop="0" crs:CropRight="1" crs:CropBottom="1" tiff:ImageWidth="100" tiff:ImageLength="100"/>"#;
        let preset = convert_xmp_to_preset(xmp).unwrap();
        assert!(
            settings_report(xmp, &preset)
                .settings
                .iter()
                .all(|s| s.outcome == Outcome::Ignored)
        );
        let sidecar = convert_xmp_sidecar_to_preset(xmp).unwrap();
        assert_eq!(settings_report(xmp, &sidecar).mapped, 5);
    }
    #[test]
    fn already_applied_photo_reports_intentional_suppression_but_presets_still_map() {
        let xmp = r#"<root crs:AlreadyApplied="True" crs:Exposure2012="1.25"/>"#;
        let photo = convert_xmp_sidecar_to_preset(xmp).unwrap();
        let r = settings_report(xmp, &photo);
        assert_eq!(r.settings.len(), 1);
        assert_eq!(r.settings[0].reason, "alreadyApplied");
        assert_eq!(r.settings[0].outcome, Outcome::Ignored);
        let preset = convert_xmp_to_preset(xmp).unwrap();
        assert_ne!(
            settings_report(xmp, &preset).settings[0].reason,
            "alreadyApplied"
        );
    }
}
