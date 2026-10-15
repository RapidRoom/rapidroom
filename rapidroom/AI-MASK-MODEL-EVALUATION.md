# Optional AI mask models: evaluation for #222

Evaluated 2026-10-14. This document proposes a sequence of experiments; it does not add model backends or downloads. The depth geometry tests from PR #239 are already merged. Keep #222 open for the remaining implementation and quality work. Imported Lightroom raster masks (#88) are independent of these models.

## Recommendation

Evaluate **SAM 2.1 Hiera Tiny first**, then **BiRefNet General-Lite** as explicit optional choices. Retain the current SAM v1, U2Net subject, SkySeg and DA2 Small paths. Defer BiRefNet Lite-2K/HR until General-Lite demonstrates a useful boundary improvement within the machine's memory and responsiveness budget. Defer DA3 Mono Large until its exact ONNX conversion is reproducible and its polarity/interface are verified.

No candidate has a measured speed or quality result on this machine yet. The local DA2 timing below is a baseline, not evidence that a newer model is faster. All choices require comparative masks and human inspection before shipping.

## Licence and artifact audit

The reviewed CalibRaw source is pinned at [`2466ca6662c3fd1e901377c0450b19f5f96d3e85`](https://github.com/Duecki1/CalibRaw/tree/2466ca6662c3fd1e901377c0450b19f5f96d3e85). Its code is GPL-3.0-or-later: any substantial adaptation must retain original notices, authorship and credits. The following weight terms are separate from that code licence.

| Candidate                          | Reviewed weight terms                                                                                                                                                                 | Decision                                                                                                                                                                                                                                                                             |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| SAM 2.1 Hiera Tiny                 | Meta explicitly licenses checkpoints under Apache-2.0. The pinned ONNX converter's card declares Apache-2.0, and the mirror records the paired payloads and original conversion URLs. | Eligible for an opt-in experiment; preserve the Apache licence and applicable notices. [Meta](https://github.com/facebookresearch/sam2#license), [conversion card](https://huggingface.co/akiyamanx/sam2.1-hiera-tiny-onnx/blob/aa11669045f8d82c74e46f8f77c9b56792c90ebb/README.md). |
| BiRefNet General-Lite, Lite-2K, HR | MIT upstream code; official Lite and HR cards also identify MIT weights. The mirror maps each exact ONNX payload to a named v1 release asset.                                         | Eligible; keep the MIT copyright/permission notice. [Licence](https://github.com/ZhengPeng7/BiRefNet/blob/main/LICENSE), [Lite card](https://huggingface.co/ZhengPeng7/BiRefNet_lite), [HR card](https://huggingface.co/ZhengPeng7/BiRefNet_HR).                                     |
| DA2 Small                          | Apache-2.0. DA2 Base/Large/Giant weights are CC BY-NC 4.0.                                                                                                                            | Keep Small; exclude the NC variants from this FOSS selection. [Official family terms](https://github.com/DepthAnything/Depth-Anything-V2#license).                                                                                                                                   |
| DA3 **Mono Large**                 | The exact Mono Large card says Apache-2.0. CalibRaw's desktop ONNX is absent from the reviewed mirror conversion inventory.                                                           | Provenance hold on that conversion. Do not substitute another DA3 family member's terms or infer licence from a filename. [Official card](https://huggingface.co/depth-anything/DA3MONO-LARGE).                                                                                      |

The mirror's [pinned artifact inventory](https://huggingface.co/Duecki/CalibRaw-Artifacts/blob/91085ce0ec322a4a7cbd20059688690218e52f9a/ARTIFACTS.md) was read, rather than treating its repository licence as a grant for every weight. Its inspection describes an older CalibRaw revision; it does not certify the later DA3 export. The SAM conversion card supplies a licence declaration only, not conversion commands or numerical equivalence evidence. Candidate payloads were **not downloaded or independently hashed** for this evaluation. The descriptor hashes below are source declarations, not our verification receipts.

## Download size and inference shape

Sizes and hashes come from the pinned CalibRaw descriptors: [`ai_masks.rs`](https://github.com/Duecki1/CalibRaw/blob/2466ca6662c3fd1e901377c0450b19f5f96d3e85/crates/calibraw-ai/src/ai_masks.rs), [`object.rs`](https://github.com/Duecki1/CalibRaw/blob/2466ca6662c3fd1e901377c0450b19f5f96d3e85/crates/calibraw-ai/src/ai_masks/object.rs), [`depth.rs`](https://github.com/Duecki1/CalibRaw/blob/2466ca6662c3fd1e901377c0450b19f5f96d3e85/crates/calibraw-ai/src/ai_masks/depth.rs). MB below means decimal bytes / 1,000,000. These are download sizes, not peak RAM estimates.

| Artifact                                    |   Exact bytes |       MB | Native input / execution considerations                                                                                    |
| ------------------------------------------- | ------------: | -------: | -------------------------------------------------------------------------------------------------------------------------- |
| SAM 2.1 Tiny encoder                        |   109,471,931 |   109.47 | 1024 × 1024; cache encoding per image/geometry/model identity.                                                             |
| SAM 2.1 Tiny decoder                        |    16,519,561 |    16.52 | Paired encoder features plus prompts; measure each click separately. Combined payload: **125,991,492 bytes**.              |
| BiRefNet General-Lite                       |   224,005,088 |   224.01 | 1024 × 1024; initial subject-quality experiment.                                                                           |
| BiRefNet Lite-2K                            |   331,082,421 |   331.08 | Tensor width 1440 × height 2560; preserve the converter's axis contract.                                                   |
| BiRefNet HR                                 | 1,098,928,953 | 1,098.93 | 2048 × 2048; defer while activation memory is unmeasured.                                                                  |
| DA3 Mono Large ONNX                         |   731,358,963 |   731.36 | 700 × 700; conversion provenance and direct-depth polarity still need verification.                                        |
| Current RapidRoom DA2 Small, locally cached |    99,373,606 |    99.37 | 518 × 518 FP32; this is the measured existing RapidRoom graph, **not** CalibRaw's different 99,060,839-byte mobile export. |

The pinned mirror revision for SAM/BiRefNet is `91085ce0ec322a4a7cbd20059688690218e52f9a`; DA3 is `4b82010fd8654fc3a1c33311ff305d793ec2f511`. Exact declared SHA-256 values:

| Artifact              | SHA-256                                                            |
| --------------------- | ------------------------------------------------------------------ |
| SAM encoder           | `667384d1e686de6828b841ac8a24db0fafa2b3452494225f82eeedac56141230` |
| SAM decoder           | `c40f5aa7d37b681cd500481a85d44839fd81c93dce1e86271a2c866470d22105` |
| BiRefNet General-Lite | `5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333` |
| BiRefNet Lite-2K      | `6003d2f758bdb4e4802a09e39167529bc2eef9288d5b8fa537331467cbc4759d` |
| BiRefNet HR           | `db0217e99b25e0c4f6f4dca2892ff1f7ea7aba38fb6ad84f93122a4024be536a` |
| DA3 Mono Large        | `71079fb3c7d3b04e9df9d157e0b3ee0e6614cb5198f0729e7b490512c4f8d667` |

Any future download needs consent showing model, source, licence, exact byte size and cache cost. Use immutable URLs, validate size and SHA-256 before loading, install atomically, and support an offline cache. Downloading one backend must not fetch every candidate. A cancelled or failed install must leave the existing backend usable.

## Speed on the evaluation machine

Hardware inspected locally: Intel Core Ultra 7 258V, eight cores/eight threads, approximately 30 GiB usable RAM and Intel Arc 130V/140V integrated graphics reported by PCI enumeration. No CUDA device is available. Current RapidRoom mask sessions use ordinary ONNX Runtime session construction without an explicitly selected accelerated provider in `src-tauri/src/ai_processing.rs`; integrated graphics support alone does not establish mask acceleration. OpenVINO denoise work in #137 is not a performance result for these graphs.

The [local measurement receipt](evaluations/222-da2-cpu-baseline.json) uses an existing cached DA2 Small graph: ONNX Runtime **1.30.0**, CPU provider, four intra-op threads, one inter-op thread, one warm-up and five timed runs. A deterministic normalized RGB tensor has shape `[1,3,518,518]`; output shape `[1,518,518]`, all finite. Model identity was SHA-256 checked outside the run lock.

| Measured item                 |                              Result |
| ----------------------------- | ----------------------------------: |
| Session creation              |                             0.480 s |
| Warm graph runs               | 0.998, 1.004, 0.905, 0.922, 0.849 s |
| Warm median / maximum         |                 **0.922 / 1.004 s** |
| Whole Python process peak RSS |         496,580 KiB (about 485 MiB) |

This is a CPU graph baseline on a synthetic input, not native-app end-to-end latency, a semantic quality test, or an isolated CPU benchmark: other agents could compile concurrently. It excludes photo decoding, resize, map normalization, mask persistence and preview composition. The run held only the model/app lock, used no NAS reads, and downloaded no weights.

| Candidate      | Local latency evidence | Next timing needed                                                                                         |
| -------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------- |
| SAM 2.1 Tiny   | **Unmeasured**         | Session load, image encode, first click, repeated positive/negative clicks, cache invalidation.            |
| BiRefNet tiers | **Unmeasured**         | Each native input shape, session load, warm inference, postprocess, optional crop refinement and peak RSS. |
| DA3 Mono Large | **Unmeasured**         | Only after provenance hold is resolved: load, warm run, geometry restore and peak RSS versus DA2.          |

Published GPU figures do not predict this machine. Meta's Tiny result is 91.2 FPS on an A100 using PyTorch/CUDA, and BiRefNet reports 17 FPS at 1024² FP16 with 3.45 GB GPU memory on an RTX 4090. Different runtime, device, precision and workload prevent converting those figures into Intel CPU/Arc latency. [SAM methodology](https://github.com/facebookresearch/sam2#model-description), [BiRefNet efficiency](https://github.com/ZhengPeng7/BiRefNet#model-zoo).

## Integration contract for a later implementation

Keep model selection explicit per new mask operation. Existing stored mask bitmaps remain authoritative and are never regenerated on opening a photo, switching a default or upgrading a model. Store the chosen backend/version as provenance for newly generated masks; preserve old sidecars and MCP behavior. Failed inference must not replace the current mask.

SAM needs a separate paired-model adapter; SAM v1 tensor names are not a compatible interface. A cache key must include source pixel identity, orientation, crop/geometry, encoder version and preprocessing. Bound both memory and entry count. Changing any key invalidates cached features. Prompt coordinates must map through letterbox/crop transforms exactly once; negative clicks and candidate selection need dedicated fixtures. Candidate cleanup must retain intentional holes and thin structures.

BiRefNet should preserve probabilities through resizing, validate output batch/channel/shape and finiteness, and avoid hard thresholding before edge refinement. Begin with General-Lite. CalibRaw deliberately keeps its medium/high tiers on CPU to protect the interactive GPU; that policy is a useful experiment, not proof that this machine can afford those tiers. Measure editor responsiveness while inference runs, cancellation, repeated images and session release.

Keep DA2's existing near/far meaning. DA3 Mono predicts direct relative depth while DA2 is disparity-like, so normalizing both without an explicit polarity adapter can invert selections. Exclude padding before normalization; test constant/NaN/Inf outputs and depth outliers before considering percentile normalization. A robust-normalization change can alter existing depth-mask results even without new weights and needs its own reviewed rendering evidence.

## Experiment and acceptance plan

1. Finish a reproducible conversion record for each proposed artifact: original checkpoint revision/hash, exporter revision and commands, precision/opset, external-data files, graph input/output signatures and numerical comparison with the original. The current SAM card alone does not satisfy this reproducibility checklist. For DA3, this is a prerequisite to the first download/integration experiment.
2. With user consent, download only the selected, pinned candidate into an owned cache. Verify payload size/hash outside either build/run lock. Compile an adapter under the build lock with four jobs, incremental off and nice 10; pin the resulting executable outside the shared target before releasing that lock.
3. Under the separate model/app lock, compare the existing and candidate backends on original synthetic fixtures and public licensed photos. Record actual runtime/provider, cold load, warm median and p95 (at least 20 runs), input geometry, end-to-end latency, output identity and peak RSS. Repeat on an otherwise idle CPU; do not confuse shared-build measurements with idle-machine results. A CPU fallback must succeed without CUDA/OpenVINO, and no UI thread may block on inference.
4. Check portrait/landscape/square/thin/odd dimensions, EXIF orientation, crop/rotation, letterbox edges, repeated prompts, cancellation and corrupt/missing weights. Use night subjects, hair, fences, windows, thin wires and holes for mask quality. Keep private photo panels local if used. Judge soft boundary leakage, missed fine detail and click consistency; benchmark claims alone do not establish better masks.
5. Proposed first acceptance budgets on this class of machine: cached SAM click median ≤250 ms, image encode ≤3 s; General-Lite whole operation ≤8 s; no operation pushes the app above 6 GiB additional peak RSS. These are experiment targets, not measured results or promised product specifications. If targets fail, retain the existing choice and report actual numbers before exposing the candidate.
6. Require focused adapter tests, native app/MCP checks, the applicable exact-head local/hosted gates and unchanged default renders. Intended changes to opted-in mask output need labelled before/after panels and human approval. Do not regenerate existing bitmap masks or close #222 merely because this document or PR #239 landed.

Next implementation order: SAM adapter/provenance and timing → optional General-Lite quality experiment → measured higher tiers if justified → separately approved depth normalization or DA3 conversion. SkySeg replacement remains outside this evaluation because the mirror weight provenance is incomplete.
