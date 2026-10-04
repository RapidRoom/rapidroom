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

These are single local runs in the optimized Rust test profile with four CPU threads per session. They include decoding, preprocessing, tiled inference, CFA restoration and cache publication, but exclude model initialization, source-file reading and developed previews. They are not end-to-end dialog timings or a quality-equivalence claim. The earlier private RawForge review used OpenVINO CPU and is not an interchangeable runtime benchmark.

Before merge, also require fmt, strict Clippy, library tests, Vitest, generated status checks, a locked release build and the 60-image default regression. Final evidence belongs in the PR.

Human check: Best/Fast selection, Sharpen checkbox, first-use download, before/after preview and saved DNGs at fit and 100%, closing/cancelling then reopening on another photo, batch cancellation, saving/opening a denoised result, and the collapsed basic sliders including existing non-zero sidecars. Full-frame/phone-size sharpening preference and focus-aware denoise remain separate follow-ups.
