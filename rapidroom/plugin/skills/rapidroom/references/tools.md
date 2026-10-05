# RapidRoom's MCP tools

RapidRoom's MCP server runs inside the app and drives the editor the user is looking at. It exists only in builds made with the `mcp` cargo feature. The separate `rapidroom-mcp-stdio` adapter discovers a private per-user endpoint and authenticates to the loopback listener. The plugin's `.mcp.json` uses that adapter; Codex users configure the same command, with `env_vars = ["XDG_CONFIG_HOME"]` for a custom config directory. See [starting.md](starting.md) for setup; no hardcoded URL, bearer token or endpoint environment variable is needed.

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

## History, context and preview options

- `history_list(imagePath)` reads the same undo stack as the GUI. Entries include the label, actor, changed keys, timestamp, active/undone flags and current index. Assistant changes carry an `AI:` label and a brief toast. User entries without recorded metadata have a null timestamp.
- `undo(imagePath, expectedRevision)` and `redo(imagePath, expectedRevision)` move one step in that shared stack. They return the resulting state/revision, and do nothing at the stack boundary. Read the revision before moving history: GUI edits can also be in this stack.
- `get_editor_context()` reads the active path/virtual copy, dimensions, EXIF summary, crop, mask summaries and selected mask/panel, plus Card mode. With no ready photo, image fields are null or empty. It does not change history. Mask summaries are limited to the first 256; `maskCount` reports the full count.
- The MCP resource `rapidroom://schema/adjustments` supplies generated defaults, adjustment keys, UI slider ranges and sign conventions. Use it when available, together with this build's tool input schema. `defaultType` describes the default value; UI ranges are guidance, not a replacement for tool validation. The prose [adjustments.md](adjustments.md) explains the controls.
- `get_preview` accepts `original: true` for neutral source defaults, `side_by_side: true` for neutral original on the left and edited on the right, and `region: {x, y, width, height}` using 0–1 fractions inside the rendered image. Regions are crops of bounded previews, not native-resolution raw crops. Comparison cells preserve aspect ratio with dark letterboxing. The complete image's long edge respects `maxDimension` (128–4096), and JPEG payloads are capped at 8 MiB.
- Previews and resource/context/history reads do not apply edits or add history. `original` cannot be combined with supplied adjustments or `side_by_side`. Neutral original uses the current rendering/tone-mapper path; it is not the camera's embedded JPEG. To compare with your saved starting edit instead, preview that snapshot separately as proposed adjustments.

Behaviour worth knowing:

- Mutations return a new `editRevision`. Pass the last one you saw as `expectedRevision`; if the user moved a slider in between, the call fails instead of overwriting their change. Read the state again, then decide.
- An edit that changes nothing returns at once. If the preview takes longer than 40 s, the edit is still applied and the result says `renderPending: true`.
- If the open edit doesn't fit the schema (for example an out-of-range value from an imported sidecar), the state still comes back, with the problem in `validationError`. Don't "fix" it silently; tell the user.
- The server announces itself as `RapidRAW` with a version number that is not RapidRoom's version yet. Don't use it to find the source; see [source.md](source.md).
- Each app start has a fresh bearer token in its private per-user endpoint file. The stdio adapter handles discovery and authentication; restart the MCP connection after restarting the app. Processes running as the same OS user remain trusted.

## Coming

These are planned and do not exist yet. The authenticated adapter, terminal panel and history/context/schema/preview options described above are available in builds with their respective features. Don't call them, and don't tell the user they exist. When a workflow needs one, say what's missing and use the workaround.

| Planned                                                                                                        | Issue | Until then                                                                                                                                                                           |
| -------------------------------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Clipping statistics, region colour, native-resolution crops, labelled contact sheet                            | #117  | Clipping from `get_histogram_data`; look at previews; see [workflow.md](workflow.md#measure).                                                                                        |
| Mask tools including AI masks (subject, sky, depth), virtual copies, presets, library actions (rating, labels) | #120  | Linear and radial masks by writing the `masks` array (untested; see [adjustments.md](adjustments.md#masks)). No AI masks or virtual copies over MCP: compare proposed edits instead. |
| Export presets over MCP and the CLI, output sharpening                                                         | #121  | Pass `exportSettings` yourself (see [recipes.md](recipes.md)).                                                                                                                       |
| Thin `rapidroom-agent` terminal CLI                                                                            | #116  | Use the registered MCP tools through Claude Code or Codex.                                                                                                                           |
| Print soft-proofing, ICC export                                                                                | #122  | Not available.                                                                                                                                                                       |
