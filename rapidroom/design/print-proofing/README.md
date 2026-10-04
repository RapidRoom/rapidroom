# Print: soft-proofing and ICC export (design note)

Issue #122, step 1. This note describes the colour pipeline as it is today, where a printer/paper proof transform fits, and how a LittleCMS reference will check the results. It changes no code. The [decisions](#decisions-for-josh) at the end are for Josh. Once they are made, the implementation follows as separate PRs.

Upstream has only a request for this (`CyberTimon/RapidRAW#1315`). No fork has code to harvest.

## Summary

- RapidRoom renders everything into **display-referred, sRGB-encoded values clamped to 0–1**. Preview and export both come from that one output, and nothing in the app does ICC colour management (exports are only _tagged_ as sRGB).
- The proof transform fits **after the last step that sets a pixel's colour** (tone mapping, curves, LUT, grain), and before the clipping overlay, dither and 8-bit quantisation. The proof shows what printing that sRGB render through a colour-managed print path would look like. It works on the same values that export writes.
- Recommendation for the first version:
  - Do the transforms with **LittleCMS through the `lcms2` crate**, on the **CPU**, on the rendered output: preview after GPU readback, export before encoding.
  - Change no shader, so the default render stays bit-identical.
  - A GPU 3D LUT is a measured, accurate fallback if the CPU path turns out too slow in the app.
- Keep **sRGB as the output space** for now. Colours requiring negative sRGB-primary coordinates are clipped at raw decode; the final output clamp also limits the printable gamut. A wide-gamut output path is a separate, rendering-changing project.
- **LittleCMS's `tificc` is the end-to-end reference.** Its soft-proof and output conversions are bit-identical to the library transforms (measured below), so RapidRoom's exports can be checked against it pixel for pixel.

## The colour pipeline today

Line references were checked against integration `6e54bd33` (approved main plus catalog develop, colour-tool visibility and Lights Out). Measurement versions are recorded separately below.

| Stage                    | What happens                                                                                                                                                                                                                                                                                                                                                                                           | Where                                                                                                        |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| Raw decode               | rawler demosaics, white-balances and applies the camera matrix (`Calibrate`) into **linear sRGB/Rec.709 primaries, D65**. RapidRoom drops rawler's `SRgb` gamma step, so the data stays linear. **Negative values are clipped** (`clip_euclidean_norm_avg` → `clip_negative`), so colours requiring negative sRGB-primary coordinates cannot be recovered later. Highlights stay unbounded up to 1000. | [`raw_processing.rs:185-265`](../../../src-tauri/src/raw_processing.rs); pinned rawler `imgop/raw.rs:70,199` |
| Non-raw decode           | JPEG/PNG/TIFF are decoded by `image` and **treated as sRGB**. An embedded ICC profile (Adobe RGB, Display P3, …) is ignored.                                                                                                                                                                                                                                                                           | `src-tauri/src/image_loader.rs:392`                                                                          |
| GPU working space        | `shader.wgsl` linearises non-raw input with the sRGB curve, so every image is linear Rec.709 from here on. Exposure, white balance, HSL, colour grading, local contrast and masks run here. Several of them (HSL, hue shift, saturation/vibrance) go through `linear_to_srgb_extended` and assume sRGB primaries.                                                                                      | `src-tauri/src/shaders/shader.wgsl:1742`                                                                     |
| Tone mapping             | AgX (into a Rec.2020-based rendering space and back to sRGB primaries), or the legacy curve. Output is **display-referred and sRGB-encoded**.                                                                                                                                                                                                                                                          | `shader.wgsl:1931-1942`; matrices `src-tauri/src/image_processing.rs:2348`                                   |
| Display-referred steps   | Scene or display LUT, brightness, RGB/luma curves (global and mask), grain.                                                                                                                                                                                                                                                                                                                            | `shader.wgsl:1944-1995`                                                                                      |
| Overlays and output      | Clipping warning (`show_clipping`), dither (8-bit only), **clamp to 0–1**, written to `rgba8unorm`. With `HIGH_PRECISION_OUTPUT` (16-bit TIFF export) the target is `rgba16float`.                                                                                                                                                                                                                     | `shader.wgsl:1997-2014`; `src-tauri/src/gpu_processing.rs:1022`                                              |
| Preview on Linux         | Linux has no wgpu surface. The 8-bit output is read back and encoded as an **untagged JPEG**, which the WebKitGTK webview shows. **No display profile is applied**, so the monitor gets the sRGB values as they are.                                                                                                                                                                                   | `gpu_processing.rs:230`; `src-tauri/src/lib.rs:653-680`                                                      |
| Preview on macOS/Windows | By default (`use_wgpu_renderer`), the texture is drawn straight to a wgpu surface (`display.wgsl`) without readback, also with no display transform.                                                                                                                                                                                                                                                   | `gpu_processing.rs:201-230`                                                                                  |
| Export                   | GPU render (8- or 16-bit) → resize, border, padding, watermark in sRGB-encoded values → encode. JPEG, PNG and TIFF embed `sRGB-v2-magic.icc`. WebP, AVIF and JXL get no ICC profile.                                                                                                                                                                                                                   | `src-tauri/src/export_processing.rs:771,956,1015,1169-1265`                                                  |

**Colour management libraries.** None are used directly. `moxcms` 0.8.1 is already in the tree through `image`, but RapidRoom doesn't call it. In 0.8.1, black point compensation is commented out (`TransformOptions`, `src/transform.rs:112`), and there is no proofing or gamut-check transform.

**Existing UI precedent.** The clipping warning is a flag in the adjustments object that the shader turns into an overlay. Soft-proofing is a view mode, not an edit, so its state should not go into the adjustments or the sidecar (see [UX](#ux-and-settings-sketch)).

## What printing needs

1. **Soft proof:** take the rendered sRGB image to the printer/paper profile and back to the display.
   - Rendering intent: perceptual or relative colorimetric. Saturation and absolute colorimetric aren't useful for photos.
   - Black point compensation, on by default.
   - Paper white and paper black simulation: use absolute colorimetric for the printer→display leg.
   - A gamut warning overlay.
2. **ICC export:** the same image converted into the printer profile's device values, with that profile embedded, preferably as 16-bit TIFF.
3. **Later:** MCP tools that run the same proof and report out-of-gamut areas, and export at print sizes (physical size and ppi).

## Where the proof transform fits

The proof's input is the **final sRGB-encoded output in 0–1**: the same values that are exported, after grain and before the clipping overlay, dither and quantisation. In the shader that is between `shader.wgsl:1995` and `:1997`. On the CPU it is the read-back 8- or 16-bit image. Proofing anything earlier (scene-linear values, before curves or LUTs) would not show what gets printed.

### GPU or CPU

**A. CPU, LittleCMS, after readback (recommended first).**

- **Preview:** in the preview worker, apply a cached proofing transform to the RGBA8 image before JPEG encoding (`lib.rs:653-680`), split across threads with rayon. The gamut warning is LittleCMS's own gamut check, using alarm codes.
- **Export:** when a printer profile is selected, render at 16-bit (`RenderOutputPrecision::SixteenBit`) whatever the format. Then:
  1. run geometry, border and watermark as today, so pad and border colours (given in sRGB) are converted too;
  2. convert 16-bit sRGB to the printer profile with the chosen intent and BPC;
  3. quantise to 8 bits only if the format needs it, then embed the printer profile instead of sRGB.
- **Pros:**
  - The results are LittleCMS's by construction.
  - No shader change, so the default render stays bit-identical and the 60-image regression is untouched.
  - Proofing costs nothing when it's off.
- **Cons:**
  - CPU time, measured below. One 8-bit proof runs at about 25–34 MP/s per thread, about 12–13 MP/s with the gamut check. A 2 MP preview takes about 60 ms without the gamut check or 150 ms with it, single-threaded, and roughly a quarter of that across 4 threads. Interactive (dragging) previews are smaller.
  - On macOS/Windows the wgpu-surface preview has no readback. Proofing there would have to switch to the readback path while it's on. The done criteria only ask for Linux.

**B. GPU, baked 3D LUT (fallback).**

- **How it works:**
  1. Bake the same LittleCMS proofing transform into a 33³ RGBA16F 3D texture, sampled on an sRGB-encoded grid: RGB is the proofed colour, alpha is the gamut flag.
  2. Sample it with the existing tetrahedral code (`sample_lut_tetrahedral`) in a new binding at the point above, behind a uniform flag.
- **Pros:** no per-pixel CPU cost, and it works for both display paths.
- **Cons:**
  - It changes the shader. A regression run is needed to show it is a no-op when off.
  - The gamut flag is approximate near the gamut boundary.
- **Accuracy (measured):** a 33³ LUT is within mean 0.04 / p99 0.23 / max 1.1 ΔE2000 of unoptimised 16-bit LittleCMS. That is closer than LittleCMS's own default 8-bit optimised path (mean 0.19, max 2.3). The original measurement used 16-bit UNORM storage, not the proposed RGBA16F texture; the local recheck below measures float16 separately. 8-bit UNORM storage doubles the mean error.

Export should be on the CPU in either case: it's a one-off, 16-bit, and must match LittleCMS exactly. I recommend A for the preview as well, and switching to B only if A is measurably too slow in the running app.

### Working space and gamut

Proofing and export start from sRGB-clamped data, so the printer can never be asked for a colour outside sRGB.

- With colord's `FOGRA39L_coated.icc` (coated offset press), 6.8% of the printer's own colours lie outside sRGB. These are mostly cyans and yellows.
- Photo inkjet papers usually go further outside sRGB (cyan, green, yellow/orange). I couldn't measure that here, because no redistributable inkjet profile was available.

Using those colours means a **wide-gamut output path**:

- no negative clip at raw decode (a rawler change);
- the sRGB-primaries assumptions in the HSL, hue and saturation code reworked;
- AgX returning to a wider space;
- no clamp to sRGB before the output transform.

That changes rendered output for every image, so it needs its own issue, a regression reference update and two maintainer approvals. **Recommendation:** first version on sRGB output; a wide-gamut path as a separate issue if prints show the limit.

### Display

The proof result is encoded for an sRGB display. On Linux nothing applies a monitor profile today, so the proof is only as accurate as the monitor is close to sRGB. Display-profile support (on Linux, colord or the X11 `_ICC_PROFILE` atom) would be a separate, local-only feature. **Recommendation:** document the assumption in the first version and leave display profiles to a later issue.

With paper simulation on, paper white is shown darker and tinted relative to the UI's white. Lightroom handles this by drawing the proof on a paper-coloured background. We can do the same with the existing editor background colours.

## Library

|                                 | `lcms2` crate 6.2.0 (LittleCMS)                                                                                                | `moxcms` 0.8.1 (already in tree) |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- |
| Language                        | Rust wrapper over C                                                                                                            | pure Rust                        |
| Licence                         | MIT (crate and LittleCMS)                                                                                                      | BSD-3-Clause or Apache-2.0       |
| Intents                         | all four                                                                                                                       | all four                         |
| Black point compensation        | yes                                                                                                                            | no (commented out)               |
| Proofing transform, gamut check | yes                                                                                                                            | no                               |
| v2/v4 LUT profiles, CMYK        | yes                                                                                                                            | yes                              |
| Build                           | `lcms2-sys` 4.0.7: system lib via pkg-config by default, vendored LittleCMS 2.19 as fallback; `static` feature forces vendored | nothing new                      |

**Recommendation:** use `lcms2` with the `static` feature, so every platform (including Android and Windows) runs the same LittleCMS 2.19 rather than whatever the system has. This adds a dependency and a C build, which GOVERNANCE counts as a bigger decision. `moxcms` could replace it later if it gains BPC and proofing. I haven't evaluated other pure-Rust ports.

## UX and settings sketch

- **Soft proof** is a view mode, toggled from the editor toolbar like Reference View. It is not stored in adjustments or sidecars, so it never changes the edit, the render cache identity or batch exports. The panel has:
  - profile (chosen file);
  - intent (perceptual / relative colorimetric);
  - BPC (default on);
  - simulate paper and ink;
  - gamut warning, with its own overlay colour that is distinct from the clipping colours.
- **Profiles** are local files only. The picker can list the usual Linux locations (`~/.local/share/icc`, `~/.color/icc`, `/usr/share/color/icc`) read-only. There are no downloads or network requests.
  - Validate the profile class (output, also display/colour-space for testing) and its colour space (RGB or CMYK), and give a clear error for anything else.
  - A matrix-shaper output profile has no perceptual table, so the intent falls back to relative colorimetric. Say so in the UI.
- **Export:** a "Colour profile" option: sRGB (default, today's behaviour) or a printer profile with intent and BPC.
  - First version: RGB output profiles only, for TIFF 8/16-bit and JPEG. Most photo printer drivers and labs take RGB.
  - CMYK profiles can be proofed but not exported: `image` can't write CMYK TIFF or JPEG.
- **Headless CLI:** add `--icc-profile <path>`, `--intent perceptual|relative` and `--bpc` to the export flags. The reference check and the MCP tools both need this.
- **Print sizes (book help):** export by physical size and ppi, and write the resolution tags. This is a small, separate issue.
- **MCP (part 2):** builds on the `mcp` feature from #113.
  - A soft-proof tool returns the proof image and gamut statistics: fraction out of gamut, plus a coarse grid of where.
  - An export-for-print tool wraps the CLI flags.
  - No pipeline code beyond A.

## How a LittleCMS reference checks results

**Reference tool.** `tificc` from `liblcms2-utils` (LittleCMS 2.14 on Ubuntu 24.04). [`measure.py`](measure.py) step 5 shows that on a random 16-bit TIFF, these are **bit-identical** (max difference 0 in 16-bit codes) to the library calls `cmsCreateProofingTransform` and `cmsCreateTransform`:

- `tificc -t1 -b -m1 -p<profile> -w16` (proof, relative + BPC);
- `-m3` (paper simulation);
- `-t0 -m1` (perceptual);
- `-t1 -b -o<profile>` (conversion to the printer).

The `-t` flag is the intent into the printer, `-m` the intent from the printer to the display, and `-b` turns on BPC.

**Profiles for tests.** Committed fixtures must be redistributable:

- colord's generated print profiles (`/usr/share/color/icc/colord/`) are CC0-1.0, except the GRACoL and SWOP ones, which are under the NPES terms.
  - `FOGRA39L_coated.icc`: 122 028 bytes, SHA-256 `3ff7ca2a650ad47a8d2a929eb23ef162ccf979d013ade56e441c1f55156a261f`.
  - But its perceptual and colorimetric tables are effectively the same (mean 0.009 ΔE2000 between them), so it **can't test intent handling**.
- The unit tests should also build a synthetic RGB "paper" profile at test time with LittleCMS. It needs separate perceptual and colorimetric tables, a tinted media white and a raised black point, so that intent, paper simulation and BPC each change the result.
- Real paper profiles from paper makers can't be committed. Maintainers test with their own and report the profile names in the PR.

**Checks, in order of the implementation PRs:**

1. **Unit tests** (`cargo test --lib`): map each intent, BPC and paper-simulation option to the right LittleCMS flags, and check a fixed patch set (greys, primaries, skin, sky, out-of-gamut cyans) against values recorded from `transicc`/`tificc` and committed as CSV. RapidRoom would link 2.19 and the fixtures come from 2.14, so the tolerance is ΔE2000 ≤ 0.1 per patch; recording the fixtures with the vendored version makes them exact.
2. **Export, end to end:**
   1. Export an edit through the headless CLI as a 16-bit sRGB TIFF.
   2. Convert it with `tificc -t1 -b -o<profile> -w16` to get the reference.
   3. Export the same edit with `--icc-profile <profile> --intent relative --bpc`.
   4. Compare the two: same LittleCMS version → max difference 0; otherwise max ≤ 1 16-bit code or ΔE2000 ≤ 0.1. Also check that the embedded profile bytes equal the profile file.

   The test runs on a few corpus raws, with the script under `rapidroom/validation/`.

3. **Soft-proof preview:** the same comparison with `tificc -p<profile> -m1|3`, against a proof render saved through a debug or CLI option.
   - With A it should be exact.
   - If B is ever used, the limits come from the measurements: mean ≤ 0.1, p99 ≤ 0.5, max ≤ 1.5 ΔE2000.
   - The gamut overlay should agree with LittleCMS's gamut check on at least 97% of pixels (a 33³ alpha flag disagrees on 1–2% near the boundary).
4. **Default unchanged:** with no printer profile set, exports are byte-identical and the 60-image regression is identical. With A, no shader changes, so this is expected by construction, but it still has to be run.

## Measurements

Run with `python3 measure.py /usr/share/color/icc/colord/FOGRA39L_coated.icc` on LittleCMS 2.14, using one thread of a 4-vCPU Intel Xeon 2.1 GHz cloud container. The colour statistics use 200 000 random sRGB colours.

| Measurement                                                     | Result                                                       |
| --------------------------------------------------------------- | ------------------------------------------------------------ |
| sRGB 33³ grid flagged out of FOGRA39 gamut                      | 58.5%                                                        |
| FOGRA39 CMYK 11⁴ grid outside sRGB                              | 6.8%                                                         |
| ΔE2000 original vs proof (relative + BPC)                       | mean 4.78, p99 15.7, max 17.8                                |
| ΔE2000 perceptual vs relative (this profile)                    | mean 0.009, max 0.07                                         |
| ΔE2000 BPC on vs off                                            | mean 0.86, p99 2.75, max 4.83                                |
| ΔE2000 relative vs paper simulation                             | mean 2.74, max 3.55                                          |
| 17³ / 33³ / 65³ baked LUT vs LittleCMS, ΔE2000 mean / p99 / max | 0.12 / 0.55 / 1.75; 0.041 / 0.23 / 1.10; 0.014 / 0.09 / 0.56 |
| LittleCMS default 8-bit path vs unoptimised, ΔE2000             | mean 0.19, p99 0.62, max 2.33                                |
| Gamut flag from 33³ / 65³ alpha (threshold 0.5) vs LittleCMS    | disagrees on 2.1% / 1.1%                                     |
| Proof, 8-bit, 2 MP                                              | 58–79 ms; 150–164 ms with gamut check                        |
| Proof, 8-bit, 24 MP                                             | about 1.0 s; about 2.0 s with gamut check                    |
| sRGB → printer, 16-bit, 24 MP                                   | about 0.93 s                                                 |
| `tificc` vs library (proof ×3, output)                          | max difference 0                                             |

Throughput varied by about 30% between runs on this shared machine. The FOGRA39 numbers describe an offset press profile, not a photo paper. They show the method and the orders of magnitude, not what a photo print will look like.

### Local integration recheck (2026-10-04)

LittleCMS **2.19** on the local Intel machine reproduces the FOGRA39 color statistics above to the reported rounding. The script now uses the full CIEDE2000 mean-hue wrap rule and measures **float16**, matching the proposed RGBA16F texture, rather than 16-bit UNORM. The [Sharma, Wu and Dalal supplementary test data](https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/) checks all 34 pairs within 0.00005 ΔE2000; an identity field checks tetrahedral interpolation at 1002 points, including the endpoints.

The 33³ float16 LUT gives mean **0.043**, p99 **0.226**, max **1.103** ΔE2000 against unoptimised LittleCMS. The float64 LUT gives 0.041 / 0.225 / 1.100. These are CPU interpolation measurements, not a GPU accuracy or speed test. Timings are machine-specific: this run took 58 ms for a 2 MP 8-bit proof and 158 ms with gamut checking. Original cloud timings remain above with their original environment.

Local profiles came from colord-data 1.4.8 (CC0 profile notices retained in the distribution). The FOGRA39 and AdobeRGB measurements and the `tificc` checks are retained with the local validation logs. No photo paper profile, real print or monitor-profile validation is claimed. The six choices below remain proposals for Josh; merging this note does not approve or implement them.

## Decisions for Josh

1. **Output space:** proof and export from the sRGB render (recommended), or a wide-gamut output path first (rendering change, separate issue)?
2. **Library:** add `lcms2` with vendored LittleCMS 2.19 (`static`) (recommended), or wait for `moxcms` to gain BPC and proofing?
3. **Preview path:** CPU after readback (recommended first), or GPU baked LUT from the start?
4. **Display:** assume an sRGB monitor in the first version (recommended), or include display-profile support?
5. **Scope of the first version:** RGB printer profiles for export, CMYK for proofing only (recommended)?
6. **Test fixtures:** colord's CC0 profiles plus a synthetic profile generated in tests (recommended)?

## Plan after the decisions

One issue and PR each, in this order:

1. `lcms2` dependency and a `print_proof` module: load and validate profiles, build and cache transforms, apply them to RGB8/RGB16 images with rayon. Unit tests and fixtures.
2. ICC export: settings, CLI flags, encoding with the printer profile, and the end-to-end `tificc` check script.
3. Soft-proof preview with gamut warning and UI, on Linux first.
4. Export at print sizes (physical size, ppi, resolution tags).
5. MCP soft-proof and export tools (after #113's `mcp` feature).
6. Book help: skill and docs for sequencing, consistent looks across a set (presets, virtual copies), and print-size export.
