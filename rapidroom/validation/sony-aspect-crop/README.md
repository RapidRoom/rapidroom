# Sony in-camera aspect crop validation (#92)

Sony records the selected aspect in `SonyCropTopLeft`/`SonyCropSize` separately from the full-image `DefaultCropOrigin`/`DefaultCropSize`. The fork now prefers a validated smaller Sony rectangle and exposes the original default area. RapidRoom develops the original area and seeds the selected rectangle as a normal editable crop, in oriented image pixel coordinates. Resetting/expanding the crop retains the full recommended frame.

Existing non-null sidecar adjustments remain unchanged, even when they have no crop key. Explicit crops and `crop: null` overrides take precedence. XMP dimension probing continues to use the full editing frame. The fork retains the earlier Sony M/S/Canon default-crop fix and saturating highlight dithering.

- RapidRoom base: `a2a4f147` (2.2.0, including the approved #90 upstream sync).
- Validated source commit: `64a2d62c3c146f51d5130750dab828f7ad3e7590`.
- Validated Rust tree: `ab9d4e9f59f652a5a5e8b5b9132ee87fe28286dd`.
- Rawler pin: [b84ca29](https://github.com/yojen7/RapidRAW-DngLab/commit/b84ca29b143c62f920361081b058c77e4d73faa5).
- Pinned executable SHA-256: `d0a5b919b0f55e6d5d067a98b486eef7477971095cd5fb3d4efc268da17b6a20`.
- Baseline executable SHA-256: `933bab69f42ee992dc43ecb8d2c07387c0219c1c664b4a25e8afac06976699d2` (the pinned `samples/engines/int7-rapidroom`, matching `baseline-int7/manifest.json`).

## Historical validation snapshot

The identities and checks below describe the pre-integration snapshot. The final integration combines rawler `a98bd053` (scoped Olympus calibration and DNG/preview fixes) with the Sony crop commits as `53b3dceb`. Fresh final checks are retained in `~/code/rapidraw-project/baseline/results/sora-final-stack/127/` and reported on PR #127 before merge.

## Results

The A7C II maintainer pair is local only. Both ARWs contain `DefaultCropOrigin = 12,8`, `DefaultCropSize = 7008×4672`. The 4:3 ARW additionally contains Sony origin `404,8`, size `6224×4672`; its crop in the developed full frame is `x=392, y=0, width=6224, height=4672`.

| Check                                   | Before               | After                                |
| --------------------------------------- | -------------------- | ------------------------------------ |
| Untouched 4:3 ARW                       | 7008×4672 (full 3:2) | 6224×4672 (camera framing)           |
| Untouched 3:2 ARW                       | 7008×4672            | 7008×4672, identical pixels          |
| 4:3 ARW with `crop: null`               | 7008×4672            | 7008×4672, identical pixels          |
| Existing saved crop                     | 4200×2800            | 4200×2800, identical pixels          |
| Existing sidecar with `crop: null`      | full frame           | full frame, identical pixels         |
| Legacy sidecar edits without a crop key | full frame           | full frame, identical pixels         |
| Explicit camera rectangle               | 6224×4672            | identical to new default camera crop |

A global adjustment override without a crop key also starts with camera framing. The after default export is pixel-identical to an explicit application of the same crop in the before engine. The after reset is pixel-identical to the original full-image export.

All **60/60** CC0 corpus renders are pixel-identical to the approved current-main `baseline-int7` baseline, in the same environment (Intel `8086:64A0`, xe, Mesa 26.2.2, Linux 7.2.5 Omarchy). This includes all nine Sony variants and the existing ProRAW/dither behavior. No tolerances or committed reference were changed by #92; the comparison uses the maintainer-approved post-#90 reference. The corpus name `sony-a7c2-37mp-4x3` refers to padded RAW storage dimensions; its Sony crop tags actually select full 3:2, verified with ExifTool.

## Checks and limits

Changed rawler files pass rustfmt. `cargo test -p rawler --lib` passes; the added synthetic tests cover 4:3, 1:1, 16:9, M/S coordinates, equal/missing/malformed/unsupported/out-of-bounds tags. RapidRoom passes fmt, locked all-target/all-feature strict Clippy, 182 library tests (3 pre-existing ignored), 44 frontend tests, the status-generator check and a locked release build. Its tests cover all eight EXIF orientations and explicit saved crop/null precedence. Native logs and the build identity are recorded in `native-validation.json`; full comparison and per-render hashes are adjacent.

Actual 1:1 and 16:9 camera files were not available; those modes are covered synthetically. Human desktop crop-tool review remains required. Reading metadata seeds only in memory; no sidecar is written by this feature until normal user edit saving.

## Local before/after photographs

**Do not upload or commit the private photographs or derivatives.** On the maintainer machine, the real before/after exports and camera JPEG comparison are at:

`~/code/rapidraw-project/samples/aspect-test-validation-codex-92/private-before-after-camera.png`

The full TIFF exports and isolated saved-sidecar inputs are in that directory. Original RAW+JPEG files remain in `samples/aspect-test/`. The committed pair JSON contains dimensions and hashes only.

For human review: open the 4:3 file, compare its initial frame with the camera JPEG, expand/reset its crop to 7008×4672, reopen a saved custom crop and a saved full-frame reset, and confirm the 3:2 file stays unchanged. Josh approved #127 on 2026-10-04: “I will never use a 4:3 shot, personally, I will always take the full sensor and crop later, so 127 is approved”. The checks above remain useful desktop follow-ups; the approval permits merging after fresh automated gates. Offering the fix upstream is for maintainers after it lands; nothing was posted upstream.
