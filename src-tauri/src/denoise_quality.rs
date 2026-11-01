//! Adapted from vegidio/open-photo-ai@67f1dd10d17efcb191ce1bb099642e32b2918861
//! imaging/{grid,blend,canvas}.rs and opai/models/upscale/osaka/colorfix.rs (AGPL-3.0).
//! Copyright 2026 Vinicius Egidio. RapidRoom integration and tests by Yojen.

pub(crate) struct Axis {
    pub starts: Vec<usize>,
    pub weights: Vec<Vec<f32>>,
}

impl Axis {
    pub fn new(length: usize, tile: usize, overlap: usize) -> Result<Self, String> {
        if length == 0 || tile == 0 || overlap >= tile {
            return Err("Invalid denoise tile geometry".into());
        }
        let extent = length.min(tile);
        let last = length - extent;
        let mut starts = vec![0];
        while *starts.last().unwrap() < last {
            starts.push((starts.last().unwrap() + tile - overlap).min(last));
        }
        let mut totals = vec![0.0; length];
        let mut weights = Vec::with_capacity(starts.len());
        for (i, &start) in starts.iter().enumerate() {
            let before = if i > 0 {
                starts[i - 1] + extent - start
            } else {
                0
            };
            let after = starts.get(i + 1).map_or(0, |next| start + extent - next);
            let row: Vec<_> = (0..extent)
                .map(|p| ramp(p, before) * ramp(extent - 1 - p, after))
                .collect();
            for (p, &v) in row.iter().enumerate() {
                totals[start + p] += v;
            }
            weights.push(row);
        }
        // The 2D canvas is separable. Normalizing each axis gives the same weighted
        // average without allocating another full-image weight plane.
        for (&start, row) in starts.iter().zip(&mut weights) {
            for (p, v) in row.iter_mut().enumerate() {
                *v /= totals[start + p];
            }
        }
        Ok(Self { starts, weights })
    }
}

// Adapted from Duecki1/CalibRaw@2466ca6662c3fd1e901377c0450b19f5f96d3e85
// crates/calibraw-ai/src/ai_denoise/tiling.rs, GPL-3.0-or-later, Duecki.
pub(crate) fn reflect_index(index: i64, length: usize) -> usize {
    if length <= 1 {
        return 0;
    }
    let period = (length as i64 - 1) * 2;
    let folded = index.rem_euclid(period);
    if folded < length as i64 {
        folded as usize
    } else {
        (period - folded) as usize
    }
}

fn ramp(offset: usize, width: usize) -> f32 {
    if width == 0 || offset >= width {
        return 1.0;
    }
    (0.5 - 0.5 * (std::f64::consts::PI * (offset as f64 + 0.5) / width as f64).cos()) as f32
}

/// Restore the original low frequencies once, after stitching all model tiles.
/// Both images are planar RGB in the same linear or encoded domain; no clipping.
pub(crate) fn restore_colour(
    result: &mut [f32],
    reference: &[f32],
    width: usize,
    height: usize,
    check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let plane = width.checked_mul(height).ok_or("Denoise image too large")?;
    if plane == 0 || plane.checked_mul(3) != Some(result.len()) || result.len() != reference.len() {
        return Err("Invalid denoise colour restoration shape".into());
    }
    let mut model_low = vec![0.0; plane];
    let mut source_low = vec![0.0; plane];
    let mut scratch = vec![0.0; plane];
    for channel in 0..3 {
        check()?;
        let range = channel * plane..(channel + 1) * plane;
        low_frequency(
            &mut model_low,
            &result[range.clone()],
            &mut scratch,
            width,
            height,
            &check,
        )?;
        low_frequency(
            &mut source_low,
            &reference[range.clone()],
            &mut scratch,
            width,
            height,
            &check,
        )?;
        for ((pixel, original), model) in result[range].iter_mut().zip(&source_low).zip(&model_low)
        {
            *pixel += original - model;
        }
    }
    check()
}

// Adapted from skymanbp/autoshade@8f25a8aed6e77b4aa2f1dde64da2c8fbde5a9ed2
// src/render/denoise_grain.rs, MIT, copyright 2026 skymanbp.
pub(crate) fn return_luminance(
    result: &mut [f32],
    reference: &[f32],
    width: usize,
    height: usize,
    share: f32,
    check: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    const REC2020_Y: [f32; 3] = [0.2627, 0.6780, 0.0593];
    let plane = width.checked_mul(height).ok_or("Denoise image too large")?;
    if plane == 0
        || plane.checked_mul(3) != Some(result.len())
        || result.len() != reference.len()
        || !share.is_finite()
        || !(0.0..=1.0).contains(&share)
    {
        return Err("Invalid denoise luminance return".into());
    }
    if share == 0.0 {
        return Ok(());
    }
    for y in 0..height {
        check()?;
        for x in 0..width {
            let i = y * width + x;
            let luminance = |image: &[f32]| {
                image[i] * REC2020_Y[0]
                    + image[plane + i] * REC2020_Y[1]
                    + image[2 * plane + i] * REC2020_Y[2]
            };
            let delta = share * (luminance(reference) - luminance(result));
            for channel in 0..3 {
                result[channel * plane + i] += delta;
            }
        }
    }
    check()
}

fn low_frequency(
    dst: &mut [f32],
    input: &[f32],
    scratch: &mut [f32],
    width: usize,
    height: usize,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    const B3: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    dst.copy_from_slice(input);
    for level in 0..5 {
        let dilation = 1isize << level;
        for y in 0..height {
            check()?;
            for x in 0..width {
                scratch[y * width + x] = B3
                    .iter()
                    .enumerate()
                    .map(|(tap, weight)| {
                        let xx = (x as isize + (tap as isize - 2) * dilation)
                            .clamp(0, width as isize - 1) as usize;
                        weight * dst[y * width + xx]
                    })
                    .sum();
            }
        }
        for y in 0..height {
            check()?;
            let rows: [usize; 5] = std::array::from_fn(|tap| {
                (y as isize + (tap as isize - 2) * dilation).clamp(0, height as isize - 1) as usize
                    * width
            });
            for x in 0..width {
                dst[y * width + x] = B3
                    .iter()
                    .zip(rows)
                    .map(|(weight, row)| weight * scratch[row + x])
                    .sum();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_handles_tiny_images_and_preserves_bayer_phase() {
        for length in [1, 2, 3, 4, 256] {
            for index in -1024..1024 {
                let folded = reflect_index(index, length);
                assert!(folded < length);
                if length > 1 {
                    assert_eq!(folded % 2, index.rem_euclid(2) as usize);
                }
            }
        }
        assert_eq!(
            (-3..7).map(|i| reflect_index(i, 4)).collect::<Vec<_>>(),
            vec![3, 2, 1, 0, 1, 2, 3, 2, 1, 0]
        );
    }

    #[test]
    fn tile_layout_covers_small_odd_and_irregular_images_once() {
        for length in [1, 3, 255, 256, 257, 258, 320, 448, 449, 512, 7028] {
            for overlap in [0, 6, 64, 255] {
                let axis = Axis::new(length, 256, overlap).unwrap();
                assert!(axis.starts.windows(2).all(|p| p[0] < p[1]));
                assert_eq!(axis.starts.last().unwrap() + length.min(256), length);
                let mut sum = vec![0.0f32; length];
                for (&start, weights) in axis.starts.iter().zip(&axis.weights) {
                    for (p, &v) in weights.iter().enumerate() {
                        assert!(v > 0.0 && v.is_finite());
                        sum[start + p] += v;
                    }
                }
                assert!(
                    sum.iter().all(|s| (s - 1.0).abs() < 2e-6),
                    "{length}/{overlap}"
                );
            }
        }
        assert!(Axis::new(0, 256, 64).is_err());
        assert!(Axis::new(10, 256, 256).is_err());
    }

    #[test]
    fn black_image_stays_black_and_tiny_overlap_contributions_are_not_quantized() {
        let mut black = vec![0.0; 3 * 17 * 19];
        restore_colour(&mut black, &vec![0.0; 3 * 17 * 19], 17, 19, || Ok(())).unwrap();
        assert!(black.iter().all(|v| *v == 0.0));
        let axis = Axis::new(513, 256, 64).unwrap();
        let mut output = vec![0.0f32; 513];
        for (&start, weights) in axis.starts.iter().zip(&axis.weights) {
            for (p, &weight) in weights.iter().enumerate() {
                output[start + p] += 1e-8 * weight;
            }
        }
        assert!(output.iter().all(|value| (*value - 1e-8).abs() < 1e-14));
    }

    #[test]
    fn wavelet_restores_colour_without_losing_model_detail() {
        let (w, h) = (193, 131);
        let n = w * h;
        let reference = vec![0.3; 3 * n];
        let mut model: Vec<f32> = (0..3 * n)
            .map(|i| {
                0.3 + (i / n + 1) as f32 * 0.1
                    + if (i % n % w + i % n / w) % 2 == 0 {
                        0.02
                    } else {
                        -0.02
                    }
            })
            .collect();
        restore_colour(&mut model, &reference, w, h, || Ok(())).unwrap();
        for c in 0..3 {
            for y in 64..h - 64 {
                for x in 64..w - 64 {
                    let i = c * n + y * w + x;
                    let expected = 0.3 + if (x + y) % 2 == 0 { 0.02 } else { -0.02 };
                    assert!((model[i] - expected).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn colour_restoration_keeps_constant_edges_and_honours_cancellation() {
        for (w, h) in [(1, 1), (3, 7), (65, 33)] {
            let source = vec![0.15; 3 * w * h];
            let mut model = vec![0.5; source.len()];
            restore_colour(&mut model, &source, w, h, || Ok(())).unwrap();
            assert!(model.iter().all(|p| (p - 0.15).abs() < 1e-6));
            let result = restore_colour(&mut model, &source, w, h, || Err("cancelled".into()));
            assert_eq!(result.unwrap_err(), "cancelled");
        }
        assert!(restore_colour(&mut [0.0; 2], &[0.0; 3], 1, 1, || Ok(())).is_err());
    }

    #[test]
    fn luminance_return_preserves_chroma_and_honours_strength() {
        let (w, h) = (65, 33);
        let n = w * h;
        let clean: Vec<f32> = (0..3 * n)
            .map(|i| 0.1 + (i % 997) as f32 / 2000.0)
            .collect();
        let mut reference = clean.clone();
        for i in 0..n {
            reference[i] += 0.02;
            reference[n + i] -= 0.02 * 0.2627 / 0.6780;
        }
        let mut output = clean.clone();
        return_luminance(&mut output, &reference, w, h, 0.29, || Ok(())).unwrap();
        assert!(output.iter().zip(&clean).all(|(a, b)| (a - b).abs() < 2e-6));

        for i in 0..n {
            reference[i] += 0.01;
            reference[n + i] -= 0.005;
            reference[2 * n + i] += 0.002;
        }
        return_luminance(&mut output, &reference, w, h, 0.29, || Ok(())).unwrap();
        for i in 0..n {
            let expected = 0.29 * (0.01 * 0.2627 - 0.005 * 0.6780 + 0.002 * 0.0593);
            for c in 0..3 {
                let value = output[c * n + i];
                assert!((value - clean[c * n + i] - expected).abs() < 2e-6);
            }
            assert!((output[i] - output[n + i] - (clean[i] - clean[n + i])).abs() < 2e-6);
            assert!(
                (output[2 * n + i] - output[n + i] - (clean[2 * n + i] - clean[n + i])).abs()
                    < 2e-6
            );
        }
        assert!(
            return_luminance(&mut output, &reference, w, h, 1.0, || Err(
                "cancelled".into()
            ))
            .is_err()
        );
        assert!(return_luminance(&mut output, &reference, w, h, f32::NAN, || Ok(())).is_err());
    }

    #[test]
    fn star_field_at_tile_joins_preserves_faint_peak_flux_and_spread() {
        let (w, h) = (513, 321);
        let stars = [(192, 192, 0.006), (256, 128, 0.03), (510, 318, 0.2)];
        let mut source = vec![0.001; w * h];
        for (sx, sy, amplitude) in stars {
            for y in sy - 2..=(sy + 2).min(h - 1) {
                for x in sx - 2..=(sx + 2).min(w - 1) {
                    let r2 =
                        ((x as f32 - sx as f32).powi(2) + (y as f32 - sy as f32).powi(2)) / 2.0;
                    source[y * w + x] += amplitude * (-r2).exp();
                }
            }
        }
        let xs = Axis::new(w, 256, 64).unwrap();
        let ys = Axis::new(h, 256, 64).unwrap();
        let mut output = vec![0.0; source.len()];
        for (yi, &y) in ys.starts.iter().enumerate() {
            for (xi, &x) in xs.starts.iter().enumerate() {
                for (dy, &wy) in ys.weights[yi].iter().enumerate() {
                    for (dx, &wx) in xs.weights[xi].iter().enumerate() {
                        let i = (y + dy) * w + x + dx;
                        output[i] += source[i] * wx * wy;
                    }
                }
            }
        }
        assert!(
            source
                .iter()
                .zip(&output)
                .all(|(a, b)| (a - b).abs() < 1e-7)
        );
        // This is a geometry/control gate, not a claim that any learned model retains stars.
        for (sx, sy, _) in stars {
            assert!((output[sy * w + sx] / source[sy * w + sx] - 1.0).abs() < 1e-6);
            let flux = |pixels: &[f32]| {
                (sy - 2..=(sy + 2).min(h - 1))
                    .flat_map(|y| {
                        (sx - 2..=(sx + 2).min(w - 1)).map(move |x| pixels[y * w + x] - 0.001)
                    })
                    .sum::<f32>()
            };
            assert!((flux(&output) / flux(&source) - 1.0).abs() < 1e-5);
        }
    }
}
