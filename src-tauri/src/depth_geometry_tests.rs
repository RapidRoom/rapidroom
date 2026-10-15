// SPDX-License-Identifier: GPL-3.0-or-later
// Geometry fixtures adapted from Duecki's CalibRaw ai_masks/depth.rs,
// commit 2466ca6662c3fd1e901377c0450b19f5f96d3e85. See tests/fixtures/depth.
// RapidRoom keeps its existing DA2 Small map polarity and resized map dimensions.

use super::*;
use image::{Rgb, RgbImage};

fn geometry_session() -> Mutex<Session> {
    static INITIALIZED: std::sync::Once = std::sync::Once::new();
    INITIALIZED.call_once(|| {
        let name = if cfg!(target_os = "windows") {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };
        let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(name);
        ort::init_from(runtime.to_string_lossy()).commit().unwrap();
    });
    Mutex::new(
        Session::builder()
            .unwrap()
            .with_intra_threads(1)
            .unwrap()
            .commit_from_memory(include_bytes!("../tests/fixtures/depth/channel-mean.onnx"))
            .unwrap(),
    )
}

fn gradient(width: u32, height: u32) -> DynamicImage {
    let landscape = width >= height;
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        let value = if landscape {
            x * 255 / (width - 1).max(1)
        } else {
            y * 255 / (height - 1).max(1)
        } as u8;
        Rgb([value; 3])
    }))
}

#[test]
fn da2_unpadding_preserves_landscape_portrait_square_and_thin_geometry() {
    let session = geometry_session();
    for (width, height, expected_width, expected_height) in [
        (4, 2, 518, 259),
        (2, 4, 259, 518),
        (4, 4, 518, 518),
        (1, 10_000, 1, 518),
        (10_000, 1, 518, 1),
    ] {
        let mask = run_depth_anything_model(&gradient(width, height), &session).unwrap();
        assert_eq!(mask.dimensions(), (expected_width, expected_height));
        assert_eq!(mask.get_pixel(0, 0)[0], 0);
        assert!(mask.get_pixel(expected_width - 1, expected_height - 1)[0] >= 254);
        if width >= height {
            for row in mask.as_raw().chunks_exact(expected_width as usize) {
                assert!(row.windows(2).all(|pair| pair[0] <= pair[1]));
                assert_eq!(row, &mask.as_raw()[..expected_width as usize]);
            }
        } else {
            let rows: Vec<_> = mask
                .as_raw()
                .chunks_exact(expected_width as usize)
                .collect();
            assert!(
                rows.iter()
                    .all(|row| row.iter().all(|pixel| *pixel == row[0]))
            );
            assert!(rows.windows(2).all(|pair| pair[0][0] <= pair[1][0]));
        }
    }
}

#[test]
fn da2_normalization_ignores_letterbox_padding() {
    let session = geometry_session();
    for (width, height) in [(4, 2), (2, 4)] {
        // All content normalizes below zero in the input tensor. The graph's
        // padded zero pixels must not influence min/max after unpadding.
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            let coordinate = if width > height { x } else { y };
            Rgb([(10 + coordinate * 10) as u8; 3])
        }));
        let mask = run_depth_anything_model(&image, &session).unwrap();
        assert_eq!(mask.as_raw().iter().min(), Some(&0));
        assert!(*mask.as_raw().iter().max().unwrap() >= 254);
    }
}

#[test]
fn da2_near_far_order_stays_aligned_for_odd_landscape_and_portrait() {
    let session = geometry_session();
    for (width, height) in [(7, 3), (3, 7)] {
        let mask = run_depth_anything_model(&gradient(width, height), &session).unwrap();
        let (w, h) = mask.dimensions();
        assert_eq!(w.max(h), DEPTH_INPUT_SIZE);
        assert_eq!(mask.get_pixel(0, 0)[0], 0);
        assert!((120..=135).contains(&mask.get_pixel(w / 2, h / 2)[0]));
        assert!(mask.get_pixel(w - 1, h - 1)[0] >= 254);
    }
}
