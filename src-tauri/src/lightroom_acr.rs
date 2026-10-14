//! Read-only, bounded support for ACR v1 grayscale TIFF/JPEG XL mask payloads.
//! Other payloads (including enhanced image data) are never interpreted as masks.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{GrayImage, ImageEncoder, Luma, imageops};
use std::{collections::HashMap, path::Path};

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
pub(crate) struct AcrMasks {
    file: crate::acr::AcrFile,
    #[cfg(test)]
    fixture: Option<tempfile::NamedTempFile>,
    cache: HashMap<String, String>,
    output_pixels: u64,
    encoded_bytes: usize,
}
impl AcrMasks {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        Ok(Self {
            file: crate::acr::AcrFile::open(path).map_err(|e| e.to_string())?,
            #[cfg(test)]
            fixture: None,
            cache: HashMap::new(),
            output_pixels: 0,
            encoded_bytes: 0,
        })
    }
    #[cfg(test)]
    fn parse(bytes: Vec<u8>) -> Result<Self> {
        use std::io::Write;
        let mut fixture = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        fixture.write_all(&bytes).map_err(|e| e.to_string())?;
        let mut reader = Self::read(fixture.path())?;
        reader.fixture = Some(fixture);
        Ok(reader)
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
            let blob = self.file.read_object(digest).map_err(|e| e.to_string())?;
            let raster = decode_mask(&blob)?;
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
            if self.encoded_bytes.saturating_add(png.len()) > MAX_ENCODED {
                return Err("Mask import PNG budget exceeded".into());
            }
            self.cache.insert(key, png.clone());
            png
        };
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(png.len())
            .ok_or("Mask import PNG budget exceeded")?;
        if self.encoded_bytes > MAX_ENCODED {
            return Err("Mask import PNG budget exceeded".into());
        }
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
    let mut metadata_ranges = Vec::with_capacity(count + 1);
    metadata_ranges.push(0..ifd + 2 + count * 12 + 4);
    for i in 0..count {
        let entry = ifd + 2 + i * 12;
        let tag = u16_at(tiff, entry)?;
        let kind = u16_at(tiff, entry + 2)?;
        let values_count = u32_at(tiff, entry + 4)? as usize;
        if !matches!(kind, 3 | 4)
            || values_count == 0
            || values_count > 256
            || (values_count != 1 && !matches!(tag, 273 | 279 | 324 | 325))
        {
            return Err("Unsupported mask TIFF tag type/count".into());
        }
        let item_size = if kind == 3 { 2 } else { 4 };
        let len = values_count * item_size;
        let offset = if len <= 4 {
            entry + 8
        } else {
            u32_at(tiff, entry + 8)? as usize
        };
        slice(tiff, offset, len)?;
        if len > 4 {
            if metadata_ranges
                .iter()
                .any(|r| offset < r.end && offset + len > r.start)
            {
                return Err("Overlapping mask TIFF metadata".into());
            }
            metadata_ranges.push(offset..offset + len);
        }
        let mut values = Vec::with_capacity(values_count);
        for index in 0..values_count {
            values.push(if kind == 3 {
                u16_at(tiff, offset + index * 2)? as u32
            } else {
                u32_at(tiff, offset + index * 4)?
            });
        }
        if tags.insert(tag, values).is_some() {
            return Err("Duplicate mask TIFF tag".into());
        }
    }
    let required = |tag| -> Result<u32> {
        let values = tags
            .get(&tag)
            .ok_or_else(|| format!("Missing mask TIFF tag {tag}"))?;
        if values.len() != 1 {
            return Err("Invalid scalar mask TIFF tag".into());
        }
        Ok(values[0])
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
        || tags.get(&284).is_some_and(|v| v != &[1])
        || tags.get(&274).is_some_and(|v| v != &[1])
        || tags.get(&339).is_some_and(|v| v != &[1])
        || tags.contains_key(&338)
    {
        return Err("Only 8-bit grayscale JPEG XL masks are supported".into());
    }
    let (tile_width, tile_height, offset_tag, size_tag) = if tags.contains_key(&324) {
        if tags.contains_key(&273) || tags.contains_key(&279) {
            return Err("Mixed mask TIFF tile/strip layout".into());
        }
        (required(322)?, required(323)?, 324, 325)
    } else {
        if tags.contains_key(&325) {
            return Err("Mixed mask TIFF tile/strip layout".into());
        }
        (width, required(278)?.min(height), 273, 279)
    };
    if tile_width == 0 || tile_height == 0 || tile_width > width || tile_height > height {
        return Err("Invalid mask TIFF tile/strip dimensions".into());
    }
    let columns = width.div_ceil(tile_width);
    let rows = height.div_ceil(tile_height);
    let tile_count = columns as usize * rows as usize;
    let offsets = tags.get(&offset_tag).ok_or("Missing mask TIFF offsets")?;
    let sizes = tags.get(&size_tag).ok_or("Missing mask TIFF byte counts")?;
    if tile_count > 256 || offsets.len() != tile_count || sizes.len() != tile_count {
        return Err("Mask TIFF tile/strip count mismatch".into());
    }
    let mut ranges = metadata_ranges;
    for (&offset, &size) in offsets.iter().zip(sizes) {
        let (offset, size) = (offset as usize, size as usize);
        if size == 0 || size > 16 * 1024 * 1024 {
            return Err("Invalid mask codestream range".into());
        }
        slice(tiff, offset, size)?;
        if ranges
            .iter()
            .any(|r| offset < r.end && offset + size > r.start)
        {
            return Err("Overlapping mask TIFF codestreams/metadata".into());
        }
        ranges.push(offset..offset + size);
    }
    let mut image = GrayImage::new(width, height);
    for (index, (&offset, &size)) in offsets.iter().zip(sizes).enumerate() {
        let x = (index as u32 % columns) * tile_width;
        let y = (index as u32 / columns) * tile_height;
        let visible_width = tile_width.min(width - x);
        let visible_height = tile_height.min(height - y);
        let codestream = slice(tiff, offset as usize, size as usize)?;
        let tile = decode_jxl(
            codestream,
            tile_width,
            tile_height,
            visible_width,
            visible_height,
        )?;
        for ty in 0..visible_height {
            let src = ty as usize * tile.width() as usize;
            let dst = (y + ty) as usize * width as usize + x as usize;
            image.as_mut()[dst..dst + visible_width as usize]
                .copy_from_slice(&tile.as_raw()[src..src + visible_width as usize]);
        }
    }
    Ok(image)
}

fn decode_jxl(
    codestream: &[u8],
    padded_width: u32,
    padded_height: u32,
    visible_width: u32,
    visible_height: u32,
) -> Result<GrayImage> {
    let decoder = jxl_oxide::JxlImage::builder()
        .alloc_tracker(jxl_oxide::AllocTracker::with_limit(128 * 1024 * 1024))
        .read(codestream)
        .map_err(|_| "Cannot decode mask JPEG XL")?;
    let width = decoder.width();
    let height = decoder.height();
    if (width != padded_width && width != visible_width)
        || (height != padded_height && height != visible_height)
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
    if stream.write_to_buffer(&mut floats) != floats.len() {
        return Err("Invalid mask coverage samples".into());
    }
    GrayImage::from_raw(width, height, coverage_pixels(floats)?)
        .ok_or_else(|| "Invalid mask raster".into())
}

fn coverage_pixels(samples: Vec<f32>) -> Result<Vec<u8>> {
    if samples.iter().any(|v| !v.is_finite()) {
        return Err("Invalid mask coverage samples".into());
    }
    // JPEG XL reconstruction may overshoot its nominal range, just as its
    // integer output conversion clamps finite samples before quantization.
    Ok(samples
        .into_iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect())
}

#[derive(Debug)]
struct Placement {
    origin: [f64; 2],
    full: [f64; 2],
    area: [f64; 4],
}
fn numbers<const N: usize>(text: &str) -> Result<[f64; N]> {
    let mut parts = text.split(',');
    let mut values = [0.0; N];
    for value in &mut values {
        let part = parts
            .next()
            .ok_or("Invalid mask geometry tuple length")?
            .trim();
        let (n, d) = part.split_once('/').unwrap_or((part, "1"));
        *value = n.parse::<f64>().map_err(|_| "Invalid mask geometry")?
            / d.parse::<f64>().map_err(|_| "Invalid mask geometry")?;
        if !value.is_finite() {
            return Err("Invalid mask geometry".into());
        }
    }
    if parts.next().is_some() {
        return Err("Invalid mask geometry tuple length".into());
    }
    Ok(values)
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
    fn fixture_digest() -> String {
        let bytes = fixture();
        let size = u32_at(&bytes, 36).unwrap() as usize;
        format!("{:X}", md5::compute(&bytes[52..52 + size]))
    }
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
        acr.extend_from_slice(&md5::compute(&blob).0);
        acr.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        acr.extend_from_slice(&0u32.to_le_bytes());
        acr.extend_from_slice(&52u32.to_le_bytes());
        acr.extend_from_slice(&0u32.to_le_bytes());
        acr.extend_from_slice(&blob);
        while !acr.len().is_multiple_of(4) {
            acr.push(0);
        }
        acr
    }
    #[test]
    fn synthetic_acr_decodes_exact_coverage_without_colour_remapping() {
        let bytes = fixture();
        let blob_len = u32_at(&bytes, 36).unwrap() as usize;
        let mut acr = AcrMasks::parse(bytes).unwrap();
        let blob = acr.file.read_object(&fixture_digest()).unwrap();
        assert_eq!(blob.len(), blob_len);
        let image = decode_mask(&blob).unwrap();
        assert_eq!(image.as_raw(), &[0, 64, 128, 192, 255, 32]);
    }
    #[test]
    fn lossy_grayscale_edges_allow_finite_reconstruction_overshoot() {
        let codestream = include_bytes!("../tests/fixtures/lightroom-ai/lossy-edges.jxl");
        let decoder = jxl_oxide::JxlImage::builder()
            .read(&codestream[..])
            .unwrap();
        let render = decoder.render_frame(0).unwrap();
        let mut samples = vec![0.0f32; 64 * 64];
        assert_eq!(render.stream().write_to_buffer(&mut samples), samples.len());
        assert!(samples.iter().all(|v| v.is_finite()));
        assert!(samples.iter().any(|v| *v < 0.0 || *v > 1.0));
        let mask = decode_jxl(codestream, 64, 64, 64, 64).unwrap();
        assert_eq!(mask.dimensions(), (64, 64));
        assert!(mask.get_pixel(0, 0)[0] <= 2);
        assert!(mask.get_pixel(63, 0)[0] >= 253);
    }

    #[test]
    fn coverage_quantization_clamps_finite_values_and_rejects_nonfinite_samples() {
        assert_eq!(
            coverage_pixels(vec![-0.02, 0.0, 128.0 / 255.0, 1.0, 1.02]).unwrap(),
            vec![0, 0, 128, 255, 255]
        );
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(coverage_pixels(vec![0.0, invalid]).is_err());
        }
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
        assert!(crate::acr::parse_key("../malicious/path").is_err());
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
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            let blob_len = u32_at(&bad, 36).unwrap() as usize;
            assert!(
                decode_mask(&bad[52..52 + blob_len]).is_err(),
                "tag {tag_index}"
            );
        }
    }
    fn tiled_blob() -> Vec<u8> {
        let tiles = [[0, 64], [128, 0], [192, 255], [32, 0]].map(|pixels| {
            jxl_encoder::LosslessConfig::default()
                .encode(&pixels, 2, 1, jxl_encoder::PixelLayout::Gray8)
                .unwrap()
        });
        let offsets: Vec<u32> = tiles
            .iter()
            .scan(178u32, |offset, tile| {
                let current = *offset;
                *offset += tile.len() as u32;
                Some(current)
            })
            .collect();
        let tags: [(u16, u32, u32); 11] = [
            (256, 1, 3),
            (257, 1, 2),
            (258, 1, 8),
            (259, 1, 52546),
            (262, 1, 1),
            (277, 1, 1),
            (284, 1, 1),
            (322, 1, 2),
            (323, 1, 1),
            (324, 4, 146),
            (325, 4, 162),
        ];
        let mut blob = vec![2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        blob.extend_from_slice(b"II*\0\x08\0\0\0");
        blob.extend_from_slice(&11u16.to_le_bytes());
        for (tag, count, value) in tags {
            blob.extend_from_slice(&tag.to_le_bytes());
            blob.extend_from_slice(&4u16.to_le_bytes());
            blob.extend_from_slice(&count.to_le_bytes());
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob.extend_from_slice(&0u32.to_le_bytes());
        for offset in offsets {
            blob.extend_from_slice(&offset.to_le_bytes());
        }
        for tile in &tiles {
            blob.extend_from_slice(&(tile.len() as u32).to_le_bytes());
        }
        for tile in tiles {
            blob.extend_from_slice(&tile);
        }
        blob
    }
    #[test]
    fn multiple_tiles_preserve_coverage_and_clip_only_padded_edge_pixels() {
        let blob = tiled_blob();
        assert_eq!(
            decode_mask(&blob).unwrap().as_raw(),
            &[0, 64, 128, 192, 255, 32]
        );
        for offset in [16 + 146, 16 + 146 + 4, 16 + 162] {
            let mut bad = blob.clone();
            bad[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
            assert!(decode_mask(&bad).is_err());
        }
        for len in 0..blob.len() {
            assert!(decode_mask(&blob[..len]).is_err());
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
            image_component("Subject", &fixture_digest(), 0, false),
            image_component("Subtract", &fixture_digest(), 1, true)
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
    fn refused_pngs_never_expand_the_cache_past_the_import_budget() {
        let properties = HashMap::from([
            ("MaskDigest".into(), fixture_digest()),
            ("Origin".into(), "1,1".into()),
            ("FullMaskSize".into(), "5,4".into()),
            ("WholeImageArea".into(), "1,1,3,4".into()),
        ]);
        let frame = ImageFrame {
            width: 3.0,
            height: 2.0,
            orientation: 1,
            rotation: 0.0,
        };
        let mut acr = AcrMasks::parse(fixture()).unwrap();
        acr.encoded_bytes = MAX_ENCODED;
        assert!(acr.image_parameters(&properties, &frame).is_err());
        assert!(acr.cache.is_empty());
        acr.encoded_bytes = 0;
        assert!(acr.image_parameters(&properties, &frame).is_ok());
        assert_eq!(acr.cache.len(), 1);
        acr.encoded_bytes = MAX_ENCODED;
        assert!(acr.image_parameters(&properties, &frame).is_err());
        assert_eq!(acr.cache.len(), 1);
        assert!(numbers::<2>("1").is_err());
        assert!(numbers::<2>("1,2,3").is_err());
    }
    #[test]
    fn missing_or_invalid_ai_components_report_each_name_and_keep_correction_whole() {
        let xmp = crate::lightroom_masks::tests::sidecar_with_corrections(
            "",
            &correction(&format!(
                "{}{}",
                image_component("Subject", &fixture_digest(), 0, false),
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
            "the companion reference is missing or ambiguous"
        );
    }
    #[test]
    #[ignore = "read-only local XMP/ACR coverage inventory, reduced diagnostic frame"]
    fn local_lightroom_ai_mask_decode_inventory() {
        let directory = std::path::PathBuf::from(std::env::var("RAPIDROOM_LRAI_XMP_DIR").unwrap());
        let mut records = Vec::new();
        let frame = ImageFrame {
            width: 600.0,
            height: 400.0,
            orientation: 1,
            rotation: 0.0,
        };
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("xmp") {
                continue;
            }
            let xmp = crate::xmp::read(&path).unwrap();
            if !xmp.contains("Mask/Image") {
                continue;
            }
            let (masks, report) = crate::lightroom_masks::import_lightroom_masks_for_image(
                &xmp,
                Some(&frame),
                Some(&path),
                |_| {},
            );
            let rasters = masks
                .iter()
                .flat_map(|mask| mask["subMasks"].as_array().unwrap())
                .filter(|sub| sub["type"] == "ai-foreground")
                .count();
            records.push(serde_json::json!({"source":path.file_name().unwrap().to_string_lossy(),"importedAiCorrections":report.imported_ai,"rasters":rasters,"failures":report.failures,"brushTableRefusals":report.brush_table_refusals}));
        }
        std::fs::write(
            std::env::var("RAPIDROOM_LRAI_OUTPUT").unwrap(),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
        assert!(!records.is_empty());
    }
    #[test]
    #[ignore = "writes an authored synthetic fixture for the optional native smoke"]
    fn write_native_synthetic_acr_fixture() {
        std::fs::write(
            std::env::var("RAPIDROOM_LRAI_FIXTURE_OUTPUT").unwrap(),
            fixture(),
        )
        .unwrap();
    }
    #[test]
    #[ignore = "requires disposable local Lightroom RAW/XMP/ACR fixture and output path"]
    fn local_lightroom_ai_mask_import_probe() {
        let input = std::path::PathBuf::from(std::env::var("RAPIDROOM_LRAI_INPUT").unwrap());
        let xmp = crate::xmp::read(&input.with_extension("xmp")).unwrap();
        let (preset, report) =
            crate::preset_converter::convert_xmp_sidecar_to_preset_for_image_with_mask_report(
                &xmp, &input,
            )
            .unwrap();
        std::fs::write(std::env::var("RAPIDROOM_LRAI_OUTPUT").unwrap(),serde_json::to_vec_pretty(&serde_json::json!({"adjustments":preset.adjustments,"importedAi":report.imported_ai,"failures":report.failures})).unwrap()).unwrap();
        assert!(report.imported_ai > 0);
    }
}
