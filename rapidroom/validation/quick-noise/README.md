# Quick noise reduction: measurement and quality evaluation (#260)

This slice supplies paired synthetic fixtures and a design contract. It does not
implement an estimator, suggest slider values, or execute the production shader.
The current manual values, saved edits and defaults remain unchanged. Issue #260
stays open for a production-shader quality runner and any separately approved
measurement/Estimate implementation.

## Source comparison and decision

Reviewed RapidRoom base `271c9ed2b7c83900bad8210c34bfb962cbf227ae` and
[RustRunner/RapidRAW-MKII at the pinned revision](https://github.com/RustRunner/RapidRAW-MKII/tree/555f7c6ddd5b6b5d219f87e3552a334968e9ddd9).
Exact input hashes are in [provenance.json](provenance.json).

| Contract     | RapidRoom                                                                                                                                     | Pinned donor                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Controls     | Quick NR Luminance and Color; global 0–100, local masks −100–100; defaults zero                                                               | Separate Strength, Detail and Chroma for `apply_denoise`; it also retains the older two-control filter |
| Shader input | `shader.wgsl::apply_noise_reduction`: RAW RGB stays linear; non-RAW RGB is sRGB-decoded, including neighbor taps                              | `apply_denoise` receives linear RGB; its calibrated controls are not our two-control filter            |
| Spacing      | `gpu_processing.rs::run` supplies min(width,height)/1080; kernel uses clamp(sqrt(scale),0.5,2), strength-dependent strides and hashed offsets | Chroma uses max(1, round(scale)), with fitted integer spacing 1–5; Detail alters luma protection       |
| Filter       | Two robust luma-weighting passes over 25 samples; separate 25-sample chroma weights and reconstruction                                        | Different luma radius/strength/Detail mapping and chroma kernel, plus its own calibration tables       |
| Placement    | After CA center sampling and mask accumulation, before sharpen/local contrast and tone adjustments                                            | Separate denoise stage plus legacy NR; calibrations depend on that production WGSL                     |
| Measurement  | No measured Quick NR Estimate action in `Details.tsx`                                                                                         | Measurement v3, calibrated explicit Estimate, generation-owned source cache                            |

The relevant RapidRoom source is [shader.wgsl](../../../src-tauri/src/shaders/shader.wgsl),
[GPU request setup](../../../src-tauri/src/gpu_processing.rs),
[control normalization](../../../src-tauri/src/image_processing.rs),
[Details controls](../../../src/components/adjustments/Details.tsx), and
[zero defaults](../../../src/utils/adjustments.ts). UI values are divided by 100
before the shader clamps combined global/mask amounts to [0,1]. Mask negatives
reduce a global amount; they are not a second negative-strength filter.

**Decision:** take fixture families and residual metrics with attribution; do not
copy the estimator, donor shader, fitted tables or rejected enhancement variants.
Our source uses a different spacing/strength response. Even identical source
noise does not imply identical useful slider values. Future fits must use our
actual WGSL and native dimensions, with its shader/measurement/transfer version
bound into the calibration identity. No ISO-to-slider rule is justified here.

The donor's final reports describe 10,200 held-out cases with unsupported domains
refused. Its earlier failed MAD audit is historical; final v3 measures developed
black-clipped residual variance separately. These are donor-reported results,
not runs on RapidRoom or this machine. The wider-chroma and stabilized-edge
variants were rejected for fine-detail/noise tradeoffs; completing those
experiments did not qualify a replacement filter. Donor timings on NVIDIA GB10
are not speed predictions for this machine.

## Runnable paired fixtures

Run from the repository root with Python 3.10+ (standard library only):

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s rapidroom/validation/quick-noise -p test_fixtures.py -v
python3 rapidroom/validation/quick-noise/fixtures.py --out /path/to/new-references.json
```

The output path must be new. The generator produces 48 small cases and keeps
pixel arrays in memory; the report contains construction parameters and paired
reference metrics. It does not upload/download data or read photographs. The
output is deterministic for a given Python math implementation; cross-platform
floating-point bit identity is not an acceptance gate.

Fixtures adapt the donor's Y/Cb/Cr textures at periods 4/8/16, thin color lines
at widths 1/2/4, luma edge, color gradients, and paired residual metrics. Added
star/colored-LED points prevent treating isolated real signal as a defect.
White and block-correlated Gaussian RGB perturbations, clipping and 8/16-bit
quantization have explicit construction metadata. Noise is injected in encoded
RGB, then both clean/noisy members receive the same clipping/quantization and
optional sRGB decoding. These fixtures represent developed RGB, never a sensor
noise model. Block correlation is a stress case, not a reproduction of all donor
box-filter/preprocessing cases. No independent release holdout is claimed.

Each domain reports paired Y/Cb/Cr residual mean, centered population variance,
MSE and Gaussian-normalized MAD scale. MSE includes bias; constant bias must
produce nonzero MSE and zero centered variance. A mostly black clipped residual
can have zero MAD and nonzero variance. Transfer-domain values differ; a linear
sigma cannot be relabeled as encoded sigma. Sub-code-step noise can quantize to
zero observed residual despite positive injected noise: it is unresolved, not
evidence of zero source noise. These are ground-truth fixture metrics; there is
no source estimator or claim of source qualification in this Python module.

The controls exercise bias, clipping, quantization, transfer equivalence,
seed/correlation construction, finite-data refusal and deliberately destroyed
fine detail/stars/LEDs. They establish that the references can detect those
failures. They do not establish GPU quality, realistic denoising, timing,
clipped-class coverage or absence of false positives in the app.

## Production measurement proposal (not implemented)

1. Analyze an immutable, native-resolution decoded source snapshot on explicit
   user action only. Capture image content identity, decode generation, transfer
   domain, native dimensions and preprocessing version. Avoid fit-preview or
   histogram pixels. JPEG/8-bit/16-bit/float input code steps are explicit.
2. Bound work and diagnostics: proposed maximum 256 native 64×64 patches, with
   fixed sampling origins and bounded per-bin summaries rather than source pixel
   payloads. Check cancellation between patches. The donor's patch limit is a
   starting engineering budget, not validated RapidRoom coverage/latency.
3. Keep robust Gaussian MAD scale, actual developed residual variance, clipping
   bias, quantization limits, texture and short/long correlation distinct. Any
   fitted planar-residual method must qualify each supported combination against
   paired truth. Saturation, insufficient patches/bins, uncertain transfer,
   unresolved quantization and unvalidated correlation return an explicit
   unsupported/insufficient result; never an invented zero or maximum slider.
4. Bind a pending request to image identity and generation. Recheck before
   publishing and before applying; switching photos, reload, cancellation or
   identity replacement discards stale results. Source measurements may be
   cached by immutable source identity; derived suggestions additionally depend
   on calibration/shader version and native settings and are not source cache
   properties. Analysis must never change pixels, history or sidecars.
5. Capture global Luminance/Color values and an edit revision for application.
   Start with global controls only. Do not derive masked suggestions from mixed
   global/local contributions. Reject stale edits or ask for a new explicit
   action; never silently overwrite a user's slider movement. Apply both values
   atomically as one undoable history entry, with undo/redo preserving the prior
   numeric values. Collapsing a panel does not change request ownership. Save
   persists numeric settings only, without hidden estimate payloads.
6. Build RapidRoom-specific fits only after useful coverage and quality are
   demonstrated. Freeze method, training cases and calibration axes before
   allocating untouched validation seeds/cases. Unsupported interpolation
   corners refuse lookup. Do not inspect/tune against the independent holdout,
   then relabel it as fresh validation. Manual values and default zero remain.

## Required follow-up acceptance matrix

| Area                       | Required evidence before production Estimate                                                                                                                                                                                                                                        |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Noise/detail               | Actual production WGSL input/output on paired clean/noisy scenes, separate Y/Cb/Cr bias/variance/MSE, native scale sweep, thin lines/textures/gradients, bright/black clipping, neutral/saturated colors, 8/16/float quantization, short/long correlation and mixed texture refusal |
| False positives            | Clean stars, colored LEDs and fine single-pixel detail remain signal; noise variance reduction alone cannot pass a clean-detail regression                                                                                                                                          |
| Transfer and preprocessing | Paired linear/encoded sources with declared injection point; demosaic/sharpen/decode correlation; CA center versus neighbor behavior; actual import decoding and output transfer                                                                                                    |
| Preview/export             | Fit/100%/multiple preview sizes and native dimensions; document scale-dependent results rather than asserting equal preview/export pixels; exact neutral export/reopen, tile boundaries and full-frame context                                                                      |
| Fit/holdout                | RapidRoom-specific shader-bound tables, separate frozen fit and independent holdout, per-domain support/refusal counts; no donor thresholds promoted without local justification                                                                                                    |
| Native lifecycle           | Actual pending request plus photo switch/reload/cancel, panel collapse, stale edit revision, repeat Estimate, one history step, undo/redo, Save/reopen and no result after failure                                                                                                  |
| Release                    | Required local gates, pinned release/native checks under separate locks, default60 exact, exact-head hosted CI, independent source review and human panels for any changed rendering                                                                                                |

GPU validation must run the actual unchanged shader, with explicit margins at
least as wide as the maximum sample reach for the selected scale; otherwise
boundary clamping can dominate results. Current full-array Python metrics are
reference construction checks, not those cropped GPU quality gates. Record full
noise/detail distributions and worst locations, rather than only average error.
Set and justify pass/fail bounds before evaluating candidates. Timing reports
must separate upload/readback, filter work and full export, disclose warmup and
repeated samples, and use the GPU lock with no competing app/engine work.

## Attribution and local verification

AGPL-3.0; RustRunner authored the pinned donor. Fixture families, conversion and
centered residual metric formulas were adapted from
`src-tauri/src/gpu_processing/denoise_tests/qualification.rs`; the remainder is
original RapidRoom test support. The source retains its AGPL license. No donor
tables, shader snapshots, photographs, model weights or private reports are
bundled. [CREDITS.md](../../../CREDITS.md) records this adaptation.

Local fixture controls: nine tests passed. Full fixture generation and standard
repository gates are recorded separately in the PR. No donor test, production
GPU fixture, noise measurement, native Estimate interaction or performance
benchmark has been executed for this slice.
