# HDR merge: design note

Issue #87, phase 1. This is a proposal for maintainer review. Nothing here changes the app yet: phase 1 adds this note and a synthetic-bracket generator for tests (`src-tauri/src/hdr_fixtures.rs`). Phase 2 implements the design once a maintainer approves it.

The goal is Lightroom-level HDR merge for handheld brackets: motion deghosting, and a result that is a **linear DNG** you edit like any raw.

## 1. What RapidRoom does today

`merge_hdr` and `save_hdr` in `src-tauri/src/lib.rs`, plus `src-tauri/src/hdr_deghosting.rs`:

1. **Load:** `load_hdr_frames` fully develops each frame, exactly like opening it in the editor. That covers demosaic, white balance, the camera-to-linear-sRGB matrix (rawler's `Calibrate` step), `recover_clipped_pixel` (which desaturates everything above 0.5 of white), and the raw colour-NR and sharpening pre-pass. ISO and exposure time come from EXIF; a frame without them is rejected.
2. **Align:** `align_hdr_frames` uses FAST+BRIEF features on a downscaled grey proxy, then RANSAC for inliers, then a rigid (rotation + translation) fit. Every frame is warped to the middle frame. This came from upstream `CyberTimon/RapidRAW#1329` by alexdhill and is byte-identical to that PR. **Despite the module name, there is no per-pixel deghosting.**
3. **Merge:** `image_hdr::hdr_merge_images` from the `image-hdr` 0.6 crate (Apache-2.0). Each frame is divided by `exposure_time × ISO`, then averaged with weights proportional to exposure time. There is no clipping handling, so clipped pixels from long frames are averaged in and pull the highlights down. There is no noise model either.
4. **Finish:** `apply_histogram_stretch` maps the global min and max to 0..1 (it builds no histogram, so one hot pixel sets white). The result is then sRGB-encoded and saved as a 32-bit float TIFF, `<stem>_Hdr.tiff`. That is display-referred data in a float container: RapidRoom treats it as a non-raw image, so white balance and highlight latitude are gone.

**A correction to the issue:** the denoise path does **not** write DNGs. `save_denoised_image` writes `_Denoised.tiff` (16-bit). Nothing in RapidRoom writes DNG yet. The writer to reuse is rawler's (`rawler::dng::writer::DngWriter`, in the DngLab fork RapidRoom already depends on). See §3.6.

## 2. Prior work evaluated

| Source                                                                                                                       | What it really is                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Size                                                           | AI provenance                                                                                                                           | Usable?                                                                                                                                                                                                                                                                                   |
| ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cgasgarth/RapidRaw` `codex/hdr-fixture-set` (05fe7d28)                                                                      | A JSON manifest of 3 synthetic brackets and a bun+zod script that builds tiny seeded 8-bit PPM brackets and checks their hashes. Nothing consumes them.                                                                                                                                                                                                                                                                                                                                                                                                                                                   | 5 files, +529                                                  | `codex/*` branch; the fork's AGENTS.md is agent-driven; its HDR docs say "Source: RapidRaw ChatGPT project consult"                     | **Ideas only:** declared motion and highlight regions per fixture, and the metric gates (below).                                                                                                                                                                                          |
| `…/codex/hdr-ui-settings` (d61e8ab8)                                                                                         | A redesigned HdrModal with alignment, strategy, deghost and preview controls, plus schemas and locales. **The settings are never sent to the backend**; it still calls `merge_hdr({paths})`.                                                                                                                                                                                                                                                                                                                                                                                                              | 18 files, +826/−36                                             | as above                                                                                                                                | No. The controls do nothing.                                                                                                                                                                                                                                                              |
| `…/codex/hdr-app-server-tools` (20faa976), `…/hdr-editable-source-contract` (c659d073), `…/hdr-performance-smoke` (0e378192) | Stacked branches: JSON-RPC tool-registry samples, a zod validator for an "editable source" artifact, and timers for JS smoke scripts. Contracts only, no pixels.                                                                                                                                                                                                                                                                                                                                                                                                                                          | +1371, +129, +93                                               | as above                                                                                                                                | No. They depend on fork-only infrastructure (`packages/rawengine-schema`, bun, zod, sidecar artifact structs).                                                                                                                                                                            |
| `…/codex/hdr-ui-api-contract` (c021df20)                                                                                     | A snapshot of the fork's main with the merged versions of the above (fork PRs 1078–1087 and 1200–1206). Merge, alignment and deghosting exist only as **JS toys**: a ±5 px integer search, triangle weights, and a threshold-0.22 mask with the reference inside it. The Rust side adds a dimension check (redundant with `assert_uniform_dimensions`) and sidecar metadata with **hard-coded placeholder metrics** (confidence 0.7, clipped ratio 0, `transformType: "identity"`). The runtime still uses `image_hdr`.                                                                                   | about +4.8k, about 390 of it Rust                              | as above                                                                                                                                | **Ideas only:** bracket-detection rules (§3.8) and recording sources and the reference index in the result's sidecar.                                                                                                                                                                     |
| `Gigmaster02/RapidRAW_HDR` (all branches)                                                                                    | HDR **display and export**, not bracket merge: an Rgba16Float swapchain, an "OpenEXR (HDR)" export of the tone-mapped image, an output colour-space enum, and a PQ curve that clamps at 1.0 (SDR in a PQ container). `merge_hdr` is upstream's, untouched.                                                                                                                                                                                                                                                                                                                                                | 6–9 files, +97 to +642                                         | The codex commits are authored by `openai-code-agent[bot]` with a Co-authored-by trailer for the owner; one carries a `Copilot` trailer | **Not relevant to #87.** HDR display or export would be a separate issue, re-implemented rather than harvested.                                                                                                                                                                           |
| Upstream PR `CyberTimon/RapidRAW#1734` ("professional suite", open)                                                          | The wired-up HDR path shells out to **Hugin `align_image_stack` + `enfuse`**, which is Mertens fusion on gamma-encoded TIFFs. It then saves that display-referred result as a "32-bit linear DNG", which is mislabelled. The UI's deghost options are not passed to the merge. Unused in-tree Rust also exists: `hdr_fusion.rs` (an inverse-variance radiance merge with a made-up noise model) and `hdr_deghosting.rs` (TV-L1 optical flow, graph-cut ghost masks, a deghost brush). It also has a hand-rolled f32 DNG encoder that **omits WhiteLevel**, so rawler would decode it nearly black (§3.6). | 209 files, +53k/−11k, plus an 18.7 MB `.exe` and scratch files | All commits use the name "CyberTimon" with an email that isn't the maintainer's; no AI trailers                                         | **No** as code: external binaries, untested heuristics, correctness bugs, tangled with unrelated changes. **Ideas used:** exposure scale from t·ISO/N² with an image-based fallback, raised deghost thresholds in flat areas, mask dilation plus feathering, and a deghost brush (later). |
| `alexdhill/RapidRAW` `feat/hdr-deghosting` = upstream #1329                                                                  | The current alignment. Its history went homography → translation → rigid; debugging aids were removed before merge. No per-pixel deghosting.                                                                                                                                                                                                                                                                                                                                                                                                                                                              | +400/−75                                                       | none stated                                                                                                                             | **Kept**, with the proxy improvement in §3.3.                                                                                                                                                                                                                                             |
| `image-hdr` 0.6                                                                                                              | The merge described in §1.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | crate                                                          | n/a                                                                                                                                     | **Replace** it with the merge in §3.4 and drop the dependency.                                                                                                                                                                                                                            |
| Upstream requests `CyberTimon/RapidRAW#1316` and `#1774`                                                                     | #1316 asks for Lightroom-style HDR (Ctrl+H, preview, deghost setting, result stacked on its sources) and HDR panoramas, and criticises darktable's reliance on Hugin. #1774 asks for manual pre-alignment (nudge frames over a 50/50 overlay) for cases auto-alignment can't handle, such as the moon. Neither has maintainer replies.                                                                                                                                                                                                                                                                    |                                                                |                                                                                                                                         | #1316 matches this design. #1774 is a later add-on (§6).                                                                                                                                                                                                                                  |

## 3. Proposed design

The pipeline: **decode to camera-native linear RGB → normalise exposure → align → detect ghosts → merge with noise- and clipping-aware weights → write a float linear DNG with the camera's colour metadata → RapidRoom opens it through its normal raw pipeline with a suggested "auto tone".**

### 3.1 Decode: camera-native linear RGB, no look

Add a "merge input" develop mode next to `develop_raw_image`. It uses rawler's `RawDevelop` with only `Rescale, Demosaic, CropActiveArea, CropDefault`:

- **No `WhiteBalance` or `Calibrate`.** The data stays in the camera's own RGB, so the DNG can carry the camera's `ColorMatrix` and `AsShotNeutral`. RapidRoom then applies white balance and calibration exactly as for the source raws, which gives full white-balance latitude, DCP profile lookup by make and model, and so on. (In rawler, white balance is applied inside `Calibrate`, so leaving out both keeps the data neutral.)
- **No `recover_clipped_pixel`, colour NR or sharpening.** These are look decisions. The DNG gets them once, when it is opened. Applying them before the merge would bake them in, and they would then run a second time.
- **Full demosaic** (`DemosaicAlgorithm::Quality`), not the fast path.
- **A clipping map** comes from the mosaic _before_ demosaic. A pixel counts as clipped if any CFA sample in its 3×3 (Bayer) or 6×6 (X-Trans) neighbourhood is ≥ 0.98 of `white − black`. Demosaic spreads clipped values sideways, so the map is dilated by 2 px.
- Orientation is applied to pixels and clipping map alike; the DNG is written upright with `Orientation = 1`.
- Frames are stored as **f16** (6 bytes per pixel): 3 × 45 MP is about 0.8 GB instead of 1.6 GB.

**Why demosaiced and not Bayer:**

- Sub-pixel rigid warping and ghost detection are clean in RGB and messy in a CFA, especially X-Trans.
- Lightroom's HDR DNG is demosaiced `LinearRaw` too.
- The cost is one demosaic per frame instead of one in total, which is acceptable.

4-colour CFAs (rawler's `FourColor`) and monochrome sensors are out of scope at first: they fall back to the non-raw path in §3.9.

### 3.2 Exposure normalisation

- **EXIF first:** the relative exposure of frame i is `k_i = t_i · ISO_i / N_i²` (N is the f-number; if it is missing, assume it is constant). Every frame is scaled into the reference frame's units: `x̂_i = x_i · k_ref / k_i`.
- **Then refine from the images.** EXIF values are rounded: 1/60 s is really 1/64 s, and shutters are imprecise. After alignment, take the median of `x_ref / x̂_i` over pixels that are well exposed in both frames (0.05–0.8 of white, unclipped, outside the ghost mask, on a 1/4-scale proxy) and multiply `x̂_i` by it.
- **Guard rail:** if the correction is more than ±1/3 EV, keep the EXIF ratio and log a warning, because that frame is probably misaligned or has moved.
- This also makes merges of brackets without exposure EXIF possible later (estimate every ratio from the images), but they stay rejected for now, as today.

### 3.3 Alignment

Keep the rigid FAST/BRIEF/RANSAC alignment from #1329, with three changes:

1. **Detect on exposure-normalised, log-encoded proxies.** Today each frame's proxy is developed separately (`apply_cpu_default_raw_processing`), so a −2 EV and a +2 EV frame look very different and corners don't match well. With `log2(x̂_i)` from §3.2, the same scene point gives the same proxy value in every frame wherever it isn't clipped or in noise. Clipped areas are masked out of feature detection.
2. **A validity mask after warping.** Pixels a warp pulls from outside the frame get weight 0. The reference is never warped, so every output pixel always has at least one source and no auto-crop is needed. An optional "auto crop to the common area" can be added later.
3. **Alignment failure is surfaced in the dialog**, not only logged as "using as-is". The user can drop the frame or merge anyway.

### 3.4 Merge weights (the radiance estimate)

Per pixel and channel, the estimate is `X = Σ w_i x̂_i / Σ w_i`, using inverse-variance weights, which are the standard result for raw HDR (Granados et al. 2010; Hasinoff et al. 2010):

- **Noise model.** In normalised sensor units the variance is `var(x_i) = a·x_i + b`: shot noise plus read noise. After scaling by `g_i = k_i / k_ref`, `var(x̂_i) = (a·x_i + b) / g_i²`. The optimal weight is therefore `w_i = g_i² / (a·g_i·Z + b)`, where Z is a **first-pass estimate** of the radiance. It is not computed from the frame's own noisy value, because weights computed from it are biased toward low samples. Z comes from the longest unclipped frame at each pixel, box-blurred over 5 px.
- **What this means in practice:** wherever it isn't clipped, the long exposure dominates (`g²` grows faster than the shot-noise term). Short exposures only contribute where the longer ones are clipped. That is exactly "trust short exposures in the highlights and long exposures in the shadows", derived from the noise model rather than from a hat function.
- **Clipping.** `w_i` tapers smoothly to 0 as the pixel's clipping value in frame i goes from 0.85 to 0.95 of white. That value is the max channel of the dilated map from §3.1, and using it tapers all three channels together so colour can't shift from mixing a clipped channel of one frame with an unclipped channel of another. The weight maps are then box-blurred over 3 px so clip boundaries don't produce seams.
- **Noise parameters a and b** come from the frame when rawler exposes masked black pixels: b is their variance, and a is fitted from flat patches. Otherwise they come from defaults scaled by ISO (a = ISO/100 / 4000, b = (2/(white − black))²). Weights only need _relative_ accuracy, so the defaults are safe. They are tuned against the fixtures.
- **Clipped in every frame:** use the shortest frame's value. That region is marked so the auto-tone (§3.7) knows it is real clipping.
- **Memory:** the merge is an accumulation (`Σ w x̂`, `Σ w`), so it runs one frame at a time, in strips, from the f16 frames.

### 3.5 Motion deghosting

**Strength:** Off / Low / Medium / High, as in Lightroom. Off is today's behaviour with the new weights.

**Reference frame:**

- **Default:** the middle exposure (by EV, ties to the middle index), as today.
- **Auto:** the frame with the most _well-exposed_ pixels, between 2 % and 90 % of white and unclipped, on the proxy. The issue suggests "fewest clipped pixels", but that always picks the darkest frame, which makes the moving areas the noisiest. Well-exposed fraction picks the darkest frame only when the highlights need it.
- **Manual:** the user picks any frame in the dialog.

**Detection,** for each frame i ≠ ref, on 1/4-scale proxies of `x̂_i` and `x̂_ref` (box-downsampled, which also cuts noise 4×):

1. Where both frames are valid, `d = |x̂_i − x̂_ref| / sqrt(var_i + var_ref + (τ·|∇x̂_ref|)²)`. The gradient term tolerates residual misregistration of τ = 0.5 px at edges. This is the noise-aware form of #1734's "raise the threshold in flat areas" idea. Use the max over channels.
2. Where the reference is clipped, it only bounds the radiance from below. Frame i is inconsistent if `x̂_i` is clearly darker than the clip level (`x̂_i < 0.8 · clip_ref`).
3. Where the reference is below the noise floor, use the symmetric test with frame i.
4. Flag a pixel if `d > k`, with k = 6 / 4 / 2.5 for Low / Medium / High, **and** the relative difference is more than 15 % / 10 % / 6 %. The second condition stops tiny but statistically significant differences, such as exposure-ratio residue, from being flagged.

**Mask cleanup:** a morphological open (radius 1 at proxy scale) drops isolated pixels, and a close (radius 2) fills holes. Then upsample to full resolution, dilate by 2 / 4 / 8 px (Low / Medium / High) and feather with a Gaussian of the same σ. The result is a soft mask `M_i ∈ [0,1]` per frame. The union is the **ghost mask**.

**Merge:** in each connected ghost region, **a single frame** supplies the pixels:

- the reference, if at least 90 % of the region is valid in it;
- otherwise the frame with the highest well-exposed fraction in the region. This handles a person walking across a clipped window.

Outside the ghost regions the merge is the normal weighted one; the feathered mask blends between the two. Moving regions are noisier when they come from a single dark frame, as in Lightroom.

**Overlay:** the 1/4-scale union mask is returned with the preview as a separate PNG. The dialog's "Show deghost overlay" tints it red, as Lightroom does.

**Later (§6):** a deghost brush that pins a region to a chosen frame, an idea from #1734.

### 3.6 Output: a float linear DNG

**Format:**

- `PhotometricInterpretation = LinearRaw` (34892), 3 samples per pixel, `SampleFormat = 3` (IEEE float).
- **16-bit half float**, as Lightroom's default. f16 has a 10-bit mantissa (0.1 %) and about 30 stops of range including subnormals, which is plenty.
- Deflate-compressed tiles with the floating-point predictor (34894).

**Writer:** rawler's `DngWriter` (`rawler/src/dng/writer.rs`). It can already write a cpp=3 `LinearRaw` image, and with `RawImageData::Float` data it writes **uncompressed f32** (`SampleFormat = 3`, 32 bits), but nothing smaller: "Lossless" silently converts float to u16. Uncompressed f32 is about 290 MB for 24 MP. So:

- **Proposed (phase 2):** extend the writer in `yojen7/RapidRAW-DngLab` (already patched in `Cargo.toml`) with f16 output and Deflate tiles with predictor 34894. rawler already _reads_ exactly this (`decompressors/deflate.rs`, which also opens Lightroom HDR DNGs) and depends on `libflate`, which has an encoder. Expected size is roughly 2–3× the source raw (an estimate, to be measured).
- **Fallback if the patch is unwanted:** uncompressed f16, which is about 145 MB for 24 MP. That needs only the sample-format part of the patch.

**Pixel values:** camera-native RGB from §3.4, in reference-frame units, with black = 0. They are then scaled LR-style so the brightest unclipped value is 1.0, and the scale is stored as `BaselineExposure = log2(scale)`. Values in the file therefore stay in [0, 1], which other raw editors expect.

**Tags** (the writer covers some itself, the rest go through `root_ifd_mut().add_tag`):

| Tag                                           | Value                                                                                                                              |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `WhiteLevel`                                  | 1. **Required:** without it, rawler assumes 2³²−1 and RapidRoom decodes the file almost black. This is the bug in #1734's encoder. |
| `BlackLevel`                                  | 0                                                                                                                                  |
| `ColorMatrix1/2` + `CalibrationIlluminant1/2` | from the reference raw                                                                                                             |
| `ForwardMatrix1/2`                            | from the reference raw, if present                                                                                                 |
| `AsShotNeutral`                               | the reference frame's                                                                                                              |
| `BaselineExposure`                            | as above                                                                                                                           |
| `Make`, `Model`, `UniqueCameraModel`          | from the reference (keeps DCP and lens lookups working)                                                                            |
| `DefaultCrop`                                 | full frame                                                                                                                         |
| EXIF                                          | the reference frame's (date, lens, focal length, ISO, exposure time)                                                               |
| `ImageDescription`                            | "HDR merge of A, B, C"                                                                                                             |
| DNG preview                                   | an sRGB JPEG preview, so file browsers and the thumbnail cache have one                                                            |

**Not copied:** source opcode lists (gain maps, warps in phone DNGs). They must be applied _before_ the merge or not at all, and rawler doesn't apply them today. Phone DNG brackets are therefore out of scope for v1.

**Pipeline changes needed to open the file correctly.** These are rendering changes for _existing_ float `LinearRaw` DNGs too, such as Lightroom HDR merges opened in RapidRoom. They go in their own PR with the `rendering-change` label and need maintainer sign-off:

1. **Honour `BaselineExposure`** for float `LinearRaw` DNGs, as a scale at load, so "Exposure 0" looks like the reference frame. RapidRoom reads it nowhere today.
2. **Skip `recover_clipped_pixel`** for float `LinearRaw` DNGs. It desaturates everything above 0.5 of white, which is right for clipped sensor data but wrong for merged highlights, which are real colour.
3. Values above 1.0 after the BaselineExposure scale already survive: `develop_internal` clamps at 1000.

**Name and place:** `<first frame stem>-HDR.dng` next to the first frame, then `-HDR-2.dng` and so on, never overwriting. That replaces `_Hdr.tiff`. Card mode is respected through `ensure_card_writable_for_paths`, as `save_hdr` does now. RapidRoom's `.rrexif` sidecar is written as today.

### 3.7 A natural default look ("Auto tone")

On save, RapidRoom writes a sidecar for the new DNG with **ordinary slider values**. Nothing is baked into the pixels, so everything stays editable. They are computed on a 1/4-scale preview of the DNG after white balance and calibration:

| Slider                       | How it is set                                                                                                                                       |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Exposure                     | Puts the log-average luminance (excluding the 0.5 % brightest and darkest pixels) at 18 % grey, clamped to ±2 EV around the reference frame's look. |
| Highlights                   | Negative, −20 to −70, scaled by how many stops of data sit above 1.0 after Exposure.                                                                |
| Shadows                      | Positive, +10 to +40, scaled by the fraction of pixels below −5 stops.                                                                              |
| Whites / Blacks              | Small, so the 0.1 % points land near clipping without crossing it.                                                                                  |
| Clarity, Texture, Saturation | Unchanged, so the result is "not crunchy".                                                                                                          |

`perform_auto_analysis` in `image_processing.rs` (the editor's Auto button) is reused where it fits, but it works on display-referred images; the HDR version needs the scene-linear statistics above. The dialog gets an "Auto tone" checkbox (on by default). Off gives the reference frame's look.

### 3.8 Workflow and UI

- **Starting a merge:** from a selection of 2–9 frames, or from a stack (#86) via a "Merge to HDR…" action on it. The dialog shows the frames with their EVs, the chosen reference, warnings, a fast preview (1/4 scale) with the deghost overlay, and options: Deghost (Off/Low/Medium/High), Reference (Auto/Middle/pick), Auto tone, Show overlay.
- **Preflight** (idea from cgasgarth's `hdrBracketDetection.ts` and #1734):
  - **Block on:** different camera models, different dimensions, duplicate EVs, or fewer than 2 frames.
  - **Warn on:** aperture mismatch, a capture-time gap over 30 s, a bracket spacing above 4 EV (gaps in coverage), or failed alignment.
- **Progress and cancel:** `merge_hdr` takes a cancel token: an `AtomicBool` in `AppState`, set by a `cancel_hdr_merge` command. It is checked between frames and between merge strips, and in rawler's develop through the existing `cancel_token`. Progress events carry `{stage, current, total}` instead of free text. Today the dialog's Cancel only closes the window while the merge keeps running.
- **Result:** saving writes the DNG and its sidecar and, once #86 lands, stacks the DNG on top of its sources. The sidecar records the source paths, reference index, EV ratios as applied, deghost strength and the RapidRoom version. These are the real values; cgasgarth's version had placeholders.
- **Preview and full resolution:** the dialog preview runs the whole pipeline at 1/4 scale, and "Merge" runs it at full resolution. Memory use is about one f16 frame per source plus two f32 accumulators.

### 3.9 Non-raw inputs (JPEG, TIFF)

They keep working, with the same merge, linearised from sRGB and clipped at 1.0. They are written as a linear DNG whose "camera" is linear sRGB: `ColorMatrix1` = XYZ→linear sRGB (D65), `AsShotNeutral` = 1, 1, 1. That gives one output format and one code path. The histogram stretch is gone for these too.

## 4. Test plan

### 4.1 Synthetic brackets (in this PR)

`src-tauri/src/hdr_fixtures.rs` (test-only) builds brackets with known ground truth:

- **Scene:** a procedural scene with about 19 stops of range (a log ramp, textured rectangles for feature matching, colour patches, and a "sun" that clips even at −2 EV). Alternatively, any linear image, such as a decoded CC0 raw.
- **Frames:** each frame scales the scene by 2^EV, applies a sub-pixel camera shift, composites a moving disc, adds shot and read noise, then clips at white and quantises to 14 bits. Exposure time follows the EV.
- **Ground truth:** the noise-free, unclipped EV 0 radiance in the reference frame's coordinates, plus the **motion mask** (everywhere the disc lands in any frame once aligned).
- **Metric:** `mean_stop_error` measures error in stops, inside or outside a mask.
- **Disk output:** an ignored test, `write_bracket_from_raw`, writes a bracket made from a CC0 raw (`rapidroom/regression/corpus.json`) as float TIFFs, for viewing:

  ```bash
  RAPIDROOM_HDR_FIXTURE_RAW=<raw> RAPIDROOM_HDR_FIXTURE_OUT=<dir> \
    cargo test --lib hdr_fixtures::tests::write_bracket_from_raw -- --ignored
  ```

### 4.2 Automated tests to add in phase 2 (`cargo test --lib`, runs in CI)

| Test                                                                       | Gate                                                                                                                                                                                     |
| -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Exposure ratio, with EXIF rounded to 1/3 stop                              | recovered to within 0.03 EV                                                                                                                                                              |
| Alignment on the proxy                                                     | recovered shift within 0.25 px                                                                                                                                                           |
| Static merge (no mover), outside areas clipped in every frame              | mean error < 0.05 stops; the sun region, recovered from −2 EV, within 0.05 stops                                                                                                         |
| Shadow noise                                                               | in a flat dark patch, std below 0.6 × the reference frame's                                                                                                                              |
| Ghosts without deghosting                                                  | Off produces visible ghosts (error > 0.5 stops in the motion mask), which proves the fixture catches them                                                                                |
| Deghosting at Low/Medium/High                                              | mask recall ≥ 0.95 and precision ≥ 0.9 against the true motion mask; error in the motion mask < 0.1 stops; outside it no worse than Off. Gates borrowed from cgasgarth's fixture smokes. |
| Clipped reference with a mover (disc over the sun)                         | the region comes from one frame; no mixed-frame pixels                                                                                                                                   |
| DNG round trip: write, read with rawler, develop with RapidRoom's pipeline | pixels match the merge within f16 precision; `WhiteLevel`, colour matrices, `AsShotNeutral`, `BaselineExposure` and make/model are present                                               |
| Cancel                                                                     | a set token stops the merge between strips and writes no file                                                                                                                            |

### 4.3 Disk fixtures from CC0 raws (phase 2)

Extend the generator to write **Bayer DNG brackets** with EXIF exposure time and ISO, scaling the CFA data and shifting by even pixels so the CFA phase is kept. They then load through `load_hdr_frames` and the real UI. Three corpus raws cover Bayer (`sony-a7c2-15mp-compressed`), X-Trans (`fuji-xt4-26mp-lossless`) and a different maker (`canon-r50-24mp-craw`).

### 4.4 Regression

- The merge isn't in the render path, so the pixel-exact regression check must stay identical for the corpus. The corpus has no float DNGs, so §3.6's pipeline changes shouldn't move it either.
- The pipeline PR is still labelled `rendering-change` and shows before/after images of a Lightroom HDR DNG.

### 4.5 Human sign-off (needs-human)

A maintainer merges real handheld brackets in RapidRoom and Lightroom and compares:

- ghosts (people, cars, water, leaves) at each strength;
- highlight colour (sunset, lamps);
- shadow noise;
- the default look;
- editing latitude: white balance ±2000 K, exposure ±3 EV, highlights −100.

### 4.6 Performance

Target: 3 × 24 MP in under 15 s on a recent 8-core laptop CPU, with peak memory under 2 GB. Merge, deghost and the DNG write are timed separately.

## 5. Phase 2 plan

One PR each, in this order:

1. **rawler:** f16 + Deflate/predictor float output in `yojen7/RapidRAW-DngLab`, and the `Cargo.toml` patch rev bump.
2. **`hdr_dng.rs`:** writing a linear DNG with all the tags in §3.6, plus the round-trip test. No UI.
3. **Merge core:** merge-input decode, exposure refinement, proxy alignment and the noise-aware weights. This replaces `image-hdr` (dependency removed) and `_Hdr.tiff` with `-HDR.dng`. Tests from §4.2.
4. **Deghosting:** detection, single-source regions and the overlay. Tests from §4.2.
5. **Pipeline (`rendering-change`):** `BaselineExposure` and no clipped-highlight recovery for float `LinearRaw` DNGs.
6. **Workflow:** dialog options, preflight, cancel and progress, auto tone, the sidecar, and stacking once #86 is in.

## 6. Questions for maintainers

1. **Output:** f16 + Deflate, which needs the rawler patch (step 1), or uncompressed f16 to start?
2. Is it OK to make **`BaselineExposure`** and **skipping clipped-highlight recovery** apply to _all_ float `LinearRaw` DNGs (§3.6)? It improves Lightroom HDR DNGs too, but it is a rendering change.
3. **Auto reference:** the well-exposed fraction (proposed) or "fewest clipped pixels" (the issue's wording)?
4. Should RapidRoom keep a legacy "merged TIFF" output, or only DNG?
5. **Later features:** a deghost brush (#1734's idea), manual pre-alignment (upstream `CyberTimon/RapidRAW#1774`) and HDR panoramas (upstream `CyberTimon/RapidRAW#1316`). Separate issues?

## Credits

- **Alignment:** alexdhill (upstream `CyberTimon/RapidRAW#1329`), kept as is.
- **Ideas from cgasgarth/RapidRaw** (no code): fixtures with declared motion and highlight regions, the deghost metric gates (recall ≥ 0.95, precision ≥ 0.9), the bracket preflight rules, and recording the merge provenance in the sidecar.
- **Ideas from upstream `CyberTimon/RapidRAW#1734` by mariusr-hub** (no code): exposure scale from t·ISO/N² with an image-based fallback, raised deghost thresholds in flat areas, ghost-mask dilation and feathering, and the deghost brush.

`CREDITS.md` rows are added in the phase 2 PRs that use these ideas.
