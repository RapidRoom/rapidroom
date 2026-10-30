//! Local measurement harness for #87 (not for commit). Runs the same steps as
//! `merge_hdr` without an AppHandle and dumps linear frames for analysis.
use crate::app_settings::AppSettings;
use crate::exif_processing::{read_exposure_time_secs, read_iso};
use crate::hdr_deghosting::{
    detect_frame_features, estimate_rigid_transform, max_corner_displacement,
};
use crate::image_loader::load_base_image_from_bytes_with_exif_persistence;
use crate::image_processing::apply_linear_to_srgb;
use crate::panorama_utils::{processing, stitching};
use image::{DynamicImage, GenericImageView};
use image_hdr::hdr_merge_images;
use image_hdr::input::HDRInput;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn load(path: &Path, settings: &AppSettings) -> (DynamicImage, Vec<u8>) {
    let bytes = fs::read(path).unwrap();
    // persist_exif = false: never write .rrexif caches beside the NAS originals.
    let image = load_base_image_from_bytes_with_exif_persistence(
        &bytes,
        &path.to_string_lossy(),
        false,
        settings,
        None,
        false,
    )
    .unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
    (image, bytes)
}

/// 2×2 box-downsampled linear RGB as little-endian f32, plus a JSON shape file.
fn dump_half(image: &DynamicImage, out: &Path, name: &str) {
    let rgb = image.to_rgb32f();
    let (w, h) = rgb.dimensions();
    let (hw, hh) = (w / 2, h / 2);
    let raw = rgb.as_raw();
    let mut data = Vec::with_capacity((hw * hh * 3) as usize * 4);
    for y in 0..hh {
        for x in 0..hw {
            for c in 0..3 {
                let mut sum = 0.0f32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = (((2 * y + dy) * w + 2 * x + dx) * 3 + c) as usize;
                    sum += raw[i];
                }
                data.extend_from_slice(&(sum / 4.0).to_le_bytes());
            }
        }
    }
    fs::write(out.join(format!("{name}.f32")), data).unwrap();
    fs::write(
        out.join(format!("{name}.json")),
        format!("{{\"width\":{hw},\"height\":{hh},\"channels\":3}}"),
    )
    .unwrap();
}

#[test]
#[ignore = "set HDR_DIR, HDR_OUT and HDR_SETS"]
fn hdr_measure() {
    let dir = PathBuf::from(std::env::var("HDR_DIR").unwrap());
    let out_root = PathBuf::from(std::env::var("HDR_OUT").unwrap());
    let sets = std::env::var("HDR_SETS").unwrap();
    let settings = AppSettings::default();
    let mut log = fs::File::create(out_root.join("harness.log")).unwrap();
    for set in sets.split(';') {
        let names: Vec<&str> = set.split(',').collect();
        let out = out_root.join(names[0]);
        fs::create_dir_all(&out).unwrap();
        let started = Instant::now();
        let mut frames = Vec::new();
        for name in &names {
            let path = dir.join(format!("{name}.arw"));
            let (image, bytes) = load(&path, &settings);
            let path_str = path.to_string_lossy().to_string();
            let iso = read_iso(&path_str, &bytes).unwrap();
            let exposure = read_exposure_time_secs(&path_str, &bytes).unwrap();
            writeln!(
                log,
                "{name}: {}x{} iso {iso} exposure {exposure}",
                image.width(),
                image.height()
            )
            .unwrap();
            dump_half(&image, &out, &format!("{name}-input"));
            frames.push((path_str, image, Duration::from_secs_f32(exposure), iso as f32));
        }
        let load_time = started.elapsed();

        // Same steps as align_hdr_frames, recording each transform.
        let started = Instant::now();
        let brief_pairs = processing::generate_brief_pairs();
        let reference = frames.len() / 2;
        let detections: Vec<_> = frames
            .iter()
            .map(|frame| detect_frame_features(&frame.1, &brief_pairs, true))
            .collect();
        for index in 0..frames.len() {
            if index == reference {
                continue;
            }
            let matches =
                processing::match_features(&detections[reference].features, &detections[index].features);
            let outcome = if matches.len() < processing::MIN_INLIERS_FOR_CONNECTION {
                format!("failed: {} matches", matches.len())
            } else if let Some((_, inliers)) = processing::find_homography_ransac(
                &matches,
                &detections[reference].keypoints,
                &detections[index].keypoints,
            ) {
                let rigid = estimate_rigid_transform(&inliers, &detections[reference], &detections[index]);
                let (w, h) = frames[index].1.dimensions();
                let displacement = max_corner_displacement(&rigid, w, h);
                if displacement < 1.0 {
                    format!("already aligned ({displacement:.2} px), {} inliers", inliers.len())
                } else {
                    let warped = stitching::warp_image_homography(&frames[index].1.to_rgb32f(), &rigid, w, h);
                    frames[index].1 = DynamicImage::ImageRgb32F(warped);
                    format!(
                        "warped, max corner displacement {displacement:.2} px, {} matches, {} inliers, transform {:?}",
                        matches.len(),
                        inliers.len(),
                        rigid.as_slice()
                    )
                }
            } else {
                "failed: no homography".to_string()
            };
            writeln!(log, "{} align {}: {outcome}", names[0], names[index]).unwrap();
        }
        let align_time = started.elapsed();
        for (index, frame) in frames.iter().enumerate() {
            dump_half(&frame.1, &out, &format!("{}-aligned", names[index]));
        }

        let started = Instant::now();
        let inputs: Vec<HDRInput> = frames
            .iter()
            .map(|(_, image, exposure, gain)| HDRInput::with_image(image, *exposure, *gain).unwrap())
            .collect();
        let merged = hdr_merge_images(&mut inputs.into()).unwrap();
        let merge_time = started.elapsed();
        dump_half(&merged, &out, "rr-merged-linear");
        let stretched = image_hdr::stretch::apply_histogram_stretch(&merged).unwrap();
        dump_half(&stretched, &out, "rr-stretched-linear");
        let srgb = apply_linear_to_srgb(stretched);
        dump_half(&srgb, &out, "rr-output-srgb");
        srgb.to_rgb8().save(out.join("rr-output.png")).unwrap();
        writeln!(
            log,
            "{} timings: load {:.1}s, align {:.1}s, merge {:.1}s",
            names[0],
            load_time.as_secs_f64(),
            align_time.as_secs_f64(),
            merge_time.as_secs_f64()
        )
        .unwrap();

        let lr = dir.join(format!("{}-HDR.dng", names[0]));
        let (lr_image, _) = load(&lr, &settings);
        writeln!(log, "{} lightroom dng {}x{}", names[0], lr_image.width(), lr_image.height()).unwrap();
        dump_half(&lr_image, &out, "lr-linear");
        log.flush().unwrap();
    }
}

/// Minimal little-endian TIFF reader for Lightroom's HDR DNG: finds the full-resolution
/// SubIFD (NewSubfileType 0, 3 samples) and decodes its JPEG XL float tiles.
fn decode_lightroom_hdr_dng(path: &Path) -> (u32, u32, Vec<f32>) {
    let b = fs::read(path).unwrap();
    assert_eq!(&b[0..4], b"II*\0");
    let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    // Returns tag -> (type, count, value-or-offset position)
    let read_ifd = |off: usize| {
        let n = u16_at(off);
        let mut tags = std::collections::HashMap::new();
        for i in 0..n {
            let e = off + 2 + i * 12;
            tags.insert(u16_at(e), (u16_at(e + 2), u32_at(e + 4), e + 8));
        }
        tags
    };
    let values = |tags: &std::collections::HashMap<usize, (usize, usize, usize)>, tag: usize| -> Vec<usize> {
        let &(ty, count, pos) = tags.get(&tag).unwrap();
        let size = match ty { 3 => 2, 4 | 13 => 4, _ => panic!("type {ty}") };
        let base = if size * count > 4 { u32_at(pos) } else { pos };
        (0..count).map(|i| if size == 2 { u16_at(base + i * 2) } else { u32_at(base + i * 4) }).collect()
    };
    let ifd0 = read_ifd(u32_at(4));
    let sub_offsets = values(&ifd0, 330);
    let tags = sub_offsets
        .iter()
        .map(|&o| read_ifd(o))
        .find(|t| values(t, 254)[0] == 0 && values(t, 277)[0] == 3)
        .expect("full-resolution SubIFD");
    let (w, h) = (values(&tags, 256)[0], values(&tags, 257)[0]);
    let (tw, th) = (values(&tags, 322)[0], values(&tags, 323)[0]);
    let offsets = values(&tags, 324);
    let counts = values(&tags, 325);
    let across = w.div_ceil(tw);
    let mut out = vec![0f32; w * h * 3];
    for (i, (&off, &len)) in offsets.iter().zip(&counts).enumerate() {
        let image = jxl_oxide::JxlImage::builder().read(&b[off..off + len]).unwrap();
        let frame = image.render_frame(0).unwrap();
        let fb = frame.image_all_channels();
        assert!(fb.channels() >= 3);
        let (x0, y0) = ((i % across) * tw, (i / across) * th);
        for y in 0..fb.height().min(h - y0) {
            for x in 0..fb.width().min(w - x0) {
                for c in 0..3 {
                    out[((y0 + y) * w + x0 + x) * 3 + c] = fb.buf()[(y * fb.width() + x) * fb.channels() + c];
                }
            }
        }
    }
    (w as u32, h as u32, out)
}

#[test]
#[ignore = "set HDR_DIR, HDR_OUT and HDR_SETS"]
fn hdr_lightroom_dng() {
    let dir = PathBuf::from(std::env::var("HDR_DIR").unwrap());
    let out_root = PathBuf::from(std::env::var("HDR_OUT").unwrap());
    for set in std::env::var("HDR_SETS").unwrap().split(';') {
        let first = set.split(',').next().unwrap();
        let (w, h, data) = decode_lightroom_hdr_dng(&dir.join(format!("{first}-HDR.dng")));
        let image = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_raw(w, h, data).unwrap());
        let max = image.as_rgb32f().unwrap().iter().copied().fold(0f32, f32::max);
        println!("{first}: lightroom camera-native {w}x{h}, max {max}");
        dump_half(&image, &out_root.join(first), "lr-camera");
    }
}
