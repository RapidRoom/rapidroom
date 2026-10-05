use image::{DynamicImage, GenericImageView, Rgb, RgbImage, imageops};
use serde::Deserialize;
use serde_json::Value;

pub(super) const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Region {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

pub(super) fn region(arguments: &Value) -> Result<Option<Region>, String> {
    let Some(value) = arguments.get("region") else {
        return Ok(None);
    };
    let region: Region = serde_json::from_value(value.clone())
        .map_err(|_| "region requires finite x/y/width/height fractions".to_string())?;
    if [region.x, region.y, region.width, region.height]
        .iter()
        .any(|value| !value.is_finite())
        || region.x < 0.0
        || region.y < 0.0
        || region.width <= 0.0
        || region.height <= 0.0
        || region.x + region.width > 1.0
        || region.y + region.height > 1.0
    {
        return Err("region must be a positive rectangle inside the rendered image, using fractions from 0 to 1".into());
    }
    Ok(Some(region))
}

pub(super) fn option(arguments: &Value, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{key} must be boolean")),
    }
}

pub(super) fn crop(image: DynamicImage, region: Option<Region>) -> DynamicImage {
    let Some(region) = region else { return image };
    let (width, height) = image.dimensions();
    let x = ((region.x * width as f64).floor() as u32).min(width - 1);
    let y = ((region.y * height as f64).floor() as u32).min(height - 1);
    let end_x = (((region.x + region.width) * width as f64).ceil() as u32).clamp(x + 1, width);
    let end_y = (((region.y + region.height) * height as f64).ceil() as u32).clamp(y + 1, height);
    image.crop_imm(x, y, end_x - x, end_y - y)
}

pub(super) fn side_by_side(
    original: &DynamicImage,
    edited: &DynamicImage,
    long_edge: u32,
) -> DynamicImage {
    let edge = long_edge / 2;
    let mut sheet = RgbImage::from_pixel(edge * 2, edge, Rgb([24, 24, 24]));
    for (index, image) in [original, edited].into_iter().enumerate() {
        let scaled = image.thumbnail(edge, edge).to_rgb8();
        let x = index as u32 * edge + (edge - scaled.width()) / 2;
        let y = (edge - scaled.height()) / 2;
        imageops::overlay(&mut sheet, &scaled, x as i64, y as i64);
    }
    DynamicImage::ImageRgb8(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invalid_and_outside_regions_are_refused_before_rendering() {
        for value in [
            json!({"x":-0.1,"y":0,"width":0.5,"height":0.5}),
            json!({"x":0.8,"y":0,"width":0.3,"height":0.5}),
            json!({"x":0,"y":0,"width":0,"height":0.5}),
            json!({"x":0,"y":0,"width":0.5,"height":0.5,"unit":"px"}),
        ] {
            assert!(region(&json!({"region":value})).is_err());
        }
        assert!(option(&json!({"original":"true"}), "original").is_err());
    }

    #[test]
    fn fraction_crop_selects_the_expected_pixels_and_preserves_single_pixel_regions() {
        let image =
            DynamicImage::ImageRgb8(RgbImage::from_fn(10, 8, |x, y| Rgb([x as u8, y as u8, 0])));
        let result = crop(
            image.clone(),
            region(&json!({"region":{"x":0.2,"y":0.25,"width":0.5,"height":0.5}})).unwrap(),
        );
        assert_eq!(result.dimensions(), (5, 4));
        assert_eq!(result.to_rgb8().get_pixel(0, 0), &Rgb([2, 2, 0]));
        let tiny = crop(
            image,
            region(&json!({"region":{"x":0.99,"y":0.99,"width":0.01,"height":0.01}})).unwrap(),
        );
        assert_eq!(tiny.dimensions(), (1, 1));
    }

    #[test]
    fn comparison_keeps_order_and_aspect_ratio_with_a_bounded_long_edge() {
        let red = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 10, Rgb([255, 0, 0])));
        let blue = DynamicImage::ImageRgb8(RgbImage::from_pixel(5, 10, Rgb([0, 0, 255])));
        let result = side_by_side(&red, &blue, 128).to_rgb8();
        assert_eq!(result.dimensions(), (128, 64));
        assert_eq!(result.get_pixel(32, 32), &Rgb([255, 0, 0]));
        assert_eq!(result.get_pixel(96, 32), &Rgb([0, 0, 255]));
        assert_eq!(result.get_pixel(0, 0), &Rgb([24, 24, 24]));
    }
}
