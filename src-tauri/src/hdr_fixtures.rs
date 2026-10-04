//! Synthetic exposure brackets with known ground truth, for testing HDR
//! alignment, deghosting and merging (see `rapidroom/HDR.md`).
//!
//! A bracket is made from a linear scene: each frame scales the scene by its
//! EV, shifts it by a sub-pixel camera offset, composites a moving disc, adds
//! shot and read noise, then clips and quantises like a sensor. The scene is
//! either procedural (`synthetic_scene`) or a decoded CC0 raw
//! (`write_bracket_from_raw`, an ignored test that writes TIFFs to disk).

use image::{GrayImage, Luma, Rgb, Rgb32FImage};
use std::time::Duration;

pub struct MovingDisc {
    pub radius: f32,
    /// Linear radiance of the disc at EV 0.
    pub color: [f32; 3],
    /// Centre in scene coordinates in the first frame.
    pub start: (f32, f32),
    /// Movement per frame, in pixels.
    pub step: (f32, f32),
}

pub struct BracketSpec {
    /// EV offset of each frame relative to the reference exposure, in capture order.
    pub evs: Vec<f32>,
    /// Camera offset of each frame in pixels: frame(x, y) = scene(x - dx, y - dy).
    pub shifts: Vec<(f32, f32)>,
    pub reference_index: usize,
    pub base_exposure: Duration,
    pub iso: f32,
    /// Sensor clip level after exposure scaling.
    pub white_level: f32,
    /// Electrons at `white_level`; sets the shot noise. 0 disables shot noise.
    pub full_well: f32,
    /// Read noise as a fraction of `white_level`.
    pub read_noise: f32,
    /// Quantisation bit depth; 0 disables quantisation.
    pub bit_depth: u32,
    pub mover: Option<MovingDisc>,
    pub seed: u64,
}

impl BracketSpec {
    /// A handheld −2 / 0 / +2 EV bracket with a small drift and a moving disc.
    pub fn handheld(width: u32, height: u32) -> Self {
        let (w, h) = (width as f32, height as f32);
        BracketSpec {
            evs: vec![-2.0, 0.0, 2.0],
            shifts: vec![(-3.25, 1.5), (0.0, 0.0), (2.5, -2.75)],
            reference_index: 1,
            base_exposure: Duration::from_micros(8_000),
            iso: 100.0,
            white_level: 1.0,
            full_well: 30_000.0,
            read_noise: 0.0005,
            bit_depth: 14,
            mover: Some(MovingDisc {
                radius: h * 0.06,
                color: [0.30, 0.08, 0.04],
                start: (w * 0.30, h * 0.65),
                step: (w * 0.12, 0.0),
            }),
            seed: 0x5eed,
        }
    }
}

pub struct SyntheticFrame {
    pub image: Rgb32FImage,
    pub ev: f32,
    pub exposure: Duration,
    pub iso: f32,
    pub shift: (f32, f32),
    /// 255 where any channel reached `white_level`.
    pub clipped: GrayImage,
}

pub struct SyntheticBracket {
    pub frames: Vec<SyntheticFrame>,
    pub reference_index: usize,
    /// Noise-free, unclipped radiance at EV 0 in the reference frame's
    /// coordinates, with the disc where the reference frame shows it.
    pub truth: Rgb32FImage,
    /// 255 wherever the disc appears in any frame once that frame is aligned
    /// to the reference: the region a deghoster must take from one frame.
    pub motion_mask: GrayImage,
}

/// A deterministic scene with ~16 stops of range: a horizontal log ramp,
/// textured rectangles for feature matching, colour patches and a small
/// very bright "sun".
pub fn synthetic_scene(width: u32, height: u32, seed: u64) -> Rgb32FImage {
    let mut rng = Rng::new(seed);
    let (w, h) = (width as f32, height as f32);
    let rects: Vec<([f32; 4], [f32; 3])> = (0..48)
        .map(|_| {
            let x0 = rng.uniform() * w;
            let y0 = rng.uniform() * h;
            let rw = (0.02 + rng.uniform() * 0.08) * w;
            let rh = (0.02 + rng.uniform() * 0.08) * h;
            let gain = 2f32.powf(rng.uniform() * 4.0 - 2.0);
            let tint = [
                0.6 + rng.uniform() * 0.8,
                0.6 + rng.uniform() * 0.8,
                0.6 + rng.uniform() * 0.8,
            ];
            (
                [x0, y0, x0 + rw, y0 + rh],
                [gain * tint[0], gain * tint[1], gain * tint[2]],
            )
        })
        .collect();
    let sun = (w * 0.82, h * 0.2, h * 0.05);
    Rgb32FImage::from_fn(width, height, |x, y| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let stops = -12.0 + 14.0 * fx / w;
        let ripple = 1.0 + 0.25 * ((fx * 0.21).sin() * (fy * 0.17).cos());
        let mut v = [2f32.powf(stops) * ripple; 3];
        for (r, gain) in &rects {
            if fx >= r[0] && fx < r[2] && fy >= r[1] && fy < r[3] {
                for c in 0..3 {
                    v[c] *= gain[c];
                }
            }
        }
        let d = ((fx - sun.0).powi(2) + (fy - sun.1).powi(2)).sqrt();
        let sun_cover = coverage(d, sun.2);
        for (c, s) in [16.0, 15.0, 12.0].iter().enumerate() {
            v[c] = v[c] * (1.0 - sun_cover) + s * sun_cover;
        }
        Rgb(v)
    })
}

pub fn make_bracket(scene: &Rgb32FImage, spec: &BracketSpec) -> SyntheticBracket {
    assert_eq!(spec.evs.len(), spec.shifts.len(), "one shift per frame");
    assert!(spec.reference_index < spec.evs.len());
    let (width, height) = scene.dimensions();
    let mut rng = Rng::new(spec.seed);
    let ref_shift = spec.shifts[spec.reference_index];
    let disc_at = |i: usize| {
        spec.mover.as_ref().map(|m| {
            (
                m.start.0 + m.step.0 * i as f32,
                m.start.1 + m.step.1 * i as f32,
                m,
            )
        })
    };

    let frames = spec
        .evs
        .iter()
        .zip(&spec.shifts)
        .enumerate()
        .map(|(i, (&ev, &shift))| {
            let gain = 2f32.powf(ev);
            let disc = disc_at(i);
            let mut clipped = GrayImage::new(width, height);
            let image = Rgb32FImage::from_fn(width, height, |x, y| {
                let (sx, sy) = (x as f32 + 0.5 - shift.0, y as f32 + 0.5 - shift.1);
                let mut v = sample_bilinear(scene, sx, sy);
                if let Some((cx, cy, m)) = disc {
                    paint_disc(&mut v, sx, sy, cx, cy, m);
                }
                let mut any_clipped = false;
                for value in v.iter_mut() {
                    *value = sensor(*value * gain, spec, &mut rng);
                    any_clipped |= *value >= spec.white_level;
                }
                if any_clipped {
                    clipped.put_pixel(x, y, Luma([255]));
                }
                Rgb(v)
            });
            SyntheticFrame {
                image,
                ev,
                exposure: spec.base_exposure.mul_f32(gain),
                iso: spec.iso,
                shift,
                clipped,
            }
        })
        .collect();

    let ref_disc = disc_at(spec.reference_index);
    let truth = Rgb32FImage::from_fn(width, height, |x, y| {
        let (sx, sy) = (x as f32 + 0.5 - ref_shift.0, y as f32 + 0.5 - ref_shift.1);
        let mut v = sample_bilinear(scene, sx, sy);
        if let Some((cx, cy, m)) = ref_disc {
            paint_disc(&mut v, sx, sy, cx, cy, m);
        }
        Rgb(v)
    });

    // Aligning frame i to the reference maps scene point p to p + ref_shift,
    // so the disc of every frame lands at its scene centre + ref_shift.
    let mut motion_mask = GrayImage::new(width, height);
    if spec.mover.is_some() {
        for i in 0..spec.evs.len() {
            let (cx, cy, m) = disc_at(i).unwrap();
            let (cx, cy) = (cx + ref_shift.0, cy + ref_shift.1);
            for (x, y, p) in motion_mask.enumerate_pixels_mut() {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d <= m.radius + 1.0 {
                    p.0[0] = 255;
                }
            }
        }
    }

    SyntheticBracket {
        frames,
        reference_index: spec.reference_index,
        truth,
        motion_mask,
    }
}

/// Mean absolute log2 error (in stops) against `truth` where `mask` equals
/// `select` (255 or 0), skipping near-black truth. Merge tests compare in
/// stops so shadows count as much as highlights.
pub fn mean_stop_error(
    merged: &Rgb32FImage,
    truth: &Rgb32FImage,
    mask: &GrayImage,
    select: u8,
) -> f32 {
    let floor = 1e-4;
    let (mut sum, mut n) = (0.0f64, 0usize);
    for ((m, t), k) in merged.pixels().zip(truth.pixels()).zip(mask.pixels()) {
        if k.0[0] != select {
            continue;
        }
        for c in 0..3 {
            if t[c] > floor {
                sum += (m[c].max(floor) / t[c]).log2().abs() as f64;
                n += 1;
            }
        }
    }
    if n == 0 { 0.0 } else { (sum / n as f64) as f32 }
}

fn sensor(signal: f32, spec: &BracketSpec, rng: &mut Rng) -> f32 {
    let mut v = signal;
    if spec.full_well > 0.0 {
        let electrons = (v / spec.white_level * spec.full_well).max(0.0);
        v += rng.gaussian() * electrons.sqrt() / spec.full_well * spec.white_level;
    }
    v += rng.gaussian() * spec.read_noise * spec.white_level;
    v = v.clamp(0.0, spec.white_level);
    if spec.bit_depth > 0 {
        let steps = ((1u64 << spec.bit_depth) - 1) as f32;
        v = (v / spec.white_level * steps).round() / steps * spec.white_level;
    }
    v
}

fn paint_disc(v: &mut [f32; 3], x: f32, y: f32, cx: f32, cy: f32, disc: &MovingDisc) {
    let cover = coverage(((x - cx).powi(2) + (y - cy).powi(2)).sqrt(), disc.radius);
    for (value, color) in v.iter_mut().zip(disc.color) {
        *value = *value * (1.0 - cover) + color * cover;
    }
}

fn coverage(distance: f32, radius: f32) -> f32 {
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

fn sample_bilinear(image: &Rgb32FImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = image.dimensions();
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let p = |px, py| image.get_pixel(px, py).0;
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    let mut out = [0.0; 3];
    for i in 0..3 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bottom = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bottom - top) * ty;
    }
    out
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    fn uniform(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 + 0.5) / (1u64 << 24) as f32
    }

    fn gaussian(&mut self) -> f32 {
        let u1 = self.uniform();
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bracket(width: u32, height: u32) -> SyntheticBracket {
        make_bracket(
            &synthetic_scene(width, height, 7),
            &BracketSpec::handheld(width, height),
        )
    }

    #[test]
    fn is_deterministic() {
        let a = bracket(96, 64);
        let b = bracket(96, 64);
        for (fa, fb) in a.frames.iter().zip(&b.frames) {
            assert_eq!(fa.image.as_raw(), fb.image.as_raw());
        }
    }

    #[test]
    fn exposure_metadata_follows_ev() {
        let b = bracket(64, 48);
        let base = b.frames[b.reference_index].exposure.as_secs_f32();
        for f in &b.frames {
            let ratio = f.exposure.as_secs_f32() / base;
            assert!(
                (ratio.log2() - f.ev).abs() < 1e-3,
                "ev {} ratio {}",
                f.ev,
                ratio
            );
            assert_eq!(f.iso, 100.0);
        }
    }

    #[test]
    fn frames_clip_at_white_and_brighter_frames_clip_more() {
        let b = bracket(160, 96);
        let counts: Vec<usize> = b
            .frames
            .iter()
            .map(|f| f.clipped.pixels().filter(|p| p.0[0] == 255).count())
            .collect();
        assert!(counts[0] > 0, "the sun must clip even at -2 EV");
        assert!(counts[0] < counts[1] && counts[1] < counts[2], "{counts:?}");
        for f in &b.frames {
            assert!(f.image.pixels().all(|p| p.0.iter().all(|&v| v <= 1.0)));
        }
    }

    #[test]
    fn scene_spans_more_range_than_one_frame() {
        let scene = synthetic_scene(160, 96, 7);
        let lum: Vec<f32> = scene.pixels().map(|p| p[1]).collect();
        let max = lum.iter().cloned().fold(0.0, f32::max);
        let min = lum.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            (max / min).log2() > 14.0,
            "range {:.1} stops",
            (max / min).log2()
        );
    }

    #[test]
    fn exposure_normalised_frames_agree_with_truth_outside_motion() {
        // Without noise or shift, every unclipped, unmoved pixel divided by
        // its gain must reproduce the truth up to quantisation.
        let (w, h) = (128, 80);
        let mut spec = BracketSpec::handheld(w, h);
        spec.shifts = vec![(0.0, 0.0); 3];
        spec.full_well = 0.0;
        spec.read_noise = 0.0;
        let b = make_bracket(&synthetic_scene(w, h, 3), &spec);
        for f in &b.frames {
            let gain = 2f32.powf(f.ev);
            for (x, y, p) in f.image.enumerate_pixels() {
                if b.motion_mask.get_pixel(x, y).0[0] == 255
                    || f.clipped.get_pixel(x, y).0[0] == 255
                {
                    continue;
                }
                let t = b.truth.get_pixel(x, y);
                for c in 0..3 {
                    let err = (p[c] / gain - t[c]).abs();
                    assert!(err <= 0.5 / 16383.0 / gain + 1e-6, "ev {} at {x},{y}", f.ev);
                }
            }
        }
    }

    #[test]
    fn shift_moves_content() {
        let (w, h) = (128, 80);
        let mut spec = BracketSpec::handheld(w, h);
        spec.evs = vec![0.0, 0.0];
        spec.shifts = vec![(0.0, 0.0), (4.0, 2.0)];
        spec.reference_index = 0;
        spec.full_well = 0.0;
        spec.read_noise = 0.0;
        spec.bit_depth = 0;
        spec.mover = None;
        let b = make_bracket(&synthetic_scene(w, h, 3), &spec);
        assert_eq!(b.frames[1].shift, (4.0, 2.0));
        let (a, s) = (&b.frames[0].image, &b.frames[1].image);
        for y in 10..h - 10 {
            for x in 10..w - 10 {
                assert_eq!(a.get_pixel(x, y), s.get_pixel(x + 4, y + 2));
            }
        }
    }

    #[test]
    fn motion_mask_covers_every_disc_position() {
        let b = bracket(160, 96);
        let covered = b.motion_mask.pixels().filter(|p| p.0[0] == 255).count();
        let disc = std::f32::consts::PI * (96.0f32 * 0.06).powi(2);
        assert!(
            covered as f32 > 2.5 * disc,
            "{covered} px vs disc {disc:.0} px"
        );
        assert!(mean_stop_error(&b.truth, &b.truth, &b.motion_mask, 0) == 0.0);
    }

    /// Writes a bracket built from a decoded raw (e.g. a CC0 file from
    /// `rapidroom/regression/corpus.json`) as 32-bit float TIFFs, plus the
    /// truth and motion mask, for looking at in an image viewer:
    ///
    /// RAPIDROOM_HDR_FIXTURE_RAW=<raw> RAPIDROOM_HDR_FIXTURE_OUT=<dir> \
    ///   cargo test --lib hdr_fixtures::tests::write_bracket_from_raw -- --ignored
    #[test]
    #[ignore]
    fn write_bracket_from_raw() {
        let (Ok(raw), Ok(out)) = (
            std::env::var("RAPIDROOM_HDR_FIXTURE_RAW"),
            std::env::var("RAPIDROOM_HDR_FIXTURE_OUT"),
        ) else {
            panic!("set RAPIDROOM_HDR_FIXTURE_RAW and RAPIDROOM_HDR_FIXTURE_OUT");
        };
        let bytes = std::fs::read(&raw).unwrap();
        let developed =
            crate::raw_processing::develop_raw_image(&bytes, true, 2.5, "auto".into(), None)
                .unwrap();
        let scene = developed.to_rgb32f();
        let (w, h) = scene.dimensions();
        let mut spec = BracketSpec::handheld(w, h);
        // The source raw is the EV 0 truth; its own clipped areas stay flat.
        spec.white_level = 1.0;
        let bracket = make_bracket(&scene, &spec);
        let out = std::path::Path::new(&out);
        std::fs::create_dir_all(out).unwrap();
        for f in &bracket.frames {
            let name = format!("frame_{:+}ev.tiff", f.ev);
            image::DynamicImage::ImageRgb32F(f.image.clone())
                .save(out.join(name))
                .unwrap();
        }
        image::DynamicImage::ImageRgb32F(bracket.truth)
            .save(out.join("truth.tiff"))
            .unwrap();
        bracket
            .motion_mask
            .save(out.join("motion_mask.png"))
            .unwrap();
    }
}
