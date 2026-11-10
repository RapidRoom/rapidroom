// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Duecki and CalibRaw contributors.
// Adapted from Duecki1/CalibRaw@2466ca6662c3fd1e901377c0450b19f5f96d3e85
// crates/calibraw-ai/src/remove/lama/{grain.rs,lama.rs}.
// The adapted portions retain GPL-3.0-or-later; combined with RapidRoom under
// GPLv3 section 13. See resources/remove-grain-licenses/CalibRaw-GPL-3.0.txt.
//! Re-synthesizing sensor grain over inpainted areas from nearby source texture.

use image::{GrayImage, Rgb32FImage};

const GRAIN_BLOCK: u32 = 16;
const GRAIN_MAX_BRIGHTNESS_RATIO: f32 = 1.5;
const GRAIN_NEAR_DISTANCE: u32 = 48;
const GRAIN_MAX_ENERGY_RATIO: f32 = 1.5;
const GRAIN_MAX_PEAK_DEVIATIONS: f32 = 3.5;
const GRAIN_MAX_COHERENCE: f32 = 0.05;
const GRAIN_MAX_SOURCES: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct NativeRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl NativeRect {
    fn right(self) -> u32 {
        self.x + self.width
    }
    fn bottom(self) -> u32 {
        self.y + self.height
    }
}

/// The photo's grain around a pass, ready to be copied into its fill.
pub(crate) struct Grain {
    /// Top-left corners (local) of blocks that hold pure grain.
    pub(crate) sources: Vec<(u32, u32)>,
    /// Origin of the block grid in the fill, so every fill block maps to one source.
    grid_origin: (u32, u32),
    seed: u64,
}

impl Grain {
    /// `None` when the surroundings hold no block of pure grain to copy.
    pub(crate) fn sample(scene: &Rgb32FImage, mask: &GrayImage, fill: NativeRect) -> Option<Self> {
        let (width, height) = (scene.width() as usize, scene.height() as usize);
        if scene.dimensions() != mask.dimensions()
            || width == 0
            || height == 0
            || scene.as_raw().iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let bounds = NativeRect {
            x: 0,
            y: 0,
            width: scene.width(),
            height: scene.height(),
        };
        let lowpass = gaussian_blur_rgb(scene.as_raw(), width, height, 2.5);
        let residual = scene
            .as_raw()
            .as_chunks::<3>()
            .0
            .iter()
            .zip(lowpass.as_chunks::<3>().0.iter())
            .map(|(pixel, low)| [pixel[0] - low[0], pixel[1] - low[1], pixel[2] - low[2]])
            .collect::<Vec<_>>();
        let base_luma: Vec<f32> = lowpass
            .as_chunks::<3>()
            .0
            .iter()
            .map(|low| luma([low[0], low[1], low[2]]))
            .collect();
        // The blur has an eight-pixel footprint. Exclude that entire footprint
        // around the object so its colour cannot leak into a source residual.
        let excluded = mask
            .as_raw()
            .iter()
            .map(|value| *value != 0)
            .collect::<Vec<_>>();
        let excluded = dilate_square(&excluded, width, height, 8);

        let block = GRAIN_BLOCK as usize;
        let mut candidates = Vec::new();
        for top in (0..height.saturating_sub(block - 1)).step_by(block) {
            for left in (0..width.saturating_sub(block - 1)).step_by(block) {
                let rows = top..top + block;
                if rows
                    .clone()
                    .any(|y| excluded[y * width + left..y * width + left + block].contains(&true))
                {
                    continue;
                }
                let corner = (left as u32, top as u32);
                let global = NativeRect {
                    x: bounds.x + corner.0,
                    y: bounds.y + corner.1,
                    width: GRAIN_BLOCK,
                    height: GRAIN_BLOCK,
                };
                let pixels = rows.flat_map(|y| y * width + left..y * width + left + block);
                let (energy, peak, luma_sum) =
                    pixels.fold((0.0f32, 0.0f32, 0.0f32), |acc, index| {
                        (
                            acc.0 + luma(residual[index]).abs(),
                            acc.1.max(channel_magnitude(residual[index])),
                            acc.2 + base_luma[index],
                        )
                    });
                candidates.push(GrainCandidate {
                    corner,
                    energy,
                    peak,
                    luma: luma_sum / (block * block) as f32,
                    coherence: orientation_coherence(&residual, width, left, top),
                    distance: rect_distance(global, fill),
                });
            }
        }
        let sources = select_grain_sources(candidates, &residual, width);
        (!sources.is_empty()).then(|| Self {
            sources,
            grid_origin: (fill.x, fill.y),
            seed: u64::from(fill.x) << 32 | u64::from(fill.y),
        })
    }

    /// Deterministic address within the selected, contiguous 16x16 tiles.
    pub(crate) fn donor_index(&self, x: u32, y: u32) -> usize {
        let local_x = x.wrapping_sub(self.grid_origin.0);
        let local_y = y.wrapping_sub(self.grid_origin.1);
        let block = (local_x / GRAIN_BLOCK, local_y / GRAIN_BLOCK);
        let pick = mix64(self.seed ^ (u64::from(block.0) << 32 | u64::from(block.1)));
        (pick % self.sources.len() as u64) as usize * 256
            + (local_y % GRAIN_BLOCK) as usize * 16
            + (local_x % GRAIN_BLOCK) as usize
    }
}

/// A block of the surroundings that might be copied as grain.
#[derive(Clone, Copy, Debug)]
struct GrainCandidate {
    /// Top-left corner in surroundings-local coordinates.
    corner: (u32, u32),
    /// Sum of absolute residual luminance.
    energy: f32,
    /// Largest absolute residual on any channel.
    peak: f32,
    /// Mean low-pass luminance.
    pub(crate) luma: f32,
    /// How strongly the residual is oriented one way (0 = grain, 1 = a line).
    coherence: f32,
    /// Native distance to the fill.
    pub(crate) distance: u32,
}

/// Picks the blocks that hold only the photo's grain, from material like the
/// area around the hole, nearest first.
fn select_grain_sources(
    candidates: Vec<GrainCandidate>,
    residual: &[[f32; 3]],
    width: usize,
) -> Vec<(u32, u32)> {
    let near = candidates
        .iter()
        .filter(|candidate| candidate.distance <= GRAIN_NEAR_DISTANCE)
        .map(|candidate| candidate.luma)
        .collect::<Vec<_>>();
    let reference = median(if near.is_empty() {
        candidates.iter().map(|candidate| candidate.luma).collect()
    } else {
        near
    });
    let Some(reference) = reference else {
        return Vec::new();
    };
    let similar = candidates
        .into_iter()
        .filter(|candidate| {
            let ratio = candidate.luma.max(1e-6) / reference.max(1e-6);
            (1.0 / GRAIN_MAX_BRIGHTNESS_RATIO..=GRAIN_MAX_BRIGHTNESS_RATIO).contains(&ratio)
        })
        .collect::<Vec<_>>();
    let Some(energy) = median(similar.iter().map(|candidate| candidate.energy).collect()) else {
        return Vec::new();
    };
    let peak_limit = noise_deviation(&similar, residual, width) * GRAIN_MAX_PEAK_DEVIATIONS;
    let mut sources = similar
        .into_iter()
        .filter(|candidate| {
            candidate.energy <= energy * GRAIN_MAX_ENERGY_RATIO
                && candidate.peak <= peak_limit
                && candidate.coherence <= GRAIN_MAX_COHERENCE
        })
        .collect::<Vec<_>>();
    sources.sort_by_key(|candidate| candidate.distance);
    sources.truncate(GRAIN_MAX_SOURCES);
    sources
        .into_iter()
        .map(|candidate| candidate.corner)
        .collect()
}

/// Robust noise deviation of the candidate blocks: the median absolute
/// residual over their pixels, which features too sparse to move the median
/// cannot inflate.
fn noise_deviation(candidates: &[GrainCandidate], residual: &[[f32; 3]], width: usize) -> f32 {
    let block = GRAIN_BLOCK as usize;
    let magnitudes = candidates
        .iter()
        .flat_map(|candidate| {
            let (left, top) = (candidate.corner.0 as usize, candidate.corner.1 as usize);
            (top..top + block)
                .flat_map(move |y| &residual[y * width + left..y * width + left + block])
        })
        .map(|value| channel_magnitude(*value))
        .collect();
    // 1.4826 turns a median absolute deviation into a standard deviation.
    median(magnitudes).unwrap_or(0.0) * 1.4826
}

/// Structure-tensor coherence of the residual luminance in one block:
/// `((Jxx - Jyy)^2 + 4 Jxy^2) / (Jxx + Jyy)^2`.
pub(crate) fn orientation_coherence(
    residual: &[[f32; 3]],
    width: usize,
    left: usize,
    top: usize,
) -> f32 {
    let block = GRAIN_BLOCK as usize;
    let at = |x: usize, y: usize| luma(residual[y * width + x]);
    let (mut xx, mut yy, mut xy) = (0.0f32, 0.0f32, 0.0f32);
    for y in top + 1..top + block - 1 {
        for x in left + 1..left + block - 1 {
            let gradient_x = at(x + 1, y) - at(x - 1, y);
            let gradient_y = at(x, y + 1) - at(x, y - 1);
            xx += gradient_x * gradient_x;
            yy += gradient_y * gradient_y;
            xy += gradient_x * gradient_y;
        }
    }
    let trace = xx + yy;
    if trace <= f32::EPSILON {
        return 0.0;
    }
    ((xx - yy).powi(2) + 4.0 * xy * xy) / (trace * trace)
}

fn median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let middle = values.len() / 2;
    Some(*values.select_nth_unstable_by(middle, f32::total_cmp).1)
}

fn channel_magnitude(value: [f32; 3]) -> f32 {
    value
        .iter()
        .fold(0.0f32, |max, channel| max.max(channel.abs()))
}

/// Gap in native pixels between two rectangles; zero when they touch.
fn rect_distance(a: NativeRect, b: NativeRect) -> u32 {
    let gap = |start_a: u32, end_a: u32, start_b: u32, end_b: u32| {
        start_b
            .saturating_sub(end_a)
            .max(start_a.saturating_sub(end_b))
    };
    gap(a.x, a.right(), b.x, b.right()).max(gap(a.y, a.bottom(), b.y, b.bottom()))
}

pub(crate) fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

/// SplitMix64 finalizer: a well-mixed, deterministic hash.
pub(crate) fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

pub(crate) fn gaussian_blur_rgb(rgb: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil() as isize;
    let kernel = (-radius..=radius)
        .map(|offset| (-(offset * offset) as f32 / (2.0 * sigma * sigma)).exp())
        .collect::<Vec<_>>();
    let total = kernel.iter().sum::<f32>();
    let kernel = kernel
        .iter()
        .map(|weight| weight / total)
        .collect::<Vec<_>>();
    let pass = |source: &[f32], horizontal: bool| {
        let mut out = vec![0.0f32; source.len()];
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0.0f32; 3];
                for (tap, weight) in kernel.iter().enumerate() {
                    let offset = tap as isize - radius;
                    let (sample_x, sample_y) = if horizontal {
                        (
                            (x as isize + offset).clamp(0, width as isize - 1) as usize,
                            y,
                        )
                    } else {
                        (
                            x,
                            (y as isize + offset).clamp(0, height as isize - 1) as usize,
                        )
                    };
                    let index = (sample_y * width + sample_x) * 3;
                    for channel in 0..3 {
                        sum[channel] += source[index + channel] * weight;
                    }
                }
                out[(y * width + x) * 3..(y * width + x) * 3 + 3].copy_from_slice(&sum);
            }
        }
        out
    };
    pass(&pass(rgb, true), false)
}

fn dilate_square(mask: &[bool], width: usize, height: usize, radius: usize) -> Vec<bool> {
    let mut rows = vec![false; mask.len()];
    for y in 0..height {
        let row = &mask[y * width..(y + 1) * width];
        for x in 0..width {
            let start = x.saturating_sub(radius);
            let end = (x + radius + 1).min(width);
            rows[y * width + x] = row[start..end].iter().any(|value| *value);
        }
    }
    let mut out = vec![false; mask.len()];
    for x in 0..width {
        for y in 0..height {
            let start = y.saturating_sub(radius);
            let end = (y + radius + 1).min(height);
            out[y * width + x] = (start..end).any(|row| rows[row * width + x]);
        }
    }
    out
}

/// Match only new local fills; the native mask and stored feathering are unchanged.
/// Noise is measured in encoded colour components to account for model noise
/// without changing perceived brightness through another nonlinear conversion.
pub(crate) fn restore_grain(
    scene: &Rgb32FImage,
    mask: &GrayImage,
    fill: &mut image::RgbaImage,
    bounds: NativeRect,
) {
    if fill.dimensions() != scene.dimensions() {
        return;
    }
    #[cfg(test)]
    if let Ok(directory) = std::env::var("RAPIDROOM_REMOVE_DEBUG_DIR") {
        static SAMPLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let index = SAMPLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory = std::path::Path::new(&directory);
        crate::image_processing::apply_linear_to_srgb(image::DynamicImage::ImageRgb32F(
            scene.clone(),
        ))
        .to_rgb8()
        .save(directory.join(format!("sample-{index}-source.png")))
        .unwrap();
        fill.save(directory.join(format!("sample-{index}-model.png")))
            .unwrap();
        mask.save(directory.join(format!("sample-{index}-mask.png")))
            .unwrap();
    }
    if scene.dimensions() != mask.dimensions()
        || scene.as_raw().iter().any(|v| !v.is_finite())
        || bounds.width == 0
        || bounds.height == 0
        || bounds.x.saturating_add(bounds.width) > scene.width()
        || bounds.y.saturating_add(bounds.height) > scene.height()
    {
        return;
    }
    let grain = Grain::sample(scene, mask, bounds);
    crate::remove_grain_match::harmonize(scene, mask, fill, bounds, grain.as_ref());
}

#[cfg(test)]
fn decode(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
pub(crate) fn encode(value: f32) -> u8 {
    let value = value.max(0.0);
    let encoded = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb, Rgba};

    fn noise(index: usize, amplitude: f32) -> f32 {
        (mix64(index as u64) as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32 * amplitude
    }
    fn scene(size: u32, base: f32, amplitude: f32) -> Rgb32FImage {
        Rgb32FImage::from_fn(size, size, |x, y| {
            let index = ((y * size + x) * 3) as usize;
            Rgb(std::array::from_fn(|c| base + noise(index + c, amplitude)))
        })
    }
    fn mask(size: u32) -> (GrayImage, NativeRect) {
        let bounds = NativeRect {
            x: size / 3,
            y: size / 3,
            width: 32,
            height: 32,
        };
        let mask = GrayImage::from_fn(size, size, |x, y| {
            Luma([
                if x >= bounds.x && x < bounds.right() && y >= bounds.y && y < bounds.bottom() {
                    255
                } else {
                    0
                },
            ])
        });
        (mask, bounds)
    }

    #[test]
    fn surroundings_never_copy_the_object_and_restore_native_noise_deterministically() {
        let size = 160;
        let mut scene = scene(size, 0.2, 0.05);
        let (mask, bounds) = mask(size);
        for (x, y, value) in scene.enumerate_pixels_mut() {
            if mask.get_pixel(x, y)[0] != 0 {
                *value = Rgb([0.9; 3]);
            }
        }
        let grain = Grain::sample(&scene, &mask, bounds).unwrap();
        for &(x, y) in &grain.sources {
            assert!(
                x + GRAIN_BLOCK <= bounds.x.saturating_sub(8)
                    || x >= bounds.right() + 8
                    || y + GRAIN_BLOCK <= bounds.y.saturating_sub(8)
                    || y >= bounds.bottom() + 8
            );
        }
        let mut fill = image::RgbaImage::from_pixel(
            size,
            size,
            Rgba([encode(0.2), encode(0.2), encode(0.2), 255]),
        );
        let mut again = fill.clone();
        restore_grain(&scene, &mask, &mut fill, bounds);
        restore_grain(&scene, &mask, &mut again, bounds);
        assert_eq!(fill, again);
        let mut sum = 0.0;
        let mut squares = 0.0;
        for y in bounds.y..bounds.bottom() {
            for x in bounds.x..bounds.right() {
                let value = decode(fill.get_pixel(x, y)[1]) - 0.2;
                sum += value;
                squares += value.powi(2);
            }
        }
        let deviation = (squares / 1024.0 - (sum / 1024.0).powi(2)).sqrt();
        assert!((0.015..0.035).contains(&deviation), "{deviation}");
        assert!((sum / 1024.0).abs() < 0.01);
    }

    #[test]
    fn clean_flat_fills_are_unchanged_and_bad_or_small_sources_are_skipped() {
        let (mask, bounds) = mask(160);
        let flat = Rgb32FImage::from_pixel(160, 160, Rgb([0.0; 3]));
        let mut fill = image::RgbaImage::from_pixel(160, 160, Rgba([0, 0, 0, 255]));
        let original = fill.clone();
        restore_grain(&flat, &mask, &mut fill, bounds);
        assert_eq!(fill, original);
        let small = scene(8, 0.2, 0.05);
        assert!(Grain::sample(&small, &GrayImage::new(8, 8), bounds).is_none());
        let mut bad = flat;
        bad.put_pixel(0, 0, Rgb([f32::NAN; 3]));
        assert!(Grain::sample(&bad, &mask, bounds).is_none());
    }

    #[test]
    fn thin_lines_and_leds_are_not_sources_even_with_low_average_energy() {
        let size = 192;
        let (mask, bounds) = mask(size);
        let mut scene = scene(size, 0.2, 0.01);
        // Thin high contrast red LED and hair-like line, outside the object.
        for y in 0..size {
            scene.put_pixel(16, y, Rgb([0.5, 0.2, 0.2]));
        }
        scene.put_pixel(120, 16, Rgb([1.0, 0.0, 0.0]));
        let grain = Grain::sample(&scene, &mask, bounds).unwrap();
        for &(x, y) in &grain.sources {
            assert!(!(x <= 16 && x + GRAIN_BLOCK > 16));
            assert!(!(x <= 120 && x + GRAIN_BLOCK > 120 && y <= 16 && y + GRAIN_BLOCK > 16));
        }
    }

    #[test]
    fn source_material_matches_nearby_brightness() {
        let size = 256;
        let (mask, bounds) = mask(size);
        let mut scene = scene(size, 0.2, 0.03);
        for (x, y, pixel) in scene.enumerate_pixels_mut() {
            if x > 192 {
                *pixel = Rgb([0.8 + noise((y * size + x) as usize, 0.03); 3]);
            }
        }
        let grain = Grain::sample(&scene, &mask, bounds).unwrap();
        assert!(grain.sources.iter().all(|&(x, _)| x < 192));
        assert!(grain.sources.len() <= GRAIN_MAX_SOURCES);
    }

    #[test]
    fn restores_only_native_mask_pixels_before_display_encoding() {
        let size = 160;
        let scene = scene(size, 0.02, 0.006);
        let (mask, bounds) = mask(size);
        let mut fill = image::RgbaImage::from_pixel(size, size, Rgba([encode(0.02); 4]));
        let original = fill.clone();
        restore_grain(&scene, &mask, &mut fill, bounds);
        assert_ne!(fill, original);
        for (x, y, pixel) in fill.enumerate_pixels() {
            assert_eq!(pixel[3], original.get_pixel(x, y)[3]);
            if mask.get_pixel(x, y)[0] == 0 {
                assert_eq!(pixel, original.get_pixel(x, y));
            }
        }
    }
}
