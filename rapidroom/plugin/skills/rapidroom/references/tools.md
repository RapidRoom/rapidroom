# RapidRoom's MCP tools

RapidRoom's MCP server runs inside the app and drives the editor the user is looking at. It exists only in builds made with the `mcp` cargo feature (`npm run start:mcp`, or `npm run tauri build -- --features mcp`), and it listens on `http://127.0.0.1:7790/mcp` (the next free port up to 7800 if 7790 is taken; `RAPIDRAW_MCP_PORT` pins it, and the URL in use is in `mcp-endpoint.json` in the app's config directory). The plugin's `.mcp.json` connects to it; Codex users run `codex mcp add rapidroom --url http://127.0.0.1:7790/mcp`.

**Trust `tools/list`, not this page.** If a tool listed under "Coming" shows up, use it. If a tool listed under "Now" is missing, the build is older or different: say so, and don't call it.

## Now

From the in-app MCP server (issue #5). Every editing tool works on the image open in the editor, takes its `imagePath`, and goes through the editor, so the user sees each change, Ctrl+Z undoes it, and autosave writes it like a click.

| Tool                                | Use it for                                                                                                                                                                                                         |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `list_images`                       | `path`, `recursive`, `offset`, `limit` (≤ 500). Lists supported images in a folder.                                                                                                                                |
| `open_image` (alias `select_image`) | `imagePath`. Opens a photo in the editor, also from the library or the start screen.                                                                                                                               |
| `get_active_image_state`            | Which image is open, its adjustments and an `editRevision`. Start here.                                                                                                                                            |
| `get_image_state`                   | The same for a given `imagePath` (must be the open image).                                                                                                                                                         |
| `update_adjustments`                | `imagePath`, `changes`, `expectedRevision`. Merges `changes` into the edit; nested objects merge too. Your normal editing tool.                                                                                    |
| `set_adjustments`                   | `imagePath`, `adjustments`, `expectedRevision`. Replaces the whole edit. Use it to restore a state you saved, not for small changes.                                                                               |
| `reset_adjustments`                 | Resets everything as one undoable step. Ask first.                                                                                                                                                                 |
| `apply_auto_adjustments`            | RapidRoom's auto adjustments, merged into the edit. A starting point, not a finished edit.                                                                                                                         |
| `get_preview`                       | A JPEG of the current edit, or of a **proposed** edit passed as `adjustments` (not applied). `maxDimension` 128–4096, default 1280.                                                                                |
| `get_histogram_data`                | The editor's 256-bin red, green, blue and luma histogram of the current edit.                                                                                                                                      |
| `calculate_guided_perspective`      | Perspective transform and crop for guide lines (normalized 0–1 points). Doesn't change the edit; apply with `update_adjustments` and `guidedPerspective`.                                                          |
| `export_images`                     | `imagePaths`, `outputDirectory`, `outputFormat` (`jpg`, `png`, `tiff`, `webp`, `jxl`, `avif`), `exportSettings` (quality, resize, metadata, GPS, filename template, watermark). Waits until the files are written. |

Behaviour worth knowing:

- Mutations return a new `editRevision`. Pass the last one you saw as `expectedRevision`; if the user moved a slider in between, the call fails instead of overwriting their change. Read the state again, then decide.
- An edit that changes nothing returns at once. If the preview takes longer than 40 s, the edit is still applied and the result says `renderPending: true`.
- If the open edit doesn't fit the schema (for example an out-of-range value from an imported sidecar), the state still comes back, with the problem in `validationError`. Don't "fix" it silently; tell the user.
- The server announces itself as `RapidRAW` with a version number that is not RapidRoom's version yet. Don't use it to find the source; see [source.md](source.md).
- There is no authentication yet: any program on the machine can connect while the app runs. Don't suggest the `mcp` build on shared machines.

## Coming

These are planned and do not exist yet. Don't call them, and don't tell the user they exist. When a workflow needs one, say what's missing and use the workaround.

| Planned                                                                                                        | Issue | Until then                                                                                                                                                                           |
| -------------------------------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `undo`, `redo`, `history_list`, labelled history ("AI: Exposure +0.4") and an "AI changed …" toast             | #115  | Save `get_active_image_state` before a risky change and restore it with `set_adjustments`. The user can press Ctrl+Z.                                                                |
| `get_editor_context` (open photo, virtual copy, EXIF summary, selected panel and mask, app version)            | #115  | `get_active_image_state` for the photo; ask the user for the version (see [source.md](source.md)).                                                                                   |
| Adjustment schema as an MCP resource, generated from the code                                                  | #115  | [adjustments.md](adjustments.md), generated from the same code.                                                                                                                      |
| `get_preview` options `region`, `original`, `side_by_side`                                                     | #115  | Full-frame previews at a larger `maxDimension`; for "original", preview a proposed edit with the defaults.                                                                           |
| Per-session token, stdio proxy, `rapidroom-agent` CLI                                                          | #116  | Loopback HTTP without a token (see above).                                                                                                                                           |
| Clipping statistics, region colour, native-resolution crops, labelled contact sheet                            | #117  | Clipping from `get_histogram_data`; look at previews; see [workflow.md](workflow.md#measure).                                                                                        |
| Mask tools including AI masks (subject, sky, depth), virtual copies, presets, library actions (rating, labels) | #120  | Linear and radial masks by writing the `masks` array (untested; see [adjustments.md](adjustments.md#masks)). No AI masks or virtual copies over MCP: compare proposed edits instead. |
| Export presets over MCP and the CLI, output sharpening                                                         | #121  | Pass `exportSettings` yourself (see [recipes.md](recipes.md)).                                                                                                                       |
| Print soft-proofing, ICC export                                                                                | #122  | Not available.                                                                                                                                                                       |
| A terminal built into RapidRoom                                                                                | #119  | An external terminal (see [starting.md](starting.md)).                                                                                                                               |
