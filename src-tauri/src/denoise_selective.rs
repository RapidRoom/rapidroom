//! Opt-in rendered-pair experiment. Reuses saved Subject masks; never runs AI.
use image::{DynamicImage, GenericImageView, ImageFormat};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_PIXELS: u64 = 50_000_000;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const USAGE: &str = "rapidroom denoise-selective --pair pair.json --output result.tiff [--amount 0..100] [--invert]";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Asset {
    path: PathBuf,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pair {
    version: u8,
    width: u32,
    height: u32,
    color_space: String,
    mask_coordinate_space: String,
    original: Asset,
    denoised: Asset,
    mask: Asset,
    mask_id: String,
    prepared_rgb16_sha256: String,
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| format!("Cannot open input: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Inputs must be regular files".into());
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err("Input exceeds the prototype's file-size limit".into());
    }
    Ok(bytes)
}

fn asset_bytes(asset: &Asset, parent: &Path) -> Result<Vec<u8>, String> {
    let path = parent.join(&asset.path);
    let bytes = read_bounded(&path, MAX_FILE_BYTES)?;
    if digest(&bytes) != asset.sha256 {
        return Err("Pair asset checksum changed; prepare a new verified pair".into());
    }
    Ok(bytes)
}

fn rgb16(
    bytes: &[u8],
    dimensions: (u32, u32),
) -> Result<image::ImageBuffer<image::Rgb<u16>, Vec<u16>>, String> {
    // Inspect dimensions before allocation, then check the actual decoded image.
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if reader.format() != Some(ImageFormat::Tiff) {
        return Err("The prototype requires 16-bit TIFF pairs".into());
    }
    if reader.into_dimensions().map_err(|e| e.to_string())? != dimensions {
        return Err(
            "Pair dimensions differ; original, denoised view and mask must be aligned".into(),
        );
    }
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    if !matches!(
        image,
        DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_)
    ) {
        return Err("The prototype requires encoded 16-bit RGB TIFF inputs".into());
    }
    if let DynamicImage::ImageRgba16(ref rgba) = image
        && rgba.pixels().any(|pixel| pixel[3] != u16::MAX)
    {
        return Err("Pair inputs must be opaque".into());
    }
    Ok(image.to_rgb16())
}

fn linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn encoded(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn compose(pair_path: &Path, output: &Path, amount: f32, invert: bool) -> Result<Value, String> {
    if !amount.is_finite() || !(0.0..=100.0).contains(&amount) {
        return Err("Denoise amount must be finite and between 0 and 100".into());
    }
    if output
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("tif") && !s.eq_ignore_ascii_case("tiff"))
    {
        return Err("Output must be a new .tif or .tiff file".into());
    }
    if output.exists() || output.symlink_metadata().is_ok() {
        return Err("Output already exists; keeping it".into());
    }
    let sidecar = output.with_file_name(format!(
        "{}.rrdata",
        output.file_name().unwrap().to_string_lossy()
    ));
    if sidecar.exists() || sidecar.symlink_metadata().is_ok() {
        return Err("Output has existing independent sidecar edits; keeping them".into());
    }
    let manifest = read_bounded(pair_path, 1024 * 1024)?;
    let pair: Pair =
        serde_json::from_slice(&manifest).map_err(|e| format!("Invalid pair manifest: {e}"))?;
    if pair.version != 1
        || pair.color_space != "srgb"
        || pair.mask_coordinate_space != "prepared-view"
    {
        return Err(
            "Requires pair version 1, srgb encoding and prepared-view mask coordinates".into(),
        );
    }
    if pair.width == 0
        || pair.height == 0
        || u64::from(pair.width) * u64::from(pair.height) > MAX_PIXELS
    {
        return Err("The prototype supports nonempty pairs up to 50 megapixels".into());
    }
    let parent = pair_path.parent().unwrap_or(Path::new("."));
    let dimensions = (pair.width, pair.height);
    let original_bytes = asset_bytes(&pair.original, parent)?;
    let denoised_bytes = asset_bytes(&pair.denoised, parent)?;
    let mask_bytes = asset_bytes(&pair.mask, parent)?;
    let original = rgb16(&original_bytes, dimensions)?;
    let denoised = rgb16(&denoised_bytes, dimensions)?;
    drop((original_bytes, denoised_bytes));
    let mut prepared = Sha256::new();
    for sample in original.as_raw() {
        prepared.update(sample.to_le_bytes());
    }
    if hex::encode(prepared.finalize()) != pair.prepared_rgb16_sha256 {
        return Err("Original pixels do not match the verified model input".into());
    }
    let metadata: Value =
        serde_json::from_slice(&mask_bytes).map_err(|e| format!("Invalid mask snapshot: {e}"))?;
    let adjustments = metadata.get("adjustments").unwrap_or(&metadata);
    if !adjustments["crop"].is_null()
        || ["rotation", "orientationSteps"]
            .iter()
            .any(|k| adjustments[k].as_f64().unwrap_or(0.0) != 0.0)
        || ["flipHorizontal", "flipVertical"]
            .iter()
            .any(|k| adjustments[k] == true)
        || adjustments["lensCorrectionMode"] == "auto"
        || !adjustments["lensDistortionParams"].is_null()
    {
        return Err("Use an upright, uncropped prepared-view Subject mask; transformed RAW masks need reprojection first".into());
    }
    let definitions = crate::mask_generation::parse_mask_definitions(adjustments);
    let definition = definitions
        .iter()
        .find(|mask| mask.id == pair.mask_id)
        .ok_or("Saved mask ID is missing or unreadable")?;
    if definition.requires_warped_image() {
        return Err("Color/luminance range refinements require the original working image and are outside this prototype".into());
    }
    if !definition.visible {
        return Err("Selected Subject mask is hidden".into());
    }
    let subjects: Vec<_> = definition
        .sub_masks
        .iter()
        .filter(|mask| mask.visible && mask.mask_type == "ai-subject")
        .collect();
    if subjects.is_empty() {
        return Err("Selected mask must contain an existing visible AI Subject raster".into());
    }
    use base64::Engine;
    for subject in subjects {
        if ["rotation", "orientationSteps"]
            .iter()
            .any(|key| subject.parameters[key].as_f64().unwrap_or(0.0) != 0.0)
            || ["flipHorizontal", "flipVertical"]
                .iter()
                .any(|key| subject.parameters[key] == true)
        {
            return Err(
                "Reproject transformed Subject rasters into the prepared view first".into(),
            );
        }
        let data = subject.parameters["maskDataBase64"]
            .as_str()
            .ok_or("Subject raster data is missing")?;
        let payload = data.split_once(',').map_or(data, |(_, payload)| payload);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|e| e.to_string())?;
        let raster = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        if raster.dimensions() != dimensions {
            return Err("Subject raster dimensions do not match the prepared pair".into());
        }
    }
    let mask = crate::mask_generation::generate_mask_bitmap(
        definition,
        pair.width,
        pair.height,
        1.0,
        (0.0, 0.0),
        None,
    )
    .ok_or("Saved Subject mask could not be rendered")?;
    let mut result = original.clone();
    result
        .as_mut()
        .par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let influence = mask.as_raw()[index];
            let influence = if invert { 255 - influence } else { influence };
            let mix = amount / 100.0 * f32::from(influence) / 255.0;
            for (channel, sample) in pixel.iter_mut().enumerate() {
                let denoised = denoised.as_raw()[index * 3 + channel];
                if mix >= 1.0 {
                    *sample = denoised;
                } else if mix > 0.0 {
                    let a = linear(f32::from(*sample) / 65535.0);
                    let b = linear(f32::from(denoised) / 65535.0);
                    *sample = (encoded(a + (b - a) * mix).clamp(0.0, 1.0) * 65535.0).round() as u16;
                }
            }
        });
    let directory = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    DynamicImage::ImageRgb16(result)
        .write_to(staged.as_file_mut(), ImageFormat::Tiff)
        .map_err(|e| e.to_string())?;
    staged.flush().map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    staged
        .persist_noclobber(output)
        .map_err(|e| format!("Cannot publish without replacing a file: {e}"))?;
    Ok(
        json!({"prototype":"selective-denoise-v1", "width":pair.width, "height":pair.height,
        "amount":amount, "invert":invert, "pairSha256":digest(&manifest),
        "originalSha256":pair.original.sha256,"denoisedSha256":pair.denoised.sha256,
        "maskSha256":pair.mask.sha256,"maskId":pair.mask_id,"inferenceRuns":0,
        "output":output,"outputSha256":digest(&read_bounded(output, MAX_FILE_BYTES)?)}),
    )
}

pub fn run_cli(args: &[String]) -> i32 {
    let parsed = (|| {
        let mut pair = None;
        let mut output = None;
        let mut amount = 100.0;
        let mut invert = false;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--pair" if pair.is_none() => pair = Some(PathBuf::from(iter.next().ok_or(USAGE)?)),
                "--output" if output.is_none() => {
                    output = Some(PathBuf::from(iter.next().ok_or(USAGE)?))
                }
                "--amount" => amount = iter.next().ok_or(USAGE)?.parse().map_err(|_| USAGE)?,
                "--invert" => invert = true,
                _ => return Err(format!("Unknown or repeated argument: {arg}. {USAGE}")),
            }
        }
        compose(&pair.ok_or(USAGE)?, &output.ok_or(USAGE)?, amount, invert)
    })();
    match parsed {
        Ok(receipt) => {
            cli_println!("{}", receipt);
            0
        }
        Err(error) => {
            cli_eprintln!("Selective denoise prototype: {error}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn saved_subject_pair_roundtrip_preserves_endpoints_and_refuses_stale_assets() {
        let dir = tempfile::tempdir().unwrap();
        let original = image::ImageBuffer::from_pixel(8, 8, image::Rgb([0u16; 3]));
        let denoised = image::ImageBuffer::from_pixel(8, 8, image::Rgb([u16::MAX; 3]));
        original.save(dir.path().join("original.tiff")).unwrap();
        denoised.save(dir.path().join("denoised.tiff")).unwrap();
        let raster = image::GrayImage::from_fn(8, 8, |x, _| {
            image::Luma([if x < 2 {
                0
            } else if x > 5 {
                255
            } else {
                128
            }])
        });
        let mut png = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(raster)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        // Synthetic mask data exercises the real saved Subject-mask decoder;
        // real model-produced masks are checked separately by native acceptance.
        let mask = json!({"adjustments":{"masks":[{"id":"subject","name":"Subject","visible":true,
            "invert":false,"adjustments":{},"subMasks":[{"id":"saved-subject","type":"ai-subject",
            "visible":true,"invert":false,"mode":"additive","parameters":{
                "maskDataBase64":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(png.into_inner())),
                "startX":0,"startY":0,"endX":8,"endY":8,"feather":0,"grow":0}}]}]}});
        fs::write(
            dir.path().join("mask.json"),
            serde_json::to_vec(&mask).unwrap(),
        )
        .unwrap();
        let asset = |name: &str| json!({"path":name,"sha256":digest(&fs::read(dir.path().join(name)).unwrap())});
        let manifest = json!({"version":1,"width":8,"height":8,"colorSpace":"srgb",
            "maskCoordinateSpace":"prepared-view","original":asset("original.tiff"),
            "denoised":asset("denoised.tiff"),"mask":asset("mask.json"),"maskId":"subject",
            "preparedRgb16Sha256":digest(&vec![0;8*8*3*2])});
        let pair = dir.path().join("pair.json");
        fs::write(&pair, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let before = ["original.tiff", "denoised.tiff", "mask.json", "pair.json"]
            .map(|name| fs::read(dir.path().join(name)).unwrap());
        let zero = dir.path().join("zero.tiff");
        compose(&pair, &zero, 0.0, false).unwrap();
        assert_eq!(image::open(&zero).unwrap().to_rgb16(), original);
        let selected = dir.path().join("selected.tiff");
        let receipt = compose(&pair, &selected, 100.0, false).unwrap();
        assert_eq!(receipt["inferenceRuns"], 0);
        let selected_pixels = image::open(&selected).unwrap().to_rgb16();
        assert_eq!(selected_pixels.get_pixel(0, 4).0, [0; 3]);
        assert_eq!(selected_pixels.get_pixel(7, 4).0, [u16::MAX; 3]);
        let inverted = dir.path().join("background.tiff");
        compose(&pair, &inverted, 100.0, true).unwrap();
        let inverted_pixels = image::open(&inverted).unwrap().to_rgb16();
        assert_eq!(inverted_pixels.get_pixel(0, 4).0, [u16::MAX; 3]);
        assert_eq!(inverted_pixels.get_pixel(7, 4).0, [0; 3]);
        let half = dir.path().join("half.tiff");
        compose(&pair, &half, 50.0, false).unwrap();
        // IEC 61966-2-1 sRGB: encoded 50% linear-light white is ~0.735357,
        // rather than a 0.5 encoded-value average.
        let middle = image::open(&half).unwrap().to_rgb16().get_pixel(7, 4)[0];
        assert!((i32::from(middle) - 48192).abs() <= 1);
        let kept = fs::read(&selected).unwrap();
        assert!(
            compose(&pair, &selected, 20.0, true)
                .unwrap_err()
                .contains("already exists")
        );
        assert_eq!(fs::read(&selected).unwrap(), kept);
        assert_eq!(
            ["original.tiff", "denoised.tiff", "mask.json", "pair.json"]
                .map(|name| fs::read(dir.path().join(name)).unwrap()),
            before
        );
        fs::write(dir.path().join("mask.json"), b"changed same path").unwrap();
        let refused = dir.path().join("stale.tiff");
        assert!(
            compose(&pair, &refused, 50.0, false)
                .unwrap_err()
                .contains("checksum changed")
        );
        assert!(!refused.exists());
    }
}
