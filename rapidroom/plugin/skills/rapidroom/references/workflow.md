# Editing workflow

Connect, inspect, change, render, measure, compare, save, export. The user watches the editor the whole time, so work in steps they can follow.

## 1. Connect

- Call `get_active_image_state`. If it fails because nothing is open, ask which photo, or find it with `list_images` and open it with `open_image`.
- If the MCP tools aren't there at all, RapidRoom isn't running, isn't an `mcp` build, or is on another port. Say which, and point to [starting.md](starting.md). Don't fall back to editing sidecar files by hand.
- Note the `imagePath` and `editRevision`. Keep a copy of the starting `adjustments` for comparison; `history_list`, `undo` and `redo` share the GUI history when this build exposes them.

## 2. Inspect

- `get_preview` at 1280 px to look; `get_histogram_data` for the tonal shape.
- Read what's already there. A photo can arrive with an edit (the user's, a preset, a Lightroom import). Build on it unless the user asks for a fresh start; never reset without asking.
- Write a two-line brief before changing anything: what the photo is about, what's in the way, and the direction (the user's words come first). See [direction.md](direction.md).

## 3. Change, one decision at a time

Typical order. Skip what the photo doesn't need.

1. **Geometry**: level the horizon (`rotation`), straighten verticals (`transformVertical`, guided perspective), decide the crop. Crop early, because it changes what the rest of the edit is judged on.
2. **White balance**: `temperature`, `tint`. For mood, not for a grey card.
3. **Exposure and tone**: `exposure` first, then `highlights`, `shadows`, `whites`, `blacks`, then `contrast`. Curves after the sliders.
4. **Colour**: `vibrance` before `saturation`; the colour mixer (`hsl`) to calm or deepen single colours; `colorGrading` for split tones.
5. **Local adjustments**: masks for subject and surroundings, gradients for sky, radial burns for edges. See [adjustments.md](adjustments.md#masks) for what works over MCP today.
6. **Detail**: noise reduction before sharpening; `clarity`, `structure`, `dehaze` with care.
7. **Effects**: vignette, grain, glow, a LUT. Last, and subtle unless the look calls for more.

Each change is one `update_adjustments` call with the related keys together (for example the four tone sliders of one decision), passing `expectedRevision`. Tell the user in one line what you changed and why: "Exposure +0.4 and Shadows +25: the face was underexposed."

Read `rapidroom://schema/adjustments` when the server offers it, and use only keys in it or [adjustments.md](adjustments.md). Stay inside the generated UI slider ranges, so the user can see your value on the slider and adjust it.

## 4. Check after every step

Render, measure, look, adjust. Don't stack five changes and hope.

- **Render**: `get_preview` of the result. For a choice between values, preview **proposed** edits by passing `adjustments` to `get_preview`; nothing is applied until you call `update_adjustments`.
- **Measure** (below): clipping, and the colour of what should be neutral or skin.
- **Look**: at full frame, and at a small size (about 256 px) to see what the eye lands on first.
- **Adjust**: if the step overshot, take it back now, with a smaller value.

### Measure

What you can measure today, until the measuring tools in #117 exist:

- **Histogram shape** from `get_histogram_data`: 256 bins per channel (0 = black, 255 = white). The values are smoothed and normalized for display, so they show the shape, not pixel counts. A tall pile at bin 255 (or 0) in a channel means that channel is clipping. Don't quote percentages from it.
- **The clipping overlay**: ask the user to switch on clipping display in the editor and tell you what turns red or blue. They see it live.
- **Pixel numbers**, when you really need them (skin tone, a neutral grey, exact clipping): with the user's OK, `export_images` a small JPEG (`resize` long edge 1024) to a temporary folder outside their photo folders, and measure it with whatever image tools you have (for example Python with Pillow). Delete it afterwards. Say that it's an export of the current edit, not the raw data.
- **Your eyes on a preview**: judgement comes on top of measurement, not instead of it.

## 5. Compare

- Before/after: use `get_preview` with `side_by_side: true` for neutral original/current edit, or preview the starting adjustments you saved separately as a proposed edit. Use fractional `region` for a bounded detail crop. These reads do not apply edits.
- Alternatives: preview two or three proposed edits, describe each by its idea ("warmer and darker", "clean and neutral"), and let the user choose. Apply only the chosen one. With virtual copies (#120) each alternative will be a real copy; until then, see [recipes.md](recipes.md#explore-looks).

## 6. Save

RapidRoom autosaves every change to the photo's sidecar. Nothing extra to do. If the user wants to keep the edit as a starting point for other photos, that's a preset: presets over MCP are coming (#120); until then the user saves one from the Presets panel, and [taste.md](taste.md) explains what belongs in it.

## 7. Export

`export_images` with explicit settings. Ask where to put files, or use a subfolder next to the originals (`destinationType: "originalFolder"`, `subfolder`). Recipes are in [recipes.md](recipes.md). After exporting, say where the files are.

## Safety rules

- The user decides. Propose bigger moves (a crop, a strong look, a reset) before making them.
- Every change goes through the editor and is undoable. Never write sidecars, catalogs or settings files directly.
- Don't touch photos the user didn't mention, and don't export over existing files.
- If a call fails with a revision mismatch, the user changed something. Read the state again and build on their change.
