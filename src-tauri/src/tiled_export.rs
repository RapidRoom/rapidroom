//! Export texture windows retain coordinates in the transformed, cropped image.
//! Geometry and CPU mask generation happen before this planner. The ROI only
//! clips copied output; it must never change the blur work rectangle.
use super::{RenderRequest, Roi, compute_blur_needs};

pub(super) const TILE_SIZE: u32 = 2048;
pub(super) const LEGACY_HALO: u32 = 128;
pub(super) const GPU_BUDGET: u64 = 1536 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) struct ExportPlan {
    pub tile_size: u32,
    pub work_halo: u32,
    pub read_halo: u32,
    pub work_width: u32,
    pub work_height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub estimated_bytes: u64,
}

impl ExportPlan {
    pub fn new(
        width: u32,
        height: u32,
        request: &RenderRequest,
        max_dim: u32,
    ) -> Result<Self, String> {
        Self::with_tile_size(width, height, request, max_dim, TILE_SIZE)
    }

    pub fn with_tile_size(
        width: u32,
        height: u32,
        request: &RenderRequest,
        max_dim: u32,
        tile_size: u32,
    ) -> Result<Self, String> {
        if tile_size == 0 {
            return Err("Export tile size must be nonzero".into());
        }
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("Export dimensions must be nonzero and fit shader coordinates".into());
        }
        validate_roi(width, height, request.roi)?;
        for mask in request.mask_bitmaps {
            if mask.dimensions() != (width, height) {
                return Err("Export mask dimensions must match the transformed image".into());
            }
        }
        let needs = compute_blur_needs(&request.adjustments);
        let scale = width.min(height) as f32 / 1080.0;
        let radius = if needs.structure {
            40.0
        } else if needs.clarity {
            8.0
        } else if needs.tonal {
            3.5
        } else if needs.sharpness {
            1.0
        } else {
            0.0
        };
        let blur_halo = (radius * scale).ceil() as u32;
        let g = &request.adjustments.global;
        let ca = g
            .chromatic_aberration_red_cyan
            .abs()
            .max(g.chromatic_aberration_blue_yellow.abs());
        if !ca.is_finite() {
            return Err("Non-finite chromatic aberration in export".into());
        }
        // Colour NR has 2*7 + 7/4 jitter taps; centred contrast has 7.5*scale.
        // CA's radial displacement is bounded per axis by half that image axis.
        let direct_halo = 18_u32
            .max((7.5 * scale).ceil() as u32)
            .max(((width.max(height) as f32 * 0.5 * ca).ceil() as u32).saturating_add(2));
        let work_halo = LEGACY_HALO.max(blur_halo);
        let read_halo = blur_halo.max(direct_halo).saturating_add(8);
        let work_width = width
            .min(tile_size.saturating_add(work_halo.saturating_mul(2)))
            .div_ceil(256)
            * 256;
        let work_height = height.min(tile_size.saturating_add(work_halo.saturating_mul(2)));
        let source_width = width.min(work_width.saturating_add(read_halo.saturating_mul(2)));
        let source_height = height.min(work_height.saturating_add(read_halo.saturating_mul(2)));
        if [work_width, work_height, source_width, source_height]
            .iter()
            .any(|&d| d > max_dim)
        {
            return Err("Export effect footprint exceeds the GPU texture limit".into());
        }
        let work_pixels = u64::from(work_width) * u64::from(work_height);
        let mask_layers = request.mask_bitmaps.len().clamp(2, super::MAX_MASKS) as u64;
        let lut_bytes = request
            .lut
            .as_ref()
            .map_or(0, |lut| lut.data.len() as u64 * 8);
        let flare_bytes = if g.flare_amount > 0.0 {
            if width > max_dim || height > max_dim {
                return Err("Flare's global sampling pass exceeds the GPU texture limit".into());
            }
            u64::from(width) * u64::from(height) * 8
        } else {
            0
        };
        // Five f16 work textures + u8 output + optional f16 output + readback,
        // bounded mask array + source. Reserve 16 MiB for flare maps/uniforms.
        let estimated_bytes = work_pixels * (40 + 4 + 8 + 8 + mask_layers)
            + u64::from(source_width) * u64::from(source_height) * 8
            + lut_bytes
            + flare_bytes
            + 16 * 1024 * 1024;
        if estimated_bytes > GPU_BUDGET {
            return Err(format!(
                "Export GPU allocation budget exceeded: {estimated_bytes} > {GPU_BUDGET} bytes"
            ));
        }
        Ok(Self {
            tile_size,
            work_halo,
            read_halo,
            work_width,
            work_height,
            source_width,
            source_height,
            estimated_bytes,
        })
    }

    pub fn work_rect(self, width: u32, height: u32, tile_x: u32, tile_y: u32) -> Roi {
        let x = tile_x * self.tile_size;
        let y = tile_y * self.tile_size;
        expanded(
            width,
            height,
            Roi {
                x,
                y,
                width: self.tile_size.min(width - x),
                height: self.tile_size.min(height - y),
            },
            self.work_halo,
        )
    }

    pub fn source_rect(self, width: u32, height: u32, work: Roi) -> Roi {
        expanded(width, height, work, self.read_halo)
    }
}

pub(super) fn expanded(width: u32, height: u32, rect: Roi, halo: u32) -> Roi {
    let x = rect.x.saturating_sub(halo);
    let y = rect.y.saturating_sub(halo);
    Roi {
        x,
        y,
        width: rect
            .x
            .saturating_add(rect.width)
            .saturating_add(halo)
            .min(width)
            - x,
        height: rect
            .y
            .saturating_add(rect.height)
            .saturating_add(halo)
            .min(height)
            - y,
    }
}

pub(super) fn validate_roi(width: u32, height: u32, roi: Option<Roi>) -> Result<(), String> {
    if let Some(r) = roi
        && (r.width == 0
            || r.height == 0
            || r.x.checked_add(r.width).is_none_or(|v| v > width)
            || r.y.checked_add(r.height).is_none_or(|v| v > height))
    {
        return Err("Export region lies outside the transformed image".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_processing::AllAdjustments;
    fn request(a: AllAdjustments) -> RenderRequest<'static> {
        RenderRequest {
            adjustments: a,
            mask_bitmaps: &[],
            lut: None,
            roi: None,
        }
    }
    #[test]
    fn sixty_mp_wide_effects_and_32_masks_fit_budget() {
        let mut a = AllAdjustments::default();
        a.global.glow_amount = 1.0;
        let r = request(a);
        let p = ExportPlan::new(10000, 6000, &r, 16384).unwrap();
        assert!(p.work_halo >= 223);
        assert!(
            p.estimated_bytes + 30 * u64::from(p.work_width) * u64::from(p.work_height)
                < GPU_BUDGET
        );
        let mut flare = a;
        flare.global.flare_amount = 1.0;
        assert!(
            ExportPlan::new(10000, 6000, &request(flare), 16384)
                .unwrap()
                .estimated_bytes
                < GPU_BUDGET
        );
    }
    #[test]
    fn roi_never_changes_work_or_global_alignment() {
        let r = request(AllAdjustments::default());
        let full = ExportPlan::new(8000, 6000, &r, 16384).unwrap();
        let mut region = r;
        region.roi = Some(Roi {
            x: 2301,
            y: 3107,
            width: 101,
            height: 79,
        });
        let cropped = ExportPlan::new(8000, 6000, &region, 16384).unwrap();
        let f = full.work_rect(8000, 6000, 1, 1);
        let c = cropped.work_rect(8000, 6000, 1, 1);
        assert_eq!((f.x, f.y, f.width, f.height), (c.x, c.y, c.width, c.height));
        assert_eq!((f.x, f.y), (1920, 1920));
    }
    #[test]
    fn footprint_and_roi_errors_are_explicit() {
        let mut r = request(AllAdjustments::default());
        r.roi = Some(Roi {
            x: u32::MAX,
            y: 0,
            width: 2,
            height: 1,
        });
        assert!(ExportPlan::new(10, 10, &r, 16384).is_err());
        r.roi = None;
        r.adjustments.global.chromatic_aberration_red_cyan = f32::INFINITY;
        assert!(ExportPlan::new(10000, 6000, &r, 16384).is_err());
    }
}
