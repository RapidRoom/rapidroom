# Bounded GPU export

CPU-readback renders use the existing GPU colour pipeline with reusable source
and mask windows. Geometry, lens warp, lens blur, rotation and cropping still
run in their existing CPU order before colour processing. CPU image buffers and
file encoding are full-frame; this change bounds GPU allocations.

## Coordinates and support

The transformed, cropped image defines global coordinates. Image dimensions,
source-window origin/extent and mask-window origin are separate uniforms. Source
loads translate global coordinates into an uploaded window; global clamping,
noise hashes, vignette centres, grain and chromatic aberration retain their full
image coordinates. Masks use the same transformed image grid.

Output tiles align to a 2048-pixel grid. Their work rectangle depends on the tile
and image boundaries, never the requested ROI. A region clips only copied
output. This is the coordinate invariant shared with region rendering (#151).

The work halo is at least 128 pixels and covers the largest active separable
blur: sharpness 1, tonal 3.5, clarity/halation 8, structure/glow/dehaze 40,
multiplied by `min(image_width, image_height) / 1080`, then rounded up. Active
mask effects count too. A separate source halo covers horizontal blur reads,
centred contrast, colour-noise-reduction jitter and radial chromatic aberration.
Image edges clamp globally, while padding outside the true image stays zero.

A wider work halo corrects the old vertical blur's internal-tile edge clamping
when its radius exceeded 128. Wide effects can therefore change around old tile
seams. Default corpus equality and visual panels remain required evidence;
no unchanged-render claim follows from the allocation design alone.

## Budget and limits

The allocation planner limits each CPU-readback render to **1.5 GiB** of requested
GPU resources, including five f16 work textures, u8 and optional f16 output,
tile readback, up to 32 mask layers, source windows, LUTs and flare resources.
Working/output display textures are one-pixel placeholders for this path.
Cached full-frame inputs and processors are released before export allocation.
A retained native-display frame is charged against this budget too.
Driver heaps and staging uploads require
measured peak-memory evidence in addition to this conservative texture ledger.

Flare preserves its existing hardware-linear global 512-pixel guide pass. Its
full f16 source is explicitly charged to the budget; flare requests above the
texture or budget limit fail with a descriptive error. Other effects can render
images larger than a single GPU texture because only windows are uploaded.
Invalid regions, mismatched masks and unsupported effect footprints fail before
rendering. An oversized export no longer silently returns an unprocessed image.

## Validation

Pure planner tests run with `cargo test --locked --lib tiled_export::`.
Native-GPU tests are ignored by ordinary unit runs and must run serially under
the shared build lock:

```sh
cargo test --locked --lib tiled_export_full_tiled_region_matrix -- --ignored --nocapture
cargo test --locked --lib tiled_export_wide_halo_matches_untiled -- --ignored --nocapture
```

The matrix compares true single-work-rectangle full rendering, windowed tiles
and off-grid regions crossing tile boundaries. It exercises masks, colour
noise reduction, CA, vignette/grain, global flare, wide spatial effects and
production geometry/lens blur. Comparisons require exact u16 samples.

The 60MP memory test uses 32 mask bitmaps and separate `legacy` and `bounded`
processes via `RAPIDROOM_EXPORT_MEMORY_MODE`. Its output checksum must agree;
Linux DRM fdinfo measurements must be retained with adapter and source identity.
An estimate is not a measured peak. Local default full60 against the blessed
reference remains mandatory; references must never be blessed by this PR.

## Provenance

CalibRaw `2466ca6662c3fd1e901377c0450b19f5f96d3e85` informed the halo and
coordinate invariants. Its GPL-3.0-or-later source was studied, with no code,
shader, AI model or weight copied. The implementation is original RapidRoom
code against its existing shader and geometry pipeline, by Yojen with Codex.
