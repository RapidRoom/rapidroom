//! Match local Remove fills to the unmasked photo, in the persisted sRGB domain.
use crate::remove_grain::{Grain, NativeRect, gaussian_blur_rgb};
use image::{GrayImage, Rgb32FImage, RgbaImage};

fn ycc(v: [f32; 3]) -> [f32; 3] {
    let y = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
    [y, v[2] - y, v[0] - y]
}
fn rgb(v: [f32; 3]) -> [f32; 3] {
    let r = v[0] + v[2];
    let b = v[0] + v[1];
    [r, (v[0] - 0.2126 * r - 0.0722 * b) / 0.7152, b]
}
#[derive(Default)]
struct Moments {
    n: usize,
    sum: [f64; 3],
    square: [f64; 3],
}
impl Moments {
    fn add(&mut self, v: [f32; 3]) {
        self.n += 1;
        for (c, x) in v.into_iter().enumerate() {
            self.sum[c] += f64::from(x);
            self.square[c] += f64::from(x).powi(2);
        }
    }
    fn variance(&self) -> [f32; 3] {
        std::array::from_fn(|c| {
            if self.n == 0 {
                0.0
            } else {
                (self.square[c] / self.n as f64 - (self.sum[c] / self.n as f64).powi(2)).max(0.0)
                    as f32
            }
        })
    }
}
// A flat colour or smooth gradient can constrain the entire fill, not just
// its edge. Refuse this stronger correction across textured/structured borders.
fn background_plane(points: &[(f32, f32, [f32; 3])]) -> Option<[[f32; 3]; 3]> {
    if points.len() < 8 {
        return None;
    }
    let mut a = [[0.0f64; 6]; 3];
    for &(x, y, rgb) in points {
        let v = [1.0, f64::from(x), f64::from(y)];
        for r in 0..3 {
            for c in 0..3 {
                a[r][c] += v[r] * v[c];
                a[r][c + 3] += v[r] * f64::from(rgb[c]);
            }
        }
    }
    for c in 0..3 {
        let pivot = (c..3).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))?;
        a.swap(c, pivot);
        let scale = a[c][c];
        if scale.abs() < 1e-8 {
            return None;
        }
        for value in &mut a[c][c..] {
            *value /= scale;
        }
        let row = a[c];
        for (r, other) in a.iter_mut().enumerate() {
            if r == c {
                continue;
            }
            let scale = other[c];
            for k in c..6 {
                other[k] -= row[k] * scale;
            }
        }
    }
    let plane = std::array::from_fn(|r| std::array::from_fn(|c| a[r][c + 3] as f32));
    let mut square = [0.0; 3];
    for &(x, y, v) in points {
        for c in 0..3 {
            square[c] += (v[c] - plane[0][c] - plane[1][c] * x - plane[2][c] * y).powi(2);
        }
    }
    square
        .iter()
        .all(|v| (*v / points.len() as f32).sqrt() <= 1.5)
        .then_some(plane)
}
struct ColorField {
    width: usize,
    height: usize,
    bounds: NativeRect,
    values: Vec<[f32; 3]>,
    flat: bool,
}
impl ColorField {
    fn at(&self, x: u32, y: u32) -> [f32; 3] {
        let fx = (x.saturating_sub(self.bounds.x) as f32
            / self.bounds.width.saturating_sub(1).max(1) as f32
            * (self.width - 1) as f32)
            .clamp(0.0, (self.width - 1) as f32);
        let fy = (y.saturating_sub(self.bounds.y) as f32
            / self.bounds.height.saturating_sub(1).max(1) as f32
            * (self.height - 1) as f32)
            .clamp(0.0, (self.height - 1) as f32);
        let ix = fx.floor() as usize;
        let iy = fy.floor() as usize;
        let jx = (ix + 1).min(self.width - 1);
        let jy = (iy + 1).min(self.height - 1);
        let tx = fx - ix as f32;
        let ty = fy - iy as f32;
        std::array::from_fn(|c| {
            (self.values[iy * self.width + ix][c] * (1.0 - tx)
                + self.values[iy * self.width + jx][c] * tx)
                * (1.0 - ty)
                + (self.values[jy * self.width + ix][c] * (1.0 - tx)
                    + self.values[jy * self.width + jx][c] * tx)
                    * ty
        })
    }
}
// Only unmasked source samples guide colour. No original object pixels enter
// this boundary estimate. A <=64x64 field bounds the harmonic solve's memory.
fn color_field(
    source: &[f32],
    mask: &GrayImage,
    fill: &RgbaImage,
    bounds: NativeRect,
) -> ColorField {
    let width = (bounds.width as usize).clamp(2, 64);
    let height = (bounds.height as usize).clamp(2, 64);
    let mut values = vec![[0.0; 3]; width * height];
    let mut active = vec![false; width * height];
    let mut fixed = vec![false; width * height];
    let coordinate = |x: usize, y: usize| {
        (
            bounds.x
                + (x as f32 / (width - 1) as f32 * bounds.width.saturating_sub(1) as f32).round()
                    as u32,
            bounds.y
                + (y as f32 / (height - 1) as f32 * bounds.height.saturating_sub(1) as f32).round()
                    as u32,
        )
    };
    for y in 0..height {
        for x in 0..width {
            let (px, py) = coordinate(x, y);
            active[y * width + x] = mask.get_pixel(px, py)[0] != 0;
        }
    }
    let mut mean = [0.0; 3];
    let mut anchors = 0;
    let mut guides = Vec::new();
    let mut model = vec![[0.0; 3]; width * height];
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            if !active[i] {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == width
                || y + 1 == height
                || !active[i - 1]
                || !active[i + 1]
                || !active[i - width]
                || !active[i + width];
            let (px, py) = coordinate(x, y);
            let mut outside = [0.0; 3];
            let mut inside = [0.0; 3];
            let mut ow = 0.0;
            let mut iw = 0.0;
            for dy in -8i32..=8 {
                for dx in -8i32..=8 {
                    let sx = px as i32 + dx;
                    let sy = py as i32 + dy;
                    if sx < 0 || sy < 0 || sx >= mask.width() as i32 || sy >= mask.height() as i32 {
                        continue;
                    }
                    let (sx, sy) = (sx as u32, sy as u32);
                    let weight = (-((dx * dx + dy * dy) as f32) / 18.0).exp();
                    if mask.get_pixel(sx, sy)[0] == 0 {
                        let j = (sy as usize * mask.width() as usize + sx as usize) * 3;
                        for c in 0..3 {
                            outside[c] += source[j + c] * weight;
                        }
                        ow += weight;
                    } else {
                        for (c, v) in fill.get_pixel(sx, sy).0[..3].iter().enumerate() {
                            inside[c] += f32::from(*v) * weight;
                        }
                        iw += weight;
                    }
                }
            }
            if iw > 1e-6 {
                model[i] = inside.map(|v| v / iw);
            }
            if edge && ow > 1e-6 && iw > 1e-6 {
                guides.push((
                    x as f32 / (width - 1) as f32,
                    y as f32 / (height - 1) as f32,
                    outside.map(|v| v / ow),
                ));
                fixed[i] = true;
                anchors += 1;
                for c in 0..3 {
                    values[i][c] = (outside[c] / ow - inside[c] / iw).clamp(-32.0, 32.0);
                    mean[c] += values[i][c];
                }
            }
        }
    }
    if let Some(plane) = background_plane(&guides) {
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                if active[i] {
                    for c in 0..3 {
                        values[i][c] = (plane[0][c]
                            + plane[1][c] * x as f32 / (width - 1) as f32
                            + plane[2][c] * y as f32 / (height - 1) as f32
                            - model[i][c])
                            .clamp(-32.0, 32.0);
                    }
                }
            }
        }
        return ColorField {
            width,
            height,
            bounds,
            values,
            flat: true,
        };
    }
    if anchors > 0 {
        for v in &mut mean {
            *v /= anchors as f32;
        }
        for i in 0..values.len() {
            if active[i] && !fixed[i] {
                values[i] = mean;
            }
        }
    }
    let mut next = values.clone();
    for _ in 0..2048 {
        let mut change = 0.0f32;
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                if !active[i] || fixed[i] {
                    continue;
                }
                let mut sum = [0.0; 3];
                let mut n = 0;
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if nx < width && ny < height && active[ny * width + nx] {
                        n += 1;
                        for (c, v) in sum.iter_mut().enumerate() {
                            *v += values[ny * width + nx][c];
                        }
                    }
                }
                if n > 0 {
                    for (c, v) in sum.into_iter().enumerate() {
                        next[i][c] = v / n as f32;
                        change = change.max((next[i][c] - values[i][c]).abs());
                    }
                }
            }
        }
        std::mem::swap(&mut values, &mut next);
        if change < 0.01 {
            break;
        }
    }
    ColorField {
        width,
        height,
        bounds,
        values,
        flat: false,
    }
}

pub(crate) fn harmonize(
    scene: &Rgb32FImage,
    mask: &GrayImage,
    fill: &mut RgbaImage,
    bounds: NativeRect,
    grain: Option<&Grain>,
) {
    let (w, h) = (scene.width() as usize, scene.height() as usize);
    let source = scene
        .as_raw()
        .iter()
        .map(|v| f32::from(crate::remove_grain::encode(*v)))
        .collect::<Vec<_>>();
    let mut pool = Vec::new();
    let mut target = Moments::default();
    if let Some(grain) = grain {
        let low = gaussian_blur_rgb(&source, w, h, 2.5);
        for &(left, top) in &grain.sources {
            let start = pool.len();
            let mut mean = [0.0; 3];
            for y in top..top + 16 {
                for x in left..left + 16 {
                    let i = (y as usize * w + x as usize) * 3;
                    let v = ycc(std::array::from_fn(|c| source[i + c] - low[i + c]));
                    pool.push(v);
                    for c in 0..3 {
                        mean[c] += v[c] / 256.0;
                    }
                }
            }
            // Zero each sampled tile's mean so changing tiles cannot paint
            // sixteen-pixel colour blocks onto an otherwise smooth fill.
            for v in &mut pool[start..] {
                for c in 0..3 {
                    v[c] -= mean[c];
                }
                target.add(*v);
            }
        }
    }
    let field = color_field(&source, mask, fill, bounds);
    drop(source);
    let mut existing = Moments::default();
    let mut model_low = None;
    if !pool.is_empty() {
        let model = fill
            .pixels()
            .flat_map(|p| p.0[..3].iter().map(|v| f32::from(*v)))
            .collect::<Vec<_>>();
        let low = gaussian_blur_rgb(&model, w, h, 2.5);
        for (x, y, p) in fill.enumerate_pixels() {
            if mask.get_pixel(x, y)[0] != 255 {
                continue;
            }
            // Avoid counting the outer seam as model noise.
            let core = x >= bounds.x + 8
                && y >= bounds.y + 8
                && x + 8 < bounds.x + bounds.width
                && y + 8 < bounds.y + bounds.height;
            if !core && bounds.width > 16 && bounds.height > 16 {
                continue;
            }
            let i = (y as usize * w + x as usize) * 3;
            existing.add(ycc(std::array::from_fn(|c| f32::from(p[c]) - low[i + c])));
        }
        model_low = Some(low);
    }
    let target = target.variance();
    let existing_samples = existing.n;
    let existing = existing.variance();
    let attenuation: [f32; 3] = std::array::from_fn(|c| {
        if field.flat && existing[c] > target[c] && existing[c] > 1e-8 {
            (target[c] / existing[c]).sqrt()
        } else {
            1.0
        }
    });
    let gain: [f32; 3] = std::array::from_fn(|c| {
        if existing_samples == 0 {
            0.0
        } else if target[c] > 1e-8 {
            ((target[c] - attenuation[c] * attenuation[c] * existing[c]).max(0.0) / target[c])
                .sqrt()
        } else {
            0.0
        }
    });
    for (x, y, pixel) in fill.enumerate_pixels_mut() {
        if mask.get_pixel(x, y)[0] == 0 {
            continue;
        }
        let correction = field.at(x, y);
        let removal = if let Some(low) = model_low.as_ref() {
            let i = (y as usize * w + x as usize) * 3;
            let high = ycc(std::array::from_fn(|c| f32::from(pixel[c]) - low[i + c]));
            rgb(std::array::from_fn(|c| high[c] * (attenuation[c] - 1.0)))
        } else {
            [0.0; 3]
        };
        let noise = if let Some(grain) = grain.filter(|_| !pool.is_empty()) {
            let v = pool[grain.donor_index(x, y)];
            rgb(std::array::from_fn(|c| v[c] * gain[c]))
        } else {
            [0.0; 3]
        };
        for c in 0..3 {
            pixel[c] = (f32::from(pixel[c]) + correction[c] + noise[c] + removal[c])
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb, Rgba};
    fn linear(v: f32) -> f32 {
        let v = v / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    fn noise(i: u64, amplitude: f32) -> f32 {
        (crate::remove_grain::mix64(i) as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32 * amplitude
    }
    #[test]
    fn biased_flat_fill_matches_context_without_copying_the_removed_object() {
        let size = 96;
        let bounds = NativeRect {
            x: 32,
            y: 32,
            width: 32,
            height: 32,
        };
        let mask = GrayImage::from_fn(size, size, |x, y| {
            Luma([if (32..64).contains(&x) && (32..64).contains(&y) {
                255
            } else {
                0
            }])
        });
        let scene = Rgb32FImage::from_fn(size, size, |x, y| {
            if mask.get_pixel(x, y)[0] > 0 {
                Rgb([0.9, 0.1, 0.1])
            } else {
                Rgb([linear(70.0), linear(90.0), linear(110.0)])
            }
        });
        let mut fill = RgbaImage::from_pixel(size, size, Rgba([65, 82, 107, 231]));
        let before = fill.clone();
        crate::remove_grain::restore_grain(&scene, &mask, &mut fill, bounds);
        for (x, y, p) in fill.enumerate_pixels() {
            if mask.get_pixel(x, y)[0] > 0 {
                for (c, target) in [70i16, 90, 110].into_iter().enumerate() {
                    assert!((i16::from(p[c]) - target).abs() <= 1, "{p:?}");
                }
                assert_eq!(p[3], 231);
            } else {
                assert_eq!(p, before.get_pixel(x, y));
            }
        }
    }
    #[test]
    fn existing_model_luminance_noise_is_not_doubled_when_chroma_is_restored() {
        let size = 192;
        let bounds = NativeRect {
            x: 64,
            y: 64,
            width: 64,
            height: 64,
        };
        let mask = GrayImage::from_fn(size, size, |x, y| {
            Luma([if (64..128).contains(&x) && (64..128).contains(&y) {
                255
            } else {
                0
            }])
        });
        let scene = Rgb32FImage::from_fn(size, size, |x, y| {
            let i = u64::from(y * size + x) * 3;
            let v = rgb([noise(i, 12.0), noise(i + 1, 22.0), noise(i + 2, 18.0)]);
            Rgb(v.map(|v| linear(100.0 + v)))
        });
        let mut fill = RgbaImage::from_fn(size, size, |x, y| {
            let v = (100.0 + noise(u64::from(y * size + x) + 999_999, 12.0)).round() as u8;
            Rgba([v, v, v, 255])
        });
        let before = fill.clone();
        let mut again = fill.clone();
        crate::remove_grain::restore_grain(&scene, &mask, &mut fill, bounds);
        crate::remove_grain::restore_grain(&scene, &mask, &mut again, bounds);
        assert_eq!(fill, again);
        let stats = |im: &RgbaImage| {
            let data = im
                .pixels()
                .flat_map(|p| p.0[..3].iter().map(|v| f32::from(*v)))
                .collect::<Vec<_>>();
            let low = gaussian_blur_rgb(&data, size as usize, size as usize, 2.5);
            let mut m = Moments::default();
            for y in 72..120 {
                for x in 72..120 {
                    let i = (y * size as usize + x) * 3;
                    m.add(ycc(std::array::from_fn(|c| data[i + c] - low[i + c])));
                }
            }
            m.variance().map(f32::sqrt)
        };
        let old = stats(&before);
        let new = stats(&fill);
        assert!(new[0] <= old[0] * 1.12, "luma {old:?} -> {new:?}");
        assert!((9.0..15.0).contains(&new[1]), "Cb {new:?}");
        assert!((7.0..13.0).contains(&new[2]), "Cr {new:?}");
        for (x, y, p) in fill.enumerate_pixels() {
            if mask.get_pixel(x, y)[0] == 0 {
                assert_eq!(p, before.get_pixel(x, y));
            }
        }
    }
    #[test]
    fn flat_context_corrects_a_darker_interior_even_when_the_edge_already_matches() {
        let size = 96;
        let bounds = NativeRect {
            x: 32,
            y: 32,
            width: 32,
            height: 32,
        };
        let mask = GrayImage::from_fn(size, size, |x, y| {
            Luma([if (32..64).contains(&x) && (32..64).contains(&y) {
                255
            } else {
                0
            }])
        });
        let scene = Rgb32FImage::from_pixel(
            size,
            size,
            Rgb([linear(80.0), linear(100.0), linear(120.0)]),
        );
        let mut fill = RgbaImage::from_fn(size, size, |x, y| {
            let r = (x as f32 - 47.5).powi(2) + (y as f32 - 47.5).powi(2);
            let dip = (-r / 288.0).exp();
            Rgba([
                (80.0 - 6.0 * dip).round() as u8,
                (100.0 - 4.0 * dip).round() as u8,
                (120.0 - 10.0 * dip).round() as u8,
                255,
            ])
        });
        crate::remove_grain::restore_grain(&scene, &mask, &mut fill, bounds);
        for y in 40..56 {
            for x in 40..56 {
                for (c, target) in [80i16, 100, 120].into_iter().enumerate() {
                    assert!(
                        (i16::from(fill.get_pixel(x, y)[c]) - target).abs() <= 1,
                        "{:?}",
                        fill.get_pixel(x, y)
                    );
                }
            }
        }
    }
    #[test]
    fn one_pixel_edge_selection_has_bounded_field_and_preserves_alpha() {
        let scene = Rgb32FImage::from_pixel(5, 5, Rgb([linear(64.0), linear(88.0), linear(104.0)]));
        let mut mask = GrayImage::new(5, 5);
        mask.put_pixel(0, 0, Luma([255]));
        let mut fill = RgbaImage::from_pixel(5, 5, Rgba([50, 80, 90, 7]));
        crate::remove_grain::restore_grain(
            &scene,
            &mask,
            &mut fill,
            NativeRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        assert_eq!(fill.get_pixel(0, 0), &Rgba([64, 88, 104, 7]));
        assert_eq!(fill.get_pixel(1, 0), &Rgba([50, 80, 90, 7]));
    }

    fn unsupported_mask_noise(image: &RgbaImage, mask: &GrayImage, feather: bool) -> [f32; 3] {
        let data = image
            .pixels()
            .flat_map(|p| p.0[..3].iter().map(|v| f32::from(*v)))
            .collect::<Vec<_>>();
        let low = gaussian_blur_rgb(&data, image.width() as usize, image.height() as usize, 2.5);
        let mut moments = Moments::default();
        for (x, y, pixel) in mask.enumerate_pixels() {
            let m = pixel[0];
            if m == 0 || (feather && m == 255) {
                continue;
            }
            let i = (y as usize * image.width() as usize + x as usize) * 3;
            moments.add(ycc(std::array::from_fn(|c| data[i + c] - low[i + c])));
        }
        assert!(moments.n > 0);
        moments.variance().map(f32::sqrt)
    }

    #[test]
    fn unsupported_model_noise_samples_never_mean_a_noiseless_fill() {
        let size = 192;
        let bounds = NativeRect {
            x: 64,
            y: 64,
            width: 64,
            height: 64,
        };
        let inside = |x, y| (64..128).contains(&x) && (64..128).contains(&y);
        let feather = GrayImage::from_fn(size, size, |x, y| {
            Luma([if inside(x, y) { 128 } else { 0 }])
        });
        let thin = GrayImage::from_fn(size, size, |x, y| {
            let band = (64..128).contains(&x) && ((64..70).contains(&y) || (92..98).contains(&y));
            Luma([if !band {
                0
            } else if y == 64 || y == 69 || y == 92 || y == 97 {
                128
            } else {
                255
            }])
        });
        let disconnected = GrayImage::from_fn(size, size, |x, y| {
            let square = |left, top| (left..left + 8).contains(&x) && (top..top + 8).contains(&y);
            let edge = (x == 64 || x == 71 || x == 120 || x == 127)
                || (y == 64 || y == 71 || y == 120 || y == 127);
            Luma([if square(64, 64) || square(120, 120) {
                if edge { 128 } else { 255 }
            } else {
                0
            }])
        });
        let scene = Rgb32FImage::from_fn(size, size, |x, y| {
            let i = u64::from(y * size + x) * 3;
            let v = rgb([noise(i, 12.0), noise(i + 1, 22.0), noise(i + 2, 18.0)]);
            Rgb(v.map(|v| linear(100.0 + v)))
        });
        for (name, mask) in [
            ("feather-only", feather),
            ("thin separated bands", thin),
            ("disconnected corners", disconnected),
        ] {
            assert!(crate::remove_grain::Grain::sample(&scene, &mask, bounds).is_some());
            let mut fill = RgbaImage::from_fn(size, size, |x, y| {
                let i = u64::from(y * size + x) * 3 + 999_999;
                let v = rgb([noise(i, 12.0), noise(i + 1, 22.0), noise(i + 2, 18.0)]);
                Rgba([v[0] as u8, v[1] as u8, v[2] as u8, 173])
            });
            let before = fill.clone();
            crate::remove_grain::restore_grain(&scene, &mask, &mut fill, bounds);
            for feather_band in [false, true] {
                let old = unsupported_mask_noise(&before, &mask, feather_band);
                let new = unsupported_mask_noise(&fill, &mask, feather_band);
                for c in 0..3 {
                    assert!(
                        new[c] <= old[c] * 1.1 + 0.05,
                        "{name}: feather={feather_band}, component {c}: {old:?} -> {new:?}"
                    );
                }
            }
            for (x, y, pixel) in fill.enumerate_pixels() {
                if mask.get_pixel(x, y)[0] == 0 {
                    assert_eq!(pixel, before.get_pixel(x, y));
                }
            }
        }
    }
}
