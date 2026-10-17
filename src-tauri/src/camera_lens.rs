//! Sony ARW radial tables. Formulae and knot coordinates are reimplemented
//! from darktable's src/iop/lens.cc (_init_coeffs_md_v1) and src/common/exif.cc.
//! Copyright (C) 2019-2026 darktable developers, GPL-3.0-or-later.
//! https://github.com/darktable-org/darktable/blob/master/src/iop/lens.cc
//!
//! These are native radial samples, not Lensfun polynomial coefficients.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

pub const SONY_KNOTS: usize = 16;

// Remain unavailable to the production resolver until paired residuals pass.
const VIGNETTING_VALIDATED: bool = false;
const TCA_VALIDATED: bool = false;

type SourceKey = (PathBuf, u64, SystemTime);
type SourceCache = VecDeque<(SourceKey, Option<SonyCameraData>)>;
static SOURCE_CACHE: once_cell::sync::Lazy<Mutex<SourceCache>> =
    once_cell::sync::Lazy::new(|| Mutex::new(VecDeque::new()));

fn source_data(path: &Path, exif: &HashMap<String, String>) -> Option<SonyCameraData> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("arw"))
        || !exif
            .get("Make")
            .is_some_and(|maker| maker.trim().eq_ignore_ascii_case("sony"))
    {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    let key = (
        path.to_path_buf(),
        metadata.len(),
        metadata.modified().ok()?,
    );
    if let Some((_, data)) = SOURCE_CACHE
        .lock()
        .unwrap()
        .iter()
        .find(|(stamp, _)| *stamp == key)
    {
        return data.clone();
    }
    // Old .rrexif/.rrdata caches may predate retention of Sony's tables.
    // Read the bounded source TIFF tags, not possibly stale sidecar strings.
    let data = crate::file_management::read_file_mapped(path)
        .ok()
        .and_then(|bytes| {
            let mut fields = HashMap::from([("Make".into(), "SONY".into())]);
            for (tag, values) in crate::exif_processing::sony_correction_metadata(&bytes) {
                fields.insert(
                    tag.to_string(),
                    values
                        .iter()
                        .map(i16::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            SonyCameraData::from_exif(path, &fields)
        });
    let mut cache = SOURCE_CACHE.lock().unwrap();
    cache.retain(|(stamp, _)| stamp.0 != key.0);
    if cache.len() == 64 {
        cache.pop_front();
    }
    cache.push_back((key, data.clone()));
    data
}

pub fn resolve_in_adjustments(
    map: &mut Map<String, Value>,
    path: &Path,
    exif: &Option<HashMap<String, String>>,
    lensfun_components: [bool; 3],
) {
    if map.get("lensCorrectionMode").and_then(Value::as_str) != Some("auto")
        || map
            .get("cameraLensCorrectionVersion")
            .and_then(Value::as_u64)
            != Some(1)
    {
        return;
    }
    let data = exif.as_ref().and_then(|exif| source_data(path, exif));
    let selected = SonyCameraData {
        distortion: data.as_ref().and_then(|data| data.distortion),
        vignetting: data
            .as_ref()
            .and_then(|data| data.vignetting)
            .filter(|_| VIGNETTING_VALIDATED),
        tca: data
            .as_ref()
            .and_then(|data| data.tca)
            .filter(|_| TCA_VALIDATED && !lensfun_components[1]),
    };
    let camera_components = [
        selected.distortion.is_some(),
        selected.tca.is_some(),
        selected.vignetting.is_some(),
    ];
    let neutral = json!({"k1":0.0,"k2":0.0,"k3":0.0,"model":0,"radius_scale":1.0,
        "tca_vr":1.0,"tca_vb":1.0,"vig_k1":0.0,"vig_k2":0.0,"vig_k3":0.0});
    let mut params = if lensfun_components.iter().any(|available| *available) {
        map.get("lensDistortionParams").cloned().unwrap_or(neutral)
    } else {
        neutral
    };
    if camera_components.iter().any(|selected| *selected) {
        params["camera_data"] = serde_json::to_value(&selected).expect("finite signed tables");
    }
    let mut components = Map::new();
    for (i, (component, enabled_key, amount_key, fields)) in [
        (
            "distortion",
            "lensDistortionEnabled",
            "lensDistortionAmount",
            &["k1", "k2", "k3", "model", "radius_scale"][..],
        ),
        (
            "tca",
            "lensTcaEnabled",
            "lensTcaAmount",
            &["tca_vr", "tca_vb", "tca_poly3"][..],
        ),
        (
            "vignetting",
            "lensVignetteEnabled",
            "lensVignetteAmount",
            &["vig_k1", "vig_k2", "vig_k3"][..],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let source = if camera_components[i] {
            Some("camera_data")
        } else if lensfun_components[i] {
            Some("lensfun")
        } else {
            None
        };
        let enabled = map
            .get(enabled_key)
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let amount = map.get(amount_key).and_then(Value::as_f64).unwrap_or(100.0);
        let payload = if camera_components[i] {
            json!({"model":"sony_arw_md_v1","samples":params["camera_data"][component],
                "radius":"half_diagonal","centre":"width/2,height/2"})
        } else if lensfun_components[i] {
            Value::Object(
                fields
                    .iter()
                    .filter_map(|key| params.get(*key).map(|v| ((*key).to_string(), v.clone())))
                    .collect(),
            )
        } else {
            Value::Null
        };
        components.insert(
            component.to_string(),
            json!({"available_source":source,
            "applied_source":if enabled && amount > 0.0 {source} else {None},
            "enabled":enabled,"amount":amount,"payload":payload}),
        );
    }
    // The actual renderer payload and its source travel together through all
    // existing lensDistortionParams cache keys, never a guessed coefficient label.
    params["resolution"] = json!({"recipe_version":1,"mode":"auto","components":components});
    map.insert("lensDistortionParams".into(), params);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SonyCameraData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distortion: Option<[i16; SONY_KNOTS]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vignetting: Option<[i16; SONY_KNOTS]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tca: Option<[[i16; SONY_KNOTS]; 2]>,
}

impl SonyCameraData {
    pub fn from_exif(path: &Path, exif: &HashMap<String, String>) -> Option<Self> {
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("arw"))
            || !exif
                .get("Make")
                .is_some_and(|maker| maker.trim().eq_ignore_ascii_case("sony"))
        {
            return None;
        }
        let table = |key: &str, count: usize| -> Option<Vec<i16>> {
            let values: Vec<i16> = exif
                .get(key)?
                .split_whitespace()
                .take(2 * SONY_KNOTS + 2)
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            (values.len() == count * SONY_KNOTS + 1 && values[0] == SONY_KNOTS as i16)
                .then(|| values[1..].to_vec())
        };
        let data = Self {
            distortion: table("SonyDistortionCorrParams", 1)
                .and_then(|v| v.try_into().ok())
                .filter(valid_distortion),
            vignetting: table("SonyVignettingCorrParams", 1).and_then(|v| v.try_into().ok()),
            tca: table("SonyChromaticAberrationCorrParams", 2).map(|v| {
                [
                    v[..SONY_KNOTS].try_into().expect("validated table length"),
                    v[SONY_KNOTS..].try_into().expect("validated table length"),
                ]
            }),
        };
        (data.distortion.is_some() || data.vignetting.is_some() || data.tca.is_some())
            .then_some(data)
    }

    /// Distorted/source radius factor and derivative of the source radius.
    /// Input radius is normalized to the half diagonal of the active image.
    pub fn distortion_factor_and_slope(samples: &[i16; SONY_KNOTS], radius: f64) -> (f64, f64) {
        let (value, derivative) = interpolate(samples, radius);
        let factor = 1.0 + value / 16384.0;
        (factor, factor + radius * derivative / 16384.0)
    }

    /// darktable's metadata-v1 attenuation, before geometric resampling.
    /// It is divided out of the linear source pixels. Shipping this component
    /// additionally requires independent same-shot validation.
    pub fn vignette_attenuation(samples: &[i16; SONY_KNOTS], radius: f64, amount: f64) -> f64 {
        // Interpolate the decoded attenuation samples, not their encoded EVs.
        interpolate_with(radius, |i| {
            2.0_f64.powf(1.0 - 2.0_f64.powf(amount * f64::from(samples[i]) / 8192.0))
        })
        .0
    }
}

fn valid_distortion(samples: &[i16; SONY_KNOTS]) -> bool {
    // A correction must remain a positive, invertible radial map. Each
    // piecewise-linear segment's derivative is linear, so check both ends.
    samples.iter().all(|v| 1.0 + f64::from(*v) / 16384.0 > 0.0)
        && (0..SONY_KNOTS - 1).all(|i| {
            let derivative = (f64::from(samples[i + 1]) - f64::from(samples[i]))
                * (SONY_KNOTS - 1) as f64
                / 16384.0;
            [i, i + 1].into_iter().all(|j| {
                1.0 + f64::from(samples[j]) / 16384.0
                    + (j as f64 + 0.5) / (SONY_KNOTS - 1) as f64 * derivative
                    > 0.0
            })
        })
}

fn interpolate(samples: &[i16; SONY_KNOTS], radius: f64) -> (f64, f64) {
    interpolate_with(radius, |i| f64::from(samples[i]))
}

fn interpolate_with(radius: f64, sample: impl Fn(usize) -> f64) -> (f64, f64) {
    let position = radius * (SONY_KNOTS - 1) as f64 - 0.5;
    if position <= 0.0 {
        return (sample(0), 0.0);
    }
    if position >= (SONY_KNOTS - 1) as f64 {
        return (sample(SONY_KNOTS - 1), 0.0);
    }
    let left = position.floor() as usize;
    let low = sample(left);
    let delta = sample(left + 1) - low;
    (
        low + (position - left as f64) * delta,
        delta * (SONY_KNOTS - 1) as f64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_processing::{
        apply_geometry_warp, get_geometry_params_from_json, is_geometry_identity,
    };
    use image::{DynamicImage, ImageBuffer, Rgb};

    fn raw(table: [i16; SONY_KNOTS]) -> Vec<u8> {
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(0x7037_u16.to_le_bytes());
        bytes.extend(8_u16.to_le_bytes());
        bytes.extend(17_u32.to_le_bytes());
        bytes.extend(26_u32.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
        bytes.extend(16_i16.to_le_bytes());
        for value in table {
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn source_tables_replace_old_metadata_cache_and_bind_real_geometry_cache_payloads() {
        let dir =
            std::env::temp_dir().join(format!("rapidroom-camera-lens-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("source.ARW");
        std::fs::write(&path, raw([1024; SONY_KNOTS])).unwrap();
        // A pre-parser cached EXIF map deliberately contains no correction tags.
        let exif = Some(HashMap::from([("Make".into(), "SONY".into())]));
        let base = json!({"lensCorrectionMode":"auto","cameraLensCorrectionVersion":1});
        let mut first = base.clone();
        crate::file_management::resolve_lens_params_in_adjustments(&mut first, &exif, None, &path);
        assert_eq!(
            first["lensDistortionParams"]["camera_data"]["distortion"][0],
            1024
        );
        assert_eq!(
            first["lensDistortionParams"]["resolution"]["components"]["distortion"]["applied_source"],
            "camera_data"
        );
        assert!(first["lensDistortionParams"]["resolution"]["components"]["vignetting"]["applied_source"].is_null());
        let mut replacement = raw([512; SONY_KNOTS]);
        replacement.push(0); // Changed source length also invalidates coarse filesystem timestamps.
        std::fs::write(&path, replacement).unwrap();
        let mut second = base;
        crate::file_management::resolve_lens_params_in_adjustments(&mut second, &exif, None, &path);
        assert_eq!(
            second["lensDistortionParams"]["camera_data"]["distortion"][0],
            512
        );
        assert_ne!(
            crate::cache_utils::calculate_geometry_hash(&first),
            crate::cache_utils::calculate_geometry_hash(&second)
        );
        assert_ne!(
            crate::cache_utils::calculate_full_job_hash("photo", &first),
            crate::cache_utils::calculate_full_job_hash("photo", &second)
        );
        assert_ne!(
            crate::cache_utils::calculate_thumbnail_base_hash(&first),
            crate::cache_utils::calculate_thumbnail_base_hash(&second)
        );
        assert_ne!(
            crate::export_framing::geometry_signature(&first),
            crate::export_framing::geometry_signature(&second)
        );
        assert_ne!(
            crate::cache_utils::calculate_visual_hash("photo", &first),
            crate::cache_utils::calculate_visual_hash("photo", &second)
        );
        assert_ne!(
            crate::cache_utils::calculate_effects_hash(&first),
            crate::cache_utils::calculate_effects_hash(&second)
        );
        let mut no_version = first.clone();
        no_version["cameraLensCorrectionVersion"] = json!(0);
        assert_ne!(
            crate::cache_utils::calculate_geometry_hash(&first),
            crate::cache_utils::calculate_geometry_hash(&no_version)
        );
        let mut legacy = json!({"lensCorrectionMode":"auto","lensDistortionParams":{"k1":0.123,"radius_scale":1.0}});
        let saved = legacy.clone();
        crate::file_management::resolve_lens_params_in_adjustments(&mut legacy, &exif, None, &path);
        assert_eq!(legacy, saved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn darktable_md_v1_autoscale_keeps_the_output_radius_for_table_lookup() {
        let image = DynamicImage::ImageRgb32F(ImageBuffer::from_fn(16, 12, |x, y| {
            Rgb([x as f32 / 16.0, y as f32 / 12.0, 0.0])
        }));
        let samples: Vec<i16> = (0..SONY_KNOTS).map(|i| i as i16 * 128).collect();
        let adjustments = json!({"lensCorrectionMode":"auto","cameraLensCorrectionVersion":1,
            "lensDistortionParams":{"camera_data":{"distortion":samples}}});
        let corrected = apply_geometry_warp(&image, &adjustments).to_rgb32f();
        // Independent md-v1 reference at (12,6): r=0.4, sample=704,
        // outer-half autoscale=1+1856/16384, source x=8+4*267/285.
        // Scaling the radius before lookup gives a different point.
        let point = corrected.get_pixel(12, 6);
        assert!((f64::from(point[0]) - 0.734_210_526_315_789_5).abs() < 1e-6);
        assert!((point[1] - 0.5).abs() < 1e-6);
        let source_point =
            crate::image_processing::inverse_transform_point(12.0, 6.0, 16.0, 12.0, &adjustments);
        assert!((source_point.0 - 11.747_368_421_052_63).abs() < 1e-9);
        assert!((source_point.1 - 6.0).abs() < 1e-9);
    }

    #[test]
    fn camera_vignette_only_uses_linear_source_gain_and_is_not_skipped() {
        let image = DynamicImage::ImageRgb32F(ImageBuffer::from_pixel(17, 13, Rgb([0.25; 3])));
        let mut adjustments = json!({"lensCorrectionMode":"auto","cameraLensCorrectionVersion":1,
            "lensDistortionParams":{"camera_data":{"vignetting":vec![8192; SONY_KNOTS]}}});
        let params = get_geometry_params_from_json(&adjustments);
        assert!(!is_geometry_identity(&params));
        // darktable md-v1's attenuation for 8192 is 1/2. This checks the
        // actual linear warp path and excludes the old Lensfun 0.8 multiplier.
        let corrected = apply_geometry_warp(&image, &adjustments).to_rgb32f();
        assert!(
            corrected
                .as_raw()
                .iter()
                .all(|value| (*value - 0.5).abs() < 1e-6)
        );
        adjustments["lensVignetteEnabled"] = json!(false);
        assert!(is_geometry_identity(&get_geometry_params_from_json(
            &adjustments
        )));
        assert_eq!(
            apply_geometry_warp(&image, &adjustments).to_rgb32f(),
            image.to_rgb32f()
        );
        // This mathematical fixture does not release camera vignetting for shipping.
    }

    #[test]
    fn lensfun_vignette_only_still_runs_the_actual_warp() {
        let image = DynamicImage::ImageRgb32F(ImageBuffer::from_pixel(17, 13, Rgb([0.25; 3])));
        let adjustments = json!({"lensDistortionParams":{"vig_k1":-0.25}});
        assert!(!is_geometry_identity(&get_geometry_params_from_json(
            &adjustments
        )));
        let corrected = apply_geometry_warp(&image, &adjustments).to_rgb32f();
        assert!(corrected.get_pixel(0, 0)[0] > 0.3);
    }
}
