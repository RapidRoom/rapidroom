# Adobe lossy DNG polynomial mapping

Issue #26 fixes full-resolution Adobe lossy DNGs that rendered pink or almost completely white. The rawler patch applies OpcodeList2 MapPolynomial after linearization and per-plane black/white normalization and before demosaic, preserving ordered operations, areas, planes, pitches and output clipping. Converted DNGs omit mappings that have already been baked into their pixels.

The original implementation follows the [Adobe DNG 1.7.1 specification](https://helpx.adobe.com/content/dam/help/en/camera-raw/digital-negative/jcr_content/root/content/flex/items/position/position-par/download_section_733958301/download-1/DNG_Spec_1_7_1_0.pdf), with existing rawler LGPL notices retained. It does not implement all DNG opcodes. Unsupported-only lists retain the preceding behavior; unsupported required operations mixed with polynomial mappings report an error instead of processing a partial sequence.

Local Linux x86_64 prototype validation, application base `f4d7472e`, rawler `06742401`:

- 58 rawler tests pass, including five new mapping/parser tests; app fmt and strict locked Clippy pass, 213 Rust tests pass (three ignored), 83 Vitest tests pass, locked release build passes.
- **60/60** default and busy corpus renders are pixel-identical to the blessed `baseline-int7` reference, with matching environment and no failures or missing files. Render TIFFs were removed; comparison JSON and manifests remain locally.
- Two full-resolution 7008×4672 LinearRaw samples from upstream issue `CyberTimon/RapidRAW#1542` reproduce the original defect and render with natural colours after the change. Output is finite RGB at unchanged dimensions. The result differs in brightness from the embedded Adobe preview; Adobe rendering/profile/exposure parity is not promised.
- The supplied Sony A7R V Lightroom DNG has mapping tags only on its reduced proxies. Its full 9504×6336 render is pixel-identical before/after, maximum pixel difference zero; it is an unchanged control, not a full-image reproduction.
- Uncompressed DNG conversion/reopen preserves every normalized decoded pixel exactly and omits the baked OpcodeList2 tag, preventing double application.

The sample photographs have no CC0 declaration. All originals, TIFFs and before/after contacts remain under the project's local `samples/issue-26/` directory and are not committed or uploaded. Detailed logs, scripts and comparison JSON are in local `work/issue-26/`. The pinned prototype engine has SHA256 `f7b46498e0f20052c104e4444fd7987ffd17c1308fec9efc5ab6f0c47eafb8fa`.

Current-main integration and portable-pin results are recorded on the PR. The coordinator authorized the own-fork dependency branch and normal merge rules because the 60-render corpus is unchanged. Human desktop/visual review and macOS/Windows are untested; actual CFA and floating-point DNGs with polynomial tags have synthetic coverage only.
