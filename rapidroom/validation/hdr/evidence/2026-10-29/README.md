# HDR merge measurements, 29 October 2026 (#87, step 1)

RapidRoom `main` at `1bcb83f0` compared with Lightroom Classic 15.6 HDR merges of the same four Sony A7C II brackets (`RapidRoom/hdr-test/` on the maintainer's NAS; use permitted on #87). Measurement only: nothing here changes the app.

## Files

- `results.json`: all numbers, one object per bracket.
- `A7C00449-panel.jpg`, `A7C00428-panel.jpg`: the moving region (512×384 full-resolution pixels around the densest motion, shown at half resolution). Top row: the three exposure-normalised input frames after RapidRoom's alignment. Bottom row: RapidRoom's merge and Lightroom's merge with the same display transform, and RapidRoom's result as the app saves it. Lightroom's colours are approximate (camera RGB → `AsShotNeutral` → `ForwardMatrix2` → sRGB, without Adobe's profile tables); the geometry and ghosting are what matter.
- `hdr_measure_harness.rs`, `harness-visibility.patch`: an ignored Rust test that runs the same steps as `merge_hdr` (load, align, `image-hdr` merge, histogram stretch, sRGB) without an `AppHandle`, and dumps 2×2-downsampled linear frames. The patch makes three helpers `pub(crate)` and loads without writing `.rrexif` caches beside the originals. Its `hdr_lightroom_dng` test (jxl-oxide 0.12.6) fails on Lightroom's tiles with `UnexpectedEof`; `lr_dng.py` uses libjxl 0.11 instead.
- `lr_dng.py`: decodes Lightroom's DNG 1.7 (LinearRaw, float16, lossy JPEG XL tiles) with libjxl via ctypes and converts it to linear sRGB.
- `analyze.py`: registers Lightroom's result to RapidRoom's reference frame (affine fit from tile phase correlation) and computes the metrics. Coordinates and shifts are in full-resolution pixels; tonal metrics use log luminance on the half-resolution dumps.

## Reproduce

```sh
git apply rapidroom/validation/hdr/evidence/2026-10-29/harness-visibility.patch
cp rapidroom/validation/hdr/evidence/2026-10-29/hdr_measure_harness.rs src-tauri/src/
cd src-tauri
HDR_DIR=/path/to/hdr-test HDR_OUT=/path/to/out \
HDR_SETS="A7C00409,A7C00410,A7C00411;A7C00397,A7C00398,A7C00399;A7C00428,A7C00429,A7C00430;A7C00449,A7C00450,A7C00451" \
  cargo test --lib hdr_measure_harness::hdr_measure -- --ignored --nocapture
for s in A7C00409 A7C00397 A7C00428 A7C00449; do python3 lr_dng.py /path/to/hdr-test/$s-HDR.dng /path/to/out/$s; done
python3 analyze.py /path/to/out /path/to/crops > results.json
```

The patch also adds `mod hdr_measure_harness;` to `lib.rs`. The frontend `dist/` directory must exist for the test build.
