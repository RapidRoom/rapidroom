use crate::app_settings::ExportPreset;
use crate::export_processing::{
    BorderBasis, BorderOptions, ExportSettings, PadOptions, ResizeMode, ResizeOptions,
    TiffBitDepth, WatermarkAnchor, WatermarkSettings,
};
use crate::output_sharpening::{OutputSharpening, SharpenAmount, SharpenTarget};

pub const RECIPE_ID_PREFIX: &str = "recipe-";
const LAST_USED_ID: &str = "__last_used__";

fn instagram_recipe(
    id: &str,
    name: &str,
    suffix: &str,
    ratio_width: f32,
    ratio_height: f32,
) -> ExportPreset {
    ExportPreset {
        id: format!("{RECIPE_ID_PREFIX}{id}"),
        name: name.to_string(),
        file_format: "jpeg".to_string(),
        jpeg_quality: 90,
        enable_resize: true,
        resize_mode: "width".to_string(),
        resize_value: 1080,
        dont_enlarge: false,
        keep_metadata: true,
        strip_gps: true,
        filename_template: format!("{{original_filename}}_{suffix}"),
        enable_watermark: false,
        watermark_path: None,
        watermark_anchor: Some("bottomRight".to_string()),
        watermark_scale: 10,
        watermark_spacing: 5,
        watermark_opacity: 75,
        export_masks: Some(false),
        preserve_folders: Some(false),
        enable_pad: Some(true),
        pad_ratio_width: Some(ratio_width),
        pad_ratio_height: Some(ratio_height),
        pad_color: Some("#ffffff".to_string()),
        enable_border: Some(false),
        border_basis: Some("longEdge".to_string()),
        border_horizontal_percent: Some(2.0),
        border_vertical_percent: Some(2.0),
        border_color: Some("#ffffff".to_string()),
        last_export_path: None,
        destination_type: Some("customFolder".to_string()),
        subfolder: Some(String::new()),
        tiff_bit_depth: Some(16),
        preserve_timestamps: Some(false),
        output_sharpening: Some(OutputSharpening {
            target: SharpenTarget::Screen,
            amount: SharpenAmount::Low,
        }),
    }
}

// Read-only recipes shipped with the app. Exports are already sRGB; the photo is
// padded to the post's aspect ratio (no cropping) and scaled to Instagram's 1080 px width.
pub fn builtin_export_recipes() -> Vec<ExportPreset> {
    vec![
        instagram_recipe(
            "instagram-portrait",
            "Instagram Portrait 4:5 (1080×1350)",
            "ig_4x5",
            4.0,
            5.0,
        ),
        instagram_recipe(
            "instagram-square",
            "Instagram Square 1:1 (1080×1080)",
            "ig_1x1",
            1.0,
            1.0,
        ),
        instagram_recipe(
            "instagram-landscape",
            "Instagram Landscape 1.91:1 (1080×566)",
            "ig_191x1",
            540.0,
            283.0,
        ),
    ]
}

/// Matches a preset by id or by name (case-insensitive), user presets first.
pub fn find_export_preset(
    user_presets: &[ExportPreset],
    query: &str,
) -> Result<ExportPreset, String> {
    let query = query.trim();
    let recipes = builtin_export_recipes();
    let candidates: Vec<&ExportPreset> = user_presets
        .iter()
        .chain(recipes.iter())
        .filter(|preset| preset.id != LAST_USED_ID)
        .collect();

    candidates
        .iter()
        .find(|preset| preset.id == query)
        .or_else(|| {
            candidates
                .iter()
                .find(|preset| preset.name.eq_ignore_ascii_case(query))
        })
        .map(|preset| (*preset).clone())
        .ok_or_else(|| {
            let names: Vec<String> = candidates
                .iter()
                .map(|preset| format!("\"{}\"", preset.name))
                .collect();
            format!(
                "Unknown export preset '{}'. Available presets: {}",
                query,
                names.join(", ")
            )
        })
}

/// The file extension `export_images` expects for a preset's format.
pub fn preset_output_format(preset: &ExportPreset) -> String {
    match preset.file_format.to_lowercase().as_str() {
        "jpeg" | "jpg" => "jpg".to_string(),
        "tif" | "tiff" => "tiff".to_string(),
        other => other.to_string(),
    }
}

fn parse_resize_mode(mode: &str) -> Result<ResizeMode, String> {
    serde_json::from_value(serde_json::Value::String(mode.to_string()))
        .map_err(|_| format!("Unknown resize mode '{mode}'"))
}

/// Builds the same `ExportSettings` the export panel sends for this preset.
pub fn preset_to_export_settings(preset: &ExportPreset) -> Result<ExportSettings, String> {
    let resize = if preset.enable_resize {
        Some(ResizeOptions {
            mode: parse_resize_mode(&preset.resize_mode)?,
            value: preset.resize_value,
            dont_enlarge: preset.dont_enlarge,
        })
    } else {
        None
    };

    let pad = match (
        preset.enable_pad.unwrap_or(false),
        preset.pad_ratio_width,
        preset.pad_ratio_height,
    ) {
        (true, Some(ratio_width), Some(ratio_height)) => Some(PadOptions {
            ratio_width,
            ratio_height,
            color: preset
                .pad_color
                .clone()
                .unwrap_or_else(|| "#ffffff".to_string()),
        }),
        _ => None,
    };

    let border = match (
        preset.enable_border.unwrap_or(false),
        preset.border_horizontal_percent,
        preset.border_vertical_percent,
    ) {
        (true, Some(horizontal_percent), Some(vertical_percent)) => Some(BorderOptions {
            basis: preset
                .border_basis
                .as_deref()
                .and_then(|basis| {
                    serde_json::from_value::<BorderBasis>(serde_json::Value::String(
                        basis.to_string(),
                    ))
                    .ok()
                })
                .unwrap_or_default(),
            horizontal_percent,
            vertical_percent,
            color: preset
                .border_color
                .clone()
                .unwrap_or_else(|| "#ffffff".to_string()),
        }),
        _ => None,
    };

    let watermark = match (&preset.watermark_path, preset.enable_watermark) {
        (Some(path), true) if !path.is_empty() => {
            let anchor = preset.watermark_anchor.as_deref().unwrap_or("bottomRight");
            Some(WatermarkSettings {
                path: path.clone(),
                anchor: serde_json::from_value::<WatermarkAnchor>(serde_json::Value::String(
                    anchor.to_string(),
                ))
                .map_err(|_| format!("Unknown watermark anchor '{anchor}'"))?,
                scale: preset.watermark_scale as f32,
                spacing: preset.watermark_spacing as f32,
                opacity: preset.watermark_opacity as f32,
            })
        }
        _ => None,
    };

    Ok(ExportSettings {
        jpeg_quality: preset.jpeg_quality,
        tiff_bit_depth: preset
            .tiff_bit_depth
            .map(TiffBitDepth::try_from)
            .transpose()?
            .unwrap_or_default(),
        resize,
        border,
        pad,
        keep_metadata: preset.keep_metadata,
        preserve_timestamps: preset.preserve_timestamps.unwrap_or(false),
        strip_gps: preset.strip_gps,
        filename_template: Some(preset.filename_template.clone()),
        watermark,
        export_masks: preset.export_masks.unwrap_or(false),
        preserve_folders: preset.preserve_folders.unwrap_or(false),
        destination_type: preset.destination_type.clone(),
        subfolder: preset.subfolder.clone(),
        output_sharpening: preset.output_sharpening,
    })
}

#[tauri::command]
pub fn get_export_recipes() -> Vec<ExportPreset> {
    builtin_export_recipes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_settings::default_export_presets;
    use crate::export_processing::compute_export_output_size;

    const CAMERA_SIZES: &[(u32, u32)] = &[
        (6000, 4000),
        (4000, 6000),
        (5472, 3648),
        (4032, 3024),
        (3024, 4032),
        (8256, 5504),
        (7952, 5304),
        (4000, 4000),
        (4800, 6000),
        (1600, 2000),
        (12000, 3000),
        (3000, 9000),
        (640, 480),
    ];

    fn assert_recipe_size(id: &str, expected: (u32, u32), with_border: bool) {
        let mut preset = find_export_preset(&[], id).expect("recipe exists");
        preset.enable_border = Some(with_border);
        let settings = preset_to_export_settings(&preset).unwrap();
        for &(w, h) in CAMERA_SIZES {
            assert_eq!(
                compute_export_output_size(w, h, &settings),
                expected,
                "{id} from {w}x{h} (border {with_border})"
            );
        }
    }

    #[test]
    fn instagram_recipes_produce_exact_sizes() {
        for border in [false, true] {
            assert_recipe_size("recipe-instagram-portrait", (1080, 1350), border);
            assert_recipe_size("recipe-instagram-square", (1080, 1080), border);
            assert_recipe_size("recipe-instagram-landscape", (1080, 566), border);
        }
    }

    #[test]
    fn instagram_recipes_are_srgb_jpeg_with_light_screen_sharpening() {
        for recipe in builtin_export_recipes() {
            assert!(recipe.id.starts_with(RECIPE_ID_PREFIX));
            assert_eq!(preset_output_format(&recipe), "jpg");
            let settings = preset_to_export_settings(&recipe).unwrap();
            assert_eq!(settings.jpeg_quality, 90);
            assert!(settings.strip_gps);
            assert!(settings.border.is_none());
            assert_eq!(
                settings.output_sharpening,
                Some(OutputSharpening {
                    target: SharpenTarget::Screen,
                    amount: SharpenAmount::Low,
                })
            );
        }
    }

    #[test]
    fn default_presets_have_no_output_sharpening() {
        for preset in default_export_presets() {
            assert!(
                preset_to_export_settings(&preset)
                    .unwrap()
                    .output_sharpening
                    .is_none()
            );
        }
    }

    #[test]
    fn finds_presets_by_id_or_name_with_user_presets_first() {
        let mut user = default_export_presets();
        let mut shadow = user[0].clone();
        shadow.id = "user-1".to_string();
        shadow.name = "Instagram Square 1:1 (1080×1080)".to_string();
        shadow.jpeg_quality = 70;
        user.push(shadow);

        assert_eq!(
            find_export_preset(&user, "default-fast").unwrap().id,
            "default-fast"
        );
        assert_eq!(
            find_export_preset(&user, "fast (web)").unwrap().id,
            "default-fast"
        );
        assert_eq!(
            find_export_preset(&user, "instagram square 1:1 (1080×1080)")
                .unwrap()
                .id,
            "user-1"
        );
        assert_eq!(
            find_export_preset(&user, "recipe-instagram-square")
                .unwrap()
                .jpeg_quality,
            90
        );
    }

    #[test]
    fn unknown_or_last_used_preset_lists_the_choices() {
        let mut user = default_export_presets();
        let mut last_used = user[0].clone();
        last_used.id = LAST_USED_ID.to_string();
        last_used.name = LAST_USED_ID.to_string();
        user.push(last_used);

        let error = find_export_preset(&user, LAST_USED_ID).unwrap_err();
        assert!(error.contains("\"High Quality\""));
        assert!(error.contains("\"Instagram Portrait 4:5 (1080×1350)\""));
        assert!(!error.contains(&format!("\"{LAST_USED_ID}\"")));
    }

    #[test]
    fn preset_settings_match_the_export_panel() {
        let mut preset = default_export_presets()[1].clone();
        let settings = preset_to_export_settings(&preset).unwrap();
        let resize = settings.resize.expect("Fast (Web) resizes");
        assert!(matches!(resize.mode, ResizeMode::Width));
        assert_eq!(resize.value, 2048);
        assert!(settings.pad.is_none());
        assert!(settings.watermark.is_none());
        assert_eq!(
            settings.filename_template.as_deref(),
            Some("{original_filename}_web")
        );

        // A watermark without an image is ignored, as in the panel.
        preset.enable_watermark = true;
        assert!(
            preset_to_export_settings(&preset)
                .unwrap()
                .watermark
                .is_none()
        );
        preset.watermark_path = Some("/tmp/logo.png".to_string());
        preset.watermark_anchor = Some("topLeft".to_string());
        let watermark = preset_to_export_settings(&preset)
            .unwrap()
            .watermark
            .unwrap();
        assert!(matches!(watermark.anchor, WatermarkAnchor::TopLeft));

        preset.tiff_bit_depth = Some(8);
        assert_eq!(
            preset_to_export_settings(&preset).unwrap().tiff_bit_depth,
            TiffBitDepth::Eight
        );
        preset.resize_mode = "diagonal".to_string();
        assert!(preset_to_export_settings(&preset).is_err());
    }
}
