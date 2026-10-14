# AI raw denoise

Issue #91 makes AI raw denoise the main Denoise dialog path. Best uses TreeNetDenoise; Fast uses TreeNetDenoiseSuperLight. Sharpen is independent and defaults on, adding DeepSharpen after either model. Existing NIND, BM3D and Apple RAW 9 remain under More methods. Quick luminance/chroma reduction remains opt-in under a collapsed basic section; existing saved adjustment values are preserved.

## Source and licence check (2026-10-04)

[RawForge](https://github.com/rymuelle/RawForge/blob/79f17700c29765b28a4cf5bd1b6d90bc7e3eb7f2/LICENSE) declares MIT, copyright 2025 rymuelle. The Rust implementation adapts the RawForge 0.2.4 preprocessing/model contracts, RawHandler 0.2.2 colour conversion (MIT, same author), and blended-tiling-numpy's overlap scheme (MIT, copyright 2022 ProGamerGov). Malvar preprocessing follows colour-demosaicing (BSD-3-Clause, Colour Developers). Notices are retained in `src-tauri/resources/raw-denoise-licenses/` and included in the existing Tauri resource bundle.

The author publishes the weights in [RawForge's onnx_v1.0.0 release](https://github.com/rymuelle/RawForge/releases/tag/onnx_v1.0.0), under the MIT project's release channel. That release has no separate per-checkpoint licence file or model card; this is a repository-level licence statement, not an independently stated model grant. No weights are bundled, mirrored or re-licensed by RapidRoom. First use downloads directly from the author's release and verifies its published SHA-256. RawForge credits RawNIND and NAFNet for training; their datasets are not redistributed here.

| Preset/stage | Asset                             |    Bytes | SHA-256                                                          |
| ------------ | --------------------------------- | -------: | ---------------------------------------------------------------- |
| Best         | ShadowWeightedL1.onnx             | 10073980 | 76a07047a33dba3fa27330cec483b43bb57ecb1c1f10db8d979355ce110e34c9 |
| Fast         | ShadowWeightedL1_super_light.onnx |   835451 | 1684f549fec52812ffacc3020bfd61c5fde610e77460fcc03b2a7003f61e60dd |
| Sharpen      | Deblur_deep_24.onnx               |  9961220 | 4b714af59026352990f5cc1b84af5dc01cf4a66abd0b5f8d845b0c43d99511a7 |

The assets match those used in the maintainer's local blind reviews. Those private photographs and crops are never bundled or uploaded. Downloading models sends an ordinary request to GitHub and its release CDN; no image bytes, paths, metadata or ratings leave the machine.

## Implementation contract

Use RapidRoom's existing ONNX Runtime, CPU execution (up to four threads per session) and float16 model inputs. Condition on the original ISO divided by 6400 (capped at 65535 before division). Bayer normalization/colour conversion and Malvar interpolation precede inference; no tone curve, editor exposure or white-balance adjustment is baked into model inputs. The optional sharpen stage consumes the first model's output. Map the result back to the original CFA before the normal RAW development path, preserving source metadata, crop and orientation.

Cache identity includes source content, model hashes, preset, sharpening, sensor geometry/calibration, CPU thread count and pipeline version. Cache publication is atomic, corrupt entries are discarded, and cancellation checks precede publication and preview/result storage. Save writes the cached CFA as a DNG (including in batch mode); opening it uses the normal RAW editor. Changing editor adjustments does not rerun inference. Older methods keep their existing TIFF/PNG outputs. The private cache is bounded at 2 GiB in the app cache directory (`raw-denoise`), evicting older completed entries. Default rendering bypasses this entire opt-in path.

The first implementation supports three-colour Bayer RAWs at least 256 pixels per side with compatible black/white levels, a usable daylight camera matrix and ISO metadata. Repeating black levels with an odd active-area shift are rejected when the pinned DNG reader could not preserve their phase. X-Trans, monochrome, linear DNG and non-Bayer reduced-resolution RAWs use More methods. Camera matrices come from RapidRoom’s pinned rawler camera database or embedded RAW metadata; this is not a claim of pixel identity with RawForge/LibRaw.

## Validation and human checks

Numerical Bayer/Malvar/tiling fixtures cover all four Bayer phases and irregular last tiles. DNG roundtrips cover CFA, active/default crop, black/white levels and orientation. Cache corruption, batch cancellation and stale job-ID protection have automated coverage. Local probes ran all three real checkpoints with the shipped ONNX Runtime 1.22 CPU library and verified cancellation before the second tile.

All four combinations ran on a local Sony A7C II (ILCE-7CM2), ISO 12800, 7040 × 4688 sensor buffer. Each output remained a finite Float32 CFA DNG with the source dimensions, active/default crop, black/white levels, white balance, ISO, camera identity and orientation. Each cache hit bypassed inference; changing the preset or Sharpen option missed the cache.

| Preset | Sharpen | Sensor stage | Cache hit |
| ------ | ------- | -----------: | --------: |
| Best   | On      |      399.8 s |   0.081 s |
| Best   | Off     |      193.5 s |   0.091 s |
| Fast   | On      |      271.6 s |   0.100 s |
| Fast   | Off     |       63.7 s |   0.083 s |

These are single local runs before the DNG write-buffer fix, in the optimized Rust test profile with four CPU threads per session. They include decoding, preprocessing, tiled inference, CFA restoration and cache publication, but exclude model initialization, source-file reading and developed previews. They are not end-to-end dialog timings or a quality-equivalence claim. The earlier private RawForge review used OpenVINO CPU and is not an interchangeable runtime benchmark.

The pinned DNG writer emits Float32 samples individually. A 1 MiB buffer reduced a standalone 33 MP DNG rewrite from 27.45 s to 0.134 s, with identical bytes and checksum; the implementation now buffers those writes and flushes before syncing the file. This changes neither model precision nor the Sharpen default. Provider/precision acceleration remains a separate investigation.

A full Fast + Sharpen check after that fix took 236.4 s (cache hit 0.088 s), with the same sensor/metadata/cache assertions passing. Timings remain single runs; the standalone byte-identical rewrite isolates the buffering improvement from run-to-run inference variation.

Before merge, also require fmt, strict Clippy, library tests, Vitest, generated status checks, a locked release build and the 60-image default regression. Final evidence belongs in the PR.

Human check: Best/Fast selection, Sharpen checkbox, first-use download, before/after preview and saved DNGs at fit and 100%, closing/cancelling then reopening on another photo, batch cancellation, saving/opening a denoised result, and the collapsed basic sliders including existing non-zero sidecars. Full-frame/phone-size sharpening preference and focus-aware denoise remain separate follow-ups.

## Legacy NIND encoding and neutral saves (#28)

The original RGB NIND inference driver loads 8/16-bit RGB files and scales them to floats without removing their file encoding: [primary source loader at 272428e60ef8a0a281135db0c39a4a04af802e6b](https://github.com/trougnouf/nind-denoise/blob/272428e60ef8a0a281135db0c39a4a04af802e6b/src/common/libs/np_imgops.py). RapidRoom develops RAW input through the same default GPU RAW renderer used for the original photo, including its basic display curve or configured RAW tonemapper. That encoded 16-bit RGB view is supplied to legacy NIND; ordinary RGB inputs are already encoded and pass through once. The model does not receive a separate approximation of the RAW display curve, and the saved TIFF does not skip the source's RAW look. The legacy model view is then bounded to [0, 1], matching the original integer-file loader; negative/HDR highlights outside that range are clipped only in this view. Nonfinite inputs are refused. The original file, sensor data and other denoise methods are unchanged; legacy NIND TIFF output does not preserve scene-linear HDR range. This follows the original RGB inference contract, rather than treating this RGB model as a RAW denoiser. The converted ONNX checkpoint's published hash identifies our existing artifact; it has no separate training-transfer model card, so original-driver correspondence is not independent checkpoint provenance or a universal quality claim.

The legacy NIND result is already an encoded RGB image. It is saved directly as a neutral TIFF/PNG, without the previous fixed gamma 2.38/contrast 1.28 display curve. Its preview compares it against the same default-developed encoded original; Best/Fast, BM3D and Apple RAW 9 keep their existing contracts. This default RAW look is baked into the RGB model view; scene-linear HDR latitude is not retained. GPU development failure is reported before inference rather than silently supplying linear samples. The existing save path still copies edit sidecars; a baked RGB result does not regain sensor RAW latitude or automatically reproduce every RAW-only camera adjustment.

All legacy quality settings now retain context: Faster has a 12-pixel halo/24-pixel overlap, Balanced has 28/32, and Higher quality has 64/64, with the existing 504-pixel model input. The larger retained border rejection costs extra tiles; quality/timing evidence belongs in the PR. The dependent #172 implementation below supplies normalized seam geometry, float32 accumulation and whole-image colour restoration in the encoded NIND input domain. These changes affect selected legacy NIND processing only; untouched photo exports do not invoke it.

## Seam and colour follow-up (#172, partial)

Best/Fast and legacy NIND now move overhanging final tiles back to the image edge, feather each actual overlap with a raised cosine, normalize the separable weights and accumulate in float32. The unchanged float16 Best/Fast graphs still receive float16 tensors; conversion occurs after accumulation, never per weighted contribution. Whole-image five-level à trous B3-spline restoration replaces the model's low frequencies with the original's in the same input domain, after the complete denoise/sharpen chain. Best/Fast retains the existing editable sensor DNG save contract and uses a new pipeline cache identity. NIND no longer clips individual contributions; final image encoders bound their representable range. NIND rejects malformed or nonfinite graph outputs. Cancellation is checked during wavelet passes before publication.

The pure Rust star-field control checks faint peaks and aperture flux near tile intersections and irregular image edges. It validates stitching only: learned-model faint-star survival, PSF spread and colour need measured real-night before/after evidence and human review. No Lightroom equivalence is claimed.

The AutoShade noise-aware recipe remains deferred: its generalized Anscombe transform and 71% grain-return calibration were measured for its separate fine-tuned weights, which issue #172 excludes. TreeNet's fixed ISO-conditioned linear-RGB tensors cannot be replaced with that transformed input without validating a new model contract. Post-demosaic luminance return needs both source and denoised developed images; baking it into the CFA can create colour speckle and cannot preserve our current sensor-DNG save semantics. A future explicit grain setting therefore needs a renderer/cache/save design and per-model calibration, rather than copying the 71% constant. No AutoShade weights or code are included in this patch.

CalibRaw's scalar per-tile mean gain matching was inspected and deliberately left out: a local object or model-dependent bias changes tile means and can introduce exposure steps between overlapping tiles. Colour correction is performed once over the assembled image.
