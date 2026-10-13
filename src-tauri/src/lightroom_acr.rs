//! Read-only, bounded support for ACR v1 grayscale TIFF/JPEG XL mask payloads.
//! Other payloads (including enhanced image data) are never interpreted as masks.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{GrayImage, ImageEncoder, Luma, imageops};
use std::{collections::HashMap, fs::OpenOptions, io::Read, ops::Range, path::Path};

const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_RASTER_PIXELS: u64 = 16_000_000;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const MAX_OUTPUT_PIXELS: u64 = 1_000_000_000;
const MAX_ENCODED: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

fn slice(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    let end = offset.checked_add(size).ok_or("ACR range overflow")?;
    bytes
        .get(offset..end)
        .ok_or_else(|| "Truncated ACR data".into())
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        slice(bytes, offset, 2)?.try_into().unwrap(),
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?.try_into().unwrap(),
    ))
}
fn digest_key(text: &str) -> Result<[u8; 16]> {
    if text.len() != 32 || !text.is_ascii() {
        return Err("Invalid mask digest".into());
    }
    let mut digest = [0; 16];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|_| "Invalid mask digest")?;
    }
    Ok(digest)
}

pub(crate) struct AcrMasks {
    bytes: Vec<u8>,
    entries: HashMap<[u8; 16], Range<usize>>,
    cache: HashMap<String, String>,
    output_pixels: u64,
    encoded_bytes: usize,
}
impl AcrMasks {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "Missing or unreadable ACR sidecar")?;
        let metadata = file.metadata().map_err(|_| "Cannot inspect ACR sidecar")?;
        if !metadata.is_file() || metadata.len() > MAX_FILE as u64 {
            return Err("ACR requires a regular file of at most 64 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read ACR sidecar")?;
        Self::parse(bytes)
    }
    pub(crate) fn parse(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_FILE {
            return Err("ACR exceeds 64 MiB".into());
        }
        if slice(&bytes, 0, 4)? != b"ACR\0" || u32_at(&bytes, 4)? != 1 || u32_at(&bytes, 16)? != 0 {
            return Err("Unsupported ACR header/version".into());
        }
        let format = slice(&bytes, 8, 4)?;
        if !format[..3].iter().all(u8::is_ascii_alphanumeric) || format[3] != 0 {
            return Err("Unsupported ACR raw format header".into());
        }
        let count = u32_at(&bytes, 12)? as usize;
        if count > 4096 {
            return Err("Too many ACR entries".into());
        }
        let directory_end = 20 + count * 32;
        slice(&bytes, 20, count * 32)?;
        let mut entries = HashMap::new();
        let mut ranges = Vec::new();
        for i in 0..count {
            let row = 20 + i * 32;
            let digest: [u8; 16] = slice(&bytes, row, 16)?.try_into().unwrap();
            let size = u32_at(&bytes, row + 16)? as usize;
            let offset = u32_at(&bytes, row + 24)? as usize;
            if size == 0
                || offset < directory_end
                || u32_at(&bytes, row + 20)? != 0
                || u32_at(&bytes, row + 28)? != 0
            {
                return Err("Invalid ACR directory entry".into());
            }
            slice(&bytes, offset, size)?;
            let range = offset..offset + size;
            if entries.insert(digest, range.clone()).is_some() {
                return Err("Duplicate ACR digest".into());
            }
            ranges.push(range);
        }
        ranges.sort_by_key(|r| r.start);
        if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
            return Err("Overlapping ACR payloads".into());
        }
        Ok(Self {
            bytes,
            entries,
            cache: HashMap::new(),
            output_pixels: 0,
            encoded_bytes: 0,
        })
    }
    pub(crate) fn image_parameters(
        &mut self,
        properties: &HashMap<String, String>,
        frame: &crate::lightroom_masks::ImageFrame,
    ) -> Result<serde_json::Value> {
        let get = |key: &str| {
            properties
                .get(key)
                .map(String::as_str)
                .ok_or_else(|| format!("Missing {key}"))
        };
        let digest = get("MaskDigest")?;
        let placement =
            Placement::parse(get("Origin")?, get("FullMaskSize")?, get("WholeImageArea")?)?;
        let key = format!(
            "{digest}:{placement:?}:{}:{}:{}",
            frame.width, frame.height, frame.orientation
        );
        let png = if let Some(png) = self.cache.get(&key) {
            png.clone()
        } else {
            let range = self
                .entries
                .get(&digest_key(digest)?)
                .ok_or("Mask digest not found in ACR sidecar")?
                .clone();
            let raster = decode_mask(&self.bytes[range])?;
            let pixels = image_pixels(frame.width, frame.height)?;
            self.output_pixels = self
                .output_pixels
                .checked_add(pixels)
                .ok_or("Mask import pixel budget exceeded")?;
            if self.output_pixels > MAX_OUTPUT_PIXELS {
                return Err("Mask import pixel budget exceeded".into());
            }
            let image = placement.place(&raster, frame)?;
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::L8,
                )
                .map_err(|_| "Cannot encode mask PNG")?;
            let png = format!("data:image/png;base64,{}", STANDARD.encode(png));
            self.encoded_bytes += png.len();
            if self.encoded_bytes > MAX_ENCODED {
                return Err("Mask import PNG budget exceeded".into());
            }
            self.cache.insert(key, png.clone());
            png
        };
        Ok(
            serde_json::json!({ "maskDataBase64": png, "rotation": frame.rotation, "grow": 0, "feather": 0 }),
        )
    }
}

fn image_pixels(width: f64, height: f64) -> Result<u64> {
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || width.fract() != 0.0
        || height.fract() != 0.0
        || width > 32768.0
        || height > 32768.0
    {
        return Err("Unsupported mask image dimensions".into());
    }
    let pixels = width as u64 * height as u64;
    if pixels > MAX_IMAGE_PIXELS {
        return Err("Mask image exceeds 100 megapixels".into());
    }
    Ok(pixels)
}

fn decode_mask(blob: &[u8]) -> Result<GrayImage> {
    if slice(blob, 0, 16)? != [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
        return Err("Unsupported ACR mask payload header".into());
    }
    let tiff = &blob[16..];
    if slice(tiff, 0, 4)? != b"II*\0" {
        return Err("Unsupported ACR mask TIFF format".into());
    }
    let ifd = u32_at(tiff, 4)? as usize;
    if ifd < 8 {
        return Err("Invalid mask TIFF IFD".into());
    }
    let count = u16_at(tiff, ifd)? as usize;
    if count > 64 {
        return Err("Too many mask TIFF tags".into());
    }
    slice(tiff, ifd + 2, count * 12 + 4)?;
    if u32_at(tiff, ifd + 2 + count * 12)? != 0 {
        return Err("Multiple mask TIFF images are unsupported".into());
    }
    let mut tags = HashMap::new();
    for i in 0..count {
        let entry = ifd + 2 + i * 12;
        let tag = u16_at(tiff, entry)?;
        let kind = u16_at(tiff, entry + 2)?;
        if u32_at(tiff, entry + 4)? != 1 || !matches!(kind, 3 | 4) {
            return Err("Unsupported mask TIFF tag type/count".into());
        }
        let value = if kind == 3 {
            u16_at(tiff, entry + 8)? as u32
        } else {
            u32_at(tiff, entry + 8)?
        };
        if tags.insert(tag, value).is_some() {
            return Err("Duplicate mask TIFF tag".into());
        }
    }
    let required = |tag| {
        tags.get(&tag)
            .copied()
            .ok_or_else(|| format!("Missing mask TIFF tag {tag}"))
    };
    let width = required(256)?;
    let height = required(257)?;
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || width as u64 * height as u64 > MAX_RASTER_PIXELS
    {
        return Err("Mask raster dimensions exceed limits".into());
    }
    if required(258)? != 8
        || required(259)? != 52546
        || required(262)? != 1
        || required(277)? != 1
        || tags.get(&284).is_some_and(|v| *v != 1)
        || tags.get(&274).is_some_and(|v| *v != 1)
        || tags.get(&339).is_some_and(|v| *v != 1)
        || tags.contains_key(&338)
    {
        return Err("Only 8-bit grayscale JPEG XL masks are supported".into());
    }
    let (offset, size) = if tags.contains_key(&324) {
        if tags.contains_key(&273) || required(322)? != width || required(323)? != height {
            return Err("Only a single full-image mask tile is supported".into());
        }
        (required(324)?, required(325)?)
    } else {
        if required(278)? < height {
            return Err("Only a single full-image mask strip is supported".into());
        }
        (required(273)?, required(279)?)
    };
    if size == 0 || size > 16 * 1024 * 1024 || (offset as usize) < ifd + 2 + count * 12 + 4 {
        return Err("Invalid mask codestream range".into());
    }
    let codestream = slice(tiff, offset as usize, size as usize)?;
    let decoder = jxl_oxide::JxlImage::builder()
        .alloc_tracker(jxl_oxide::AllocTracker::with_limit(128 * 1024 * 1024))
        .read(codestream)
        .map_err(|_| "Cannot decode mask JPEG XL")?;
    if decoder.width() != width
        || decoder.height() != height
        || decoder.pixel_format() != jxl_oxide::PixelFormat::Gray
        || decoder.num_loaded_frames() != 1
        || decoder.num_loaded_keyframes() != 1
        || decoder.image_header().metadata.orientation != 1
        || decoder.image_header().metadata.animation.is_some()
    {
        return Err("Mask JPEG XL dimensions/channels/frames do not match TIFF".into());
    }
    // Keep the decoder's original encoding: coverage is data, not linear light.
    let render = decoder
        .render_frame(0)
        .map_err(|_| "Cannot render mask JPEG XL")?;
    let mut stream = render.stream();
    let mut floats = vec![0.0; width as usize * height as usize];
    if stream.write_to_buffer(&mut floats) != floats.len()
        || floats
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
    {
        return Err("Invalid mask coverage samples".into());
    }
    let pixels = floats
        .into_iter()
        .map(|v| (v * 255.0).round() as u8)
        .collect();
    GrayImage::from_raw(width, height, pixels).ok_or_else(|| "Invalid mask raster".into())
}

#[derive(Debug)]
struct Placement {
    origin: [f64; 2],
    full: [f64; 2],
    area: [f64; 4],
}
fn numbers<const N: usize>(text: &str) -> Result<[f64; N]> {
    let values: Vec<f64> = text
        .split(',')
        .map(|value| {
            let (n, d) = value.trim().split_once('/').unwrap_or((value.trim(), "1"));
            let v = n.parse::<f64>().map_err(|_| "Invalid mask geometry")?
                / d.parse::<f64>().map_err(|_| "Invalid mask geometry")?;
            if v.is_finite() {
                Ok(v)
            } else {
                Err("Invalid mask geometry")
            }
        })
        .collect::<std::result::Result<_, _>>()?;
    values
        .try_into()
        .map_err(|_| "Invalid mask geometry tuple length".into())
}
impl Placement {
    fn parse(origin: &str, full: &str, area: &str) -> Result<Self> {
        let origin: [f64; 2] = numbers(origin)?;
        let full: [f64; 2] = numbers(full)?;
        // WholeImageArea is top,left,bottom,right in the full sensor mask grid.
        // It describes the default-cropped RAW, before the user's crop/rotation.
        let area: [f64; 4] = numbers(area)?;
        if full
            .iter()
            .any(|v| *v <= 0.0 || *v > 16384.0 || v.fract() != 0.0)
            || origin.iter().any(|v| *v < 0.0 || v.fract() != 0.0)
            || area[0] < 0.0
            || area[1] < 0.0
            || area[2] > full[1]
            || area[3] > full[0]
            || area[2] <= area[0]
            || area[3] <= area[1]
        {
            return Err("Mask placement outside sensor grid".into());
        }
        Ok(Self { origin, full, area })
    }
    fn place(
        &self,
        raster: &GrayImage,
        frame: &crate::lightroom_masks::ImageFrame,
    ) -> Result<GrayImage> {
        image_pixels(frame.width, frame.height)?;
        if !(1..=8).contains(&frame.orientation)
            || !frame.rotation.is_finite()
            || self.origin[0] + raster.width() as f64 > self.full[0]
            || self.origin[1] + raster.height() as f64 > self.full[1]
        {
            return Err("Invalid mask raster placement/orientation".into());
        }
        let mut full = GrayImage::new(frame.width as u32, frame.height as u32);
        for (x, y, pixel) in full.enumerate_pixels_mut() {
            let sx = self.area[1] + (x as f64 + 0.5) / frame.width * (self.area[3] - self.area[1])
                - self.origin[0]
                - 0.5;
            let sy = self.area[0] + (y as f64 + 0.5) / frame.height * (self.area[2] - self.area[0])
                - self.origin[1]
                - 0.5;
            let (ix, iy) = (sx.floor() as i64, sy.floor() as i64);
            let (fx, fy) = (sx - sx.floor(), sy - sy.floor());
            let sample = |x: i64, y: i64| {
                if x >= 0 && y >= 0 && x < raster.width() as i64 && y < raster.height() as i64 {
                    raster.get_pixel(x as u32, y as u32)[0] as f64
                } else {
                    0.0
                }
            };
            *pixel = Luma([(sample(ix, iy) * (1.0 - fx) * (1.0 - fy)
                + sample(ix + 1, iy) * fx * (1.0 - fy)
                + sample(ix, iy + 1) * (1.0 - fx) * fy
                + sample(ix + 1, iy + 1) * fx * fy)
                .round() as u8]);
        }
        Ok(match frame.orientation {
            1 => full,
            2 => imageops::flip_horizontal(&full),
            3 => imageops::rotate180(&full),
            4 => imageops::flip_vertical(&full),
            5 => imageops::rotate90(&imageops::flip_vertical(&full)),
            6 => imageops::rotate90(&full),
            7 => imageops::rotate90(&imageops::flip_horizontal(&full)),
            8 => imageops::rotate270(&full),
            _ => unreachable!(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightroom_masks::{ImageFrame, import_lightroom_masks_with_acr};
    const DIGEST: &str = "00112233445566778899AABBCCDDEEFF";
    fn fixture() -> Vec<u8> {
        let pixels = [0, 64, 128, 192, 255, 32];
        let jxl = jxl_encoder::LosslessConfig::default()
            .encode(&pixels, 3, 2, jxl_encoder::PixelLayout::Gray8)
            .unwrap();
        let tags = [
            (256, 3),
            (257, 2),
            (258, 8),
            (259, 52546),
            (262, 1),
            (277, 1),
            (284, 1),
            (322, 3),
            (323, 2),
            (324, 134),
            (325, jxl.len() as u32),
        ];
        let mut blob = vec![2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        blob.extend_from_slice(b"II*\0\x08\0\0\0");
        blob.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        for (tag, value) in tags {
            blob.extend_from_slice(&(tag as u16).to_le_bytes());
            blob.extend_from_slice(&4u16.to_le_bytes());
            blob.extend_from_slice(&1u32.to_le_bytes());
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob.extend_from_slice(&0u32.to_le_bytes());
        // IFD has eleven tags: 8 + 2 + 132 + 4 = 146.
        let offset = 146u32;
        blob[16 + 10 + 9 * 12 + 8..16 + 10 + 9 * 12 + 12].copy_from_slice(&offset.to_le_bytes());
        blob.extend_from_slice(&jxl);
        let mut acr = b"ACR\0\x01\0\0\0ARW\0\x01\0\0\0\0\0\0\0".to_vec();
        acr.extend_from_slice(&digest_key(DIGEST).unwrap());
        acr.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        acr.extend_from_slice(&0u32.to_le_bytes());
        acr.extend_from_slice(&52u32.to_le_bytes());
        acr.extend_from_slice(&0u32.to_le_bytes());
        acr.extend_from_slice(&blob);
        acr
    }
    #[test]
    fn synthetic_acr_decodes_exact_coverage_without_colour_remapping() {
        let acr = AcrMasks::parse(fixture()).unwrap();
        let image =
            decode_mask(&acr.bytes[acr.entries[&digest_key(DIGEST).unwrap()].clone()]).unwrap();
        assert_eq!(image.as_raw(), &[0, 64, 128, 192, 255, 32]);
    }
    #[test]
    fn every_truncated_container_is_refused_without_panicking() {
        let data = fixture();
        for len in 0..data.len() {
            assert!(
                AcrMasks::parse(data[..len].to_vec()).is_err(),
                "length {len}"
            );
        }
        for (offset, value) in [
            (4, 2),
            (12, u32::MAX),
            (16, 1),
            (36, u32::MAX),
            (44, 20),
            (48, 1),
        ] {
            let mut bad = data.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(AcrMasks::parse(bad).is_err(), "offset {offset}");
        }
        assert!(digest_key("../malicious/path").is_err());
    }
    #[test]
    fn tiff_layout_and_jxl_metadata_must_agree() {
        let data = fixture();
        for (tag_index, value) in [
            (0, 4),
            (2, 16),
            (3, 1),
            (4, 2),
            (5, 3),
            (7, 1),
            (9, u32::MAX),
            (10, u32::MAX),
        ] {
            let mut bad = data.clone();
            let offset = 52 + 16 + 10 + tag_index * 12 + 8;
            bad[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
            let acr = AcrMasks::parse(bad).unwrap();
            assert!(decode_mask(&acr.bytes[52..]).is_err(), "tag {tag_index}");
        }
    }
    #[test]
    fn placement_removes_sensor_default_crop_and_applies_all_exif_orientations() {
        let raster = GrayImage::from_raw(3, 2, vec![0, 64, 128, 192, 255, 32]).unwrap();
        // Sensor grid 5x4, RAW's default crop occupies rows 1..3, columns 1..4.
        let placement = Placement::parse("1,1", "5,4", "1,1,3,4").unwrap();
        for orientation in 1..=8 {
            let frame = ImageFrame {
                width: 3.0,
                height: 2.0,
                orientation,
                rotation: 17.0,
            };
            let image = placement.place(&raster, &frame).unwrap();
            for (x, y, pixel) in raster.enumerate_pixels() {
                let (u, v) = crate::preset_converter::orient_normalized_point(
                    (x as f64 + 0.5) / 3.0,
                    (y as f64 + 0.5) / 2.0,
                    orientation,
                );
                assert_eq!(
                    image.get_pixel(
                        (u * image.width() as f64).floor() as u32,
                        (v * image.height() as f64).floor() as u32
                    ),
                    pixel
                );
            }
        }
        assert!(Placement::parse("0,0", "5,4", "0,0,1/0,5").is_err());
        assert!(Placement::parse("0,0", "5,4", "0,0,4,6").is_err());
    }
    fn image_component(name: &str, digest: &str, mode: u8, inverted: bool) -> String {
        format!(
            r#"<rdf:li crs:What="Mask/Image" crs:MaskName="{name}" crs:MaskDigest="{digest}" crs:MaskSubType="1" crs:MaskBlendMode="{mode}" crs:MaskInverted="{inverted}" crs:Origin="1,1" crs:FullMaskSize="5,4" crs:WholeImageArea="1,1,3,4"/>"#
        )
    }
    fn correction(components: &str) -> String {
        format!(
            r#"<rdf:li crs:CorrectionName="Synthetic subject" crs:LocalExposure2012="0.25"><crs:CorrectionMasks><rdf:Seq>{components}</rdf:Seq></crs:CorrectionMasks></rdf:li>"#
        )
    }
    #[test]
    fn image_components_combine_with_gradients_and_cache_repeated_rasters() {
        let mut acr = AcrMasks::parse(fixture()).unwrap();
        let images = format!(
            "{}{}",
            image_component("Subject", DIGEST, 0, false),
            image_component("Subtract", DIGEST, 1, true)
        );
        let gradient = r#"<rdf:li crs:What="Mask/Gradient" crs:ZeroX="0" crs:ZeroY="0" crs:FullX="1" crs:FullY="1" crs:MaskBlendMode="1"/>"#;
        let xmp = crate::lightroom_masks::tests::sidecar_with_corrections(
            "",
            &correction(&format!("{images}{gradient}")),
        );
        let frame = ImageFrame {
            width: 3.0,
            height: 2.0,
            orientation: 6,
            rotation: 17.0,
        };
        let (masks, report) =
            import_lightroom_masks_with_acr(&xmp, Some(&frame), |_| {}, Some(&mut acr), None);
        assert_eq!(report.imported_ai, 1);
        assert!(report.failures.is_empty());
        assert_eq!(acr.cache.len(), 1);
        assert_eq!(masks[0]["adjustments"]["exposure"], 1.0);
        let subs = masks[0]["subMasks"].as_array().unwrap();
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0]["mode"], "additive");
        assert_eq!(subs[1]["mode"], "subtractive");
        assert_eq!(subs[2]["mode"], "intersect");
        assert_eq!(subs[0]["parameters"]["rotation"], 17.0);
        assert!(
            serde_json::from_value::<crate::mask_generation::MaskDefinition>(masks[0].clone())
                .is_ok()
        );
    }
    #[test]
    fn missing_or_invalid_ai_components_report_each_name_and_keep_correction_whole() {
        let xmp = crate::lightroom_masks::tests::sidecar_with_corrections(
            "",
            &correction(&format!(
                "{}{}",
                image_component("Subject", DIGEST, 0, false),
                image_component("Person", "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", 0, false)
            )),
        );
        let frame = ImageFrame {
            width: 3.0,
            height: 2.0,
            orientation: 1,
            rotation: 0.0,
        };
        let (masks, report) = import_lightroom_masks_with_acr(
            &xmp,
            Some(&frame),
            |_| {},
            None,
            Some("Missing ACR sidecar"),
        );
        assert!(masks.is_empty());
        assert_eq!(report.failures.len(), 2);
        assert_eq!(report.failures[0].component, "Subject");
        assert_eq!(report.failures[1].component, "Person");
        let mut acr = AcrMasks::parse(fixture()).unwrap();
        let (masks, report) =
            import_lightroom_masks_with_acr(&xmp, Some(&frame), |_| {}, Some(&mut acr), None);
        assert!(masks.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            report.failures[0].reason,
            "Mask digest not found in ACR sidecar"
        );
    }
    #[test]
    #[ignore = "requires disposable local Lightroom RAW/XMP/ACR fixture and output path"]
    fn local_lightroom_ai_mask_import_probe() {
        let input = std::path::PathBuf::from(std::env::var("RAPIDROOM_LRAI_INPUT").unwrap());
        let xmp = crate::xmp::read(&input.with_extension("xmp")).unwrap();
        let (preset, report) =
            crate::preset_converter::convert_xmp_sidecar_for_image_with_mask_report(
                &xmp,
                &input,
                &input.with_extension("xmp"),
            )
            .unwrap();
        std::fs::write(std::env::var("RAPIDROOM_LRAI_OUTPUT").unwrap(),serde_json::to_vec_pretty(&serde_json::json!({"adjustments":preset.adjustments,"importedAi":report.imported_ai,"failures":report.failures})).unwrap()).unwrap();
        assert!(report.imported_ai > 0);
    }
}
