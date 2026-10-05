# Export recipes

Issue #121: export presets that work the same from the GUI, the CLI and (with the `mcp` feature) an AI agent, output sharpening, and Instagram sizes. This note covers the implementation in PR #126, how to use it, and the two donor forks that were evaluated. Rendering changes await maintainer approval.

## Implementation

### Export presets everywhere

An export preset is the saved state of the export panel (format, quality, resize, pad, border, watermark, file naming, metadata, sharpening). They live in `settings.json` as `exportPresets`, exactly as before.

- **Rust understands presets now.** `src-tauri/src/export_recipes.rs` turns a preset into the `ExportSettings` the panel would send (`preset_to_export_settings`), finds one by exact id or case-insensitive name, user presets first (`find_export_preset`), and maps its format to an output extension.
- **CLI:** `rapidroom export <source> --output <dir> --preset "<name or id>"`. The preset supplies format, quality, TIFF depth, metadata/GPS, resize, pad, border, watermark, file name template and sharpening. Explicit `--format`, `--quality`, `--tiff-bit-depth`, `--keep-metadata` and `--sharpen` win over the preset. The CLI keeps its own destination rules: it writes to `--output`, preserves folder structure for folder sources, and keeps capture timestamps. An unknown name fails with exit code 1 and lists the available presets.
- **MCP:** the MCP server is not on `main` yet (PR #102, issue #5). Once it is, `export_images` takes a `preset` argument and resolves it with the same two functions; see [MCP follow-up](#mcp-follow-up). Until then an agent can run the CLI.
- **GUI:** the preset dropdown also lists the built-in recipes below. They are read-only (no overwrite or delete), like the two default presets. Apply one, change it, and save it as your own preset.

### Instagram recipes

Three built-in, read-only presets (`builtin_export_recipes`):

| Recipe                                | Id                           | Output    |
| ------------------------------------- | ---------------------------- | --------- |
| Instagram Portrait 4:5 (1080×1350)    | `recipe-instagram-portrait`  | 1080×1350 |
| Instagram Square 1:1 (1080×1080)      | `recipe-instagram-square`    | 1080×1080 |
| Instagram Landscape 1.91:1 (1080×566) | `recipe-instagram-landscape` | 1080×566  |

All three: JPEG quality 90, sRGB (every RapidRoom export is sRGB), metadata kept but GPS stripped, light screen sharpening, white padding to the post's aspect ratio (the photo is never cropped; crop in the editor if you want it to fill the frame), resized to 1080 px wide even if the source is smaller. Turn on **Add Border** after applying one for a border inside the padding. File names get `_ig_4x5`, `_ig_1x1` or `_ig_191x1`.

To make the sizes exact, a pad-plus-resize export now derives the resized edge from the pad ratio instead of from the rounded padded canvas. Before, a small or odd-sized source could come out 1 px off (1080×1351). This only changes exports that use both **Pad to Aspect Ratio** and **Resize**, by at most a pixel on the derived edge.

Posting stays manual; this change adds no account or browser automation.

### Output sharpening

A new **Output Sharpening** switch in the export panel's Image Sizing section, with **For** (Screen, Print) and **Amount** (Low, Standard, High). CLI: `--sharpen screen`, `--sharpen print:high`, `--sharpen none` (turns a preset's sharpening off).

- **Off unless chosen.** `ExportSettings.outputSharpening` defaults to `null`. Existing presets, the last-used settings and the CLI without `--sharpen` or a preset that sets it export exactly as before.
- It runs after the resize and before the border, padding and watermark, so only photo pixels are sharpened.
- Unsharp mask using output-luma detail (Rec. 709 weights applied to sRGB-coded channels): every colour channel receives the same output-luma delta before clipping to the output range. This reduces independent-channel sharpening artifacts; clipping can still alter colour near the range limits. Alpha is untouched, and 16-bit and float outputs stay 16-bit and float.
- **Radius follows the output size.** Screen: σ = 0.5 + long edge / 12000, clamped to 0.5–1.0 (0.6 at 1080 px, 1.0 at 6000 px). Print: σ = long edge / 4000, clamped to 0.8–2.0 (1.5 at 6000 px). Amount: Low 0.5, Standard 0.8, High 1.2, times 1.5 for print. A small threshold (1/255 for screen, 2/255 for print) leaves smaller luminance differences unchanged. Noise above that threshold can still be sharpened.
- Tests: `output_sharpening::tests` (edges gain contrast, flat areas unchanged, colour and alpha kept, 16-bit preserved, radius and strength order) and `export_recipes::tests` (recipes carry it, default presets don't).

## MCP follow-up

When PR #102 is merged, in `src-tauri/src/mcp/tools.rs`:

1. `export_images` accepts `"preset": string` (id or name). If set, build the settings with `find_export_preset(&load_settings(..).export_presets, name)` and `preset_to_export_settings`, use `preset_output_format` when `outputFormat` is omitted, and reject `exportSettings` alongside `preset` (or apply it on top, field by field).
2. A read-only `list_export_presets` tool returns id, name, format and output size summary for user presets and the recipes.
3. Accept `outputSharpening` in `exportSettings` (the schema and `parse_export_settings` list the allowed fields).

That's small and stays behind the `mcp` feature, so the default build is unaffected.

## Donor evaluation

### puneetrane1811: Creative Export Studio

Source: [puneetrane1811/rapidraw-creative-export-borders-and-grids](https://github.com/puneetrane1811/rapidraw-creative-export-borders-and-grids), `creative-export-borders-and-grids-v2.patch` (12,070 lines, snapshot `1d393cb77ab937553a032ee51a79a676f7877cec`; the repository contains multiple commits).

- **What it has:** 30+ framing presets (Polaroid, film rebates with sprocket holes, gallery mats, cinematic letterboxes), inset keylines, EXIF badges and "social pills", text watermarks, N×M collages from several photos, N×M tile slicing (Instagram carousels and 3×3 profile grids), live proofing on the editor canvas, and per-photo framing stored in `localStorage`.
- **Size and shape:** about 10,400 added lines. The largest pieces are a 3,000-line `default_font.rs` (a zlib-compressed font embedded as a byte array, with no name or licence given), a 1,409-line export store, a 1,343-line Creative Export tab, an 859-line canvas overlay, and about 1,000 lines in `export_processing.rs`. It replaces the export panel with two tabs and rewrites the watermark function.
- **Licence:** the README says AGPL-3.0 (and as a patch to RapidRAW it has to be), but the repository has no LICENSE file. The embedded font's origin and licence are unknown, so `default_font.rs` can't be taken without finding out what it is.
- **Fit:** it doesn't apply to RapidRoom: `git apply --check` fails in 13 files on the local integration branch, including `app_settings.rs`, `export_processing.rs`, `lib.rs`, `App.tsx`, `Editor.tsx`, `EditorToolbar.tsx`, `ExportPanel.tsx` and `useExportSettings.ts`. RapidRoom already has its own border and pad export (from upstream), which overlaps with the mats. Framing state in `localStorage` would not reach the CLI or MCP.
- **Recommendation:** don't take the patch. The smallest useful slice is **tile slicing for carousels and profile grids**: `slice_into_tiles` is about 35 lines (crop the final image into equal tiles, optionally border each tile). Re-implemented on RapidRoom's export pipeline, it would add a `tiles: {columns, rows}` export setting and name the outputs `_1`…`_n`, so it works from the GUI, CLI and MCP alike. Credit puneetrane1811 for the idea and keep their author on any code taken. That's a follow-up issue, not part of this change. Collages and EXIF badges are larger and need the font question answered first.

### weholt: export workflows

Source: [weholt/RapidRAW@2f126b4](https://github.com/weholt/RapidRAW/commit/2f126b48ec296f3035a0b2f8c55d9938d6fd2ee4) by Thomas Weholt, "run trusted local export workflows around the export pipeline" (14,292 added lines in 61 files).

- **What it has:** user scripts (`.py` or `.js`, in `~/.rapidraw/workflows` or bundled) that run after each exported image (`postImage`) or once after the batch (`postBatch`). The host sends one JSON request on stdin (protocol version 1, bounded to 1 MiB) and reads one JSON response from stdout (bounded to 256 KiB), with timeouts, cancellation (Windows job objects), `onError: warn | fail`, a per-run scratch directory, and a one-time trust prompt in the GUI. Headless: `--workflow <id>` (repeatable) and `--list-workflows`.
- **Size and shape:** `export_workflows.rs` is 4,706 lines; tests are about 4,500 more (integration, runner, security, CLI). It changes `export_processing.rs` (382 lines), `launch_request.rs`, `app_settings.rs`, the export panel, all 13 locales, the Flatpak manifest (bundled example workflows) and the Tauri capabilities (denies shell execute and spawn from the frontend, which is good). It also carries the fork's own agent tooling (`.pebbles/`, `.agents/`, its `AGENTS.md`), which a harvest would drop.
- **Security and privacy:** the scripts are unsandboxed local code with the user's full permissions and can use the network. The design is careful (nothing runs unless selected, discovery never executes code, consent prompt), but it's a new way for RapidRoom to start processes, and it needs a maintainer decision. It would need the `network-privacy` review because workflows can make network requests.
- **Fit:** a cherry-pick conflicts in 21 files, including `export_processing.rs`, `launch_request.rs`, `lib.rs`, `ExportPanel.tsx` and every locale. Licence: AGPL-3.0 (a RapidRAW fork).
- **Recommendation:** not now. For RapidRoom's direction (an assistant driving the editor over MCP), the agent can already run its own post-processing after `export_images` or the CLI returns, so a second scripting layer adds risk for little gain. If the maintainers want it later, the smallest useful slice is **headless only**: `--workflow` with the `postBatch` phase and the JSON protocol, no GUI, no consent store, no bundled examples, behind a cargo feature like `mcp`. Keep weholt's authorship and the protocol document.
