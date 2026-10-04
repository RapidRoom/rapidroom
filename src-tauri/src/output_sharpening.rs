use image::{DynamicImage, ImageBuffer, Pixel, imageops};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SharpenTarget {
    Screen,
    Print,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SharpenAmount {
    Low,
    Standard,
    High,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutputSharpening {
    pub target: SharpenTarget,
    pub amount: SharpenAmount,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharpenParams {
    pub sigma: f32,
    pub strength: f32,
    pub threshold: f32,
}

// Radius follows the output size: a 1080 px screen image needs a fine halo,
// a 6000 px print a wider one that survives the printer's dot spread.
pub fn sharpen_params(settings: &OutputSharpening, width: u32, height: u32) -> SharpenParams {
    let long_edge = width.max(height) as f32;
    let amount = match settings.amount {
        SharpenAmount::Low => 0.5,
        SharpenAmount::Standard => 0.8,
        SharpenAmount::High => 1.2,
    };
    match settings.target {
        SharpenTarget::Screen => SharpenParams {
            sigma: (0.5 + long_edge / 12_000.0).clamp(0.5, 1.0),
            strength: amount,
            threshold: 1.0 / 255.0,
        },
        SharpenTarget::Print => SharpenParams {
            sigma: (long_edge / 4_000.0).clamp(0.8, 2.0),
            strength: amount * 1.5,
            threshold: 2.0 / 255.0,
        },
    }
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

trait SharpenChannel: image::Primitive {
    const FULL_SCALE: f32;
    fn to_f32(self) -> f32;
    fn from_f32(value: f32) -> Self;
}

impl SharpenChannel for u8 {
    const FULL_SCALE: f32 = u8::MAX as f32;
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn from_f32(value: f32) -> Self {
        value
            .round()
            .clamp(0.0, <Self as SharpenChannel>::FULL_SCALE) as u8
    }
}

impl SharpenChannel for u16 {
    const FULL_SCALE: f32 = u16::MAX as f32;
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn from_f32(value: f32) -> Self {
        value
            .round()
            .clamp(0.0, <Self as SharpenChannel>::FULL_SCALE) as u16
    }
}

impl SharpenChannel for f32 {
    const FULL_SCALE: f32 = 1.0;
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(value: f32) -> Self {
        value.clamp(0.0, <Self as SharpenChannel>::FULL_SCALE)
    }
}

// Unsharp mask on luminance only, so edges get no colour fringes. Alpha is untouched.
fn sharpen_buffer<P>(src: &mut ImageBuffer<P, Vec<P::Subpixel>>, params: SharpenParams)
where
    P: Pixel + 'static,
    P::Subpixel: SharpenChannel + 'static,
{
    let channels = P::CHANNEL_COUNT as usize;
    if channels < 3 {
        return;
    }
    let blurred = imageops::blur(src, params.sigma);
    let threshold = params.threshold * <P::Subpixel as SharpenChannel>::FULL_SCALE;

    for (pixel, blurred_pixel) in src
        .as_mut()
        .chunks_exact_mut(channels)
        .zip(blurred.as_raw().chunks_exact(channels))
    {
        let read = |p: &[P::Subpixel]| [p[0].to_f32(), p[1].to_f32(), p[2].to_f32()];
        let original = read(pixel);
        let detail = luma(original) - luma(read(blurred_pixel));
        if detail.abs() <= threshold {
            continue;
        }
        let delta = params.strength * detail;
        for (channel, value) in pixel.iter_mut().take(3).zip(original) {
            *channel = P::Subpixel::from_f32(value + delta);
        }
    }
}

pub fn apply_output_sharpening(image: &mut DynamicImage, settings: &OutputSharpening) {
    let params = sharpen_params(settings, image.width(), image.height());
    match image {
        DynamicImage::ImageRgb8(buffer) => sharpen_buffer(buffer, params),
        DynamicImage::ImageRgba8(buffer) => sharpen_buffer(buffer, params),
        DynamicImage::ImageRgb16(buffer) => sharpen_buffer(buffer, params),
        DynamicImage::ImageRgba16(buffer) => sharpen_buffer(buffer, params),
        DynamicImage::ImageRgb32F(buffer) => sharpen_buffer(buffer, params),
        DynamicImage::ImageRgba32F(buffer) => sharpen_buffer(buffer, params),
        other => {
            let mut rgba = other.to_rgba8();
            sharpen_buffer(&mut rgba, params);
            *other = DynamicImage::ImageRgba8(rgba);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    const STANDARD_SCREEN: OutputSharpening = OutputSharpening {
        target: SharpenTarget::Screen,
        amount: SharpenAmount::Standard,
    };

    fn edge_image() -> RgbImage {
        RgbImage::from_fn(32, 8, |x, _| {
            if x < 16 {
                Rgb([80, 80, 80])
            } else {
                Rgb([160, 160, 160])
            }
        })
    }

    #[test]
    fn edges_gain_contrast() {
        let mut image = DynamicImage::ImageRgb8(edge_image());
        apply_output_sharpening(&mut image, &STANDARD_SCREEN);
        let out = image.to_rgb8();
        assert!(out.get_pixel(15, 4)[0] < 80, "dark side should get darker");
        assert!(
            out.get_pixel(16, 4)[0] > 160,
            "light side should get lighter"
        );
    }

    #[test]
    fn flat_areas_are_unchanged() {
        let flat = RgbImage::from_pixel(16, 16, Rgb([120, 60, 30]));
        let mut image = DynamicImage::ImageRgb8(flat.clone());
        apply_output_sharpening(&mut image, &STANDARD_SCREEN);
        assert_eq!(image.to_rgb8(), flat);

        let mut edge = DynamicImage::ImageRgb8(edge_image());
        apply_output_sharpening(&mut edge, &STANDARD_SCREEN);
        let out = edge.to_rgb8();
        assert_eq!(out.get_pixel(2, 4), &Rgb([80, 80, 80]));
        assert_eq!(out.get_pixel(29, 4), &Rgb([160, 160, 160]));
    }

    #[test]
    fn colour_and_alpha_are_kept() {
        let src = RgbaImage::from_fn(32, 8, |x, _| {
            if x < 16 {
                Rgba([100, 40, 20, 200])
            } else {
                Rgba([200, 80, 40, 200])
            }
        });
        let mut image = DynamicImage::ImageRgba8(src);
        apply_output_sharpening(&mut image, &STANDARD_SCREEN);
        let out = image.to_rgba8();
        for pixel in out.pixels() {
            assert_eq!(pixel[3], 200);
        }
        let dark = out.get_pixel(15, 4);
        let light = out.get_pixel(16, 4);
        // Luminance sharpening shifts every channel by the same delta.
        assert_eq!(100 - dark[0] as i32, 40 - dark[1] as i32);
        assert_eq!(light[0] as i32 - 200, light[1] as i32 - 80);
    }

    #[test]
    fn sixteen_bit_images_stay_sixteen_bit() {
        let src = ImageBuffer::<Rgb<u16>, Vec<u16>>::from_fn(32, 8, |x, _| {
            if x < 16 {
                Rgb([20_000, 20_000, 20_000])
            } else {
                Rgb([40_000, 40_000, 40_000])
            }
        });
        let mut image = DynamicImage::ImageRgb16(src);
        apply_output_sharpening(&mut image, &STANDARD_SCREEN);
        let DynamicImage::ImageRgb16(out) = image else {
            panic!("pixel type changed");
        };
        assert!(out.get_pixel(15, 4)[0] < 20_000);
        assert!(out.get_pixel(16, 4)[0] > 40_000);
    }

    #[test]
    fn radius_grows_with_output_size_and_print() {
        let small = sharpen_params(&STANDARD_SCREEN, 1080, 1350);
        let large = sharpen_params(&STANDARD_SCREEN, 6000, 4000);
        assert!(large.sigma > small.sigma);

        let print = OutputSharpening {
            target: SharpenTarget::Print,
            amount: SharpenAmount::Standard,
        };
        let print_params = sharpen_params(&print, 6000, 4000);
        assert!(print_params.sigma > large.sigma);
        assert!(print_params.strength > large.strength);

        let high = OutputSharpening {
            target: SharpenTarget::Screen,
            amount: SharpenAmount::High,
        };
        assert!(sharpen_params(&high, 1080, 1350).strength > small.strength);
    }

    #[test]
    fn stronger_amount_sharpens_more() {
        let run = |amount| {
            let mut image = DynamicImage::ImageRgb8(edge_image());
            apply_output_sharpening(
                &mut image,
                &OutputSharpening {
                    target: SharpenTarget::Screen,
                    amount,
                },
            );
            image.to_rgb8().get_pixel(16, 4)[0]
        };
        assert!(run(SharpenAmount::Low) < run(SharpenAmount::Standard));
        assert!(run(SharpenAmount::Standard) < run(SharpenAmount::High));
    }
}
