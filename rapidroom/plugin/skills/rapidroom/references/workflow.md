# Editing workflow

Connect, establish the destination and look, agree the crop, inspect, edit, compare and export. The user watches the editor the whole time, so work in steps they can follow.

## 1. Connect

- Call `get_active_image_state`. If it fails because nothing is open, ask which photo, or find it with `list_images` and open it with `open_image`.
- If the MCP tools aren't there at all, RapidRoom isn't running, isn't an `mcp` build, or is on another port. Say which, and point to [starting.md](starting.md). Don't fall back to editing sidecar files by hand.
- Note the `imagePath` and `editRevision`. Keep a copy of the starting `adjustments` for comparison; `history_list`, `undo` and `redo` share the GUI history when this build exposes them.

## 2. Inspect

- Ask for the destination (feed or story, print or screen) and intended look if they are missing. Use the answer to agree the framing before grading.
- `get_preview` at about 800 px to look. Read a histogram or call `analyze` only when a tonal/clipping question needs numbers.
- Read what's already there. A photo can arrive with an edit (the user's, a preset, a Lightroom import). Build on it unless the user asks for a fresh start; never reset without asking.
- Write a two-line brief before changing anything: what the photo is about, what's in the way, and the direction (the user's words come first). See [direction.md](direction.md).

## Protect existing work before proposing changes

When the user asks for ideas or an alternative edit, call `assess_edits` with the current path/revision before the first mutation. If it recommends a virtual copy (masks, curves, grading or substantial settings), automatically call `create_virtual_copy`, tell the user you preserved their version, then open the **returned** copy path and read its current revision. Edit and export that copy. An untouched photo, or only crop plus a few Basic sliders, can be edited in place. The user's explicit instruction to edit the original takes precedence; don't silently reset it. If the installed build lacks assessment/copy tools, explain the missing protection and ask before overwriting existing work.

For a requested Style/look, use `list_presets` and `apply_preset`, preferably by the returned stable ID. Inspect availability, masks and geometry first. Use the native Amount control and verify `get_editor_context.appliedPreset`. Don't manually translate a bundled XMP recipe or invent a missing preset. Compare before and after, preserve a suitable existing crop and adjust this image rather than apply one grade blindly to a series.

To compare with the user's version, open the AI copy as active and use `open_reference_view` with the base as reference. The user sees the existing side-by-side view. `render_compare` remains for the assistant's own inspection under the installed schema; no wipe feature is promised. “Keep both” is the default. For “keep mine,” open the AI copy, refresh its revision, request deletion, and let the user confirm in the GUI. For “keep AI,” offer keeping both; deleting the original also deletes its virtual copies, so never request that deletion if it would remove the chosen AI copy. A pending confirmation is not a completed deletion.

## 3. Change, one decision at a time

Typical order. Skip what the photo doesn't need.

1. **Geometry**: level the horizon (`rotation`), straighten verticals (`transformVertical`, guided perspective), agree and apply the crop first, so every subsequent preview shows the final frame. Crop coordinates are original-image pixels; use `get_editor_context` dimensions and the documented geometry units, or let the user place the frame in the Crop panel.
2. **White balance**: `temperature`, `tint`. For mood, not for a grey card.
3. **Exposure and tone**: `exposure` first, then `highlights`, `shadows`, `whites`, `blacks`, then `contrast`. Curves after the sliders. Before committing a heavy curve, inspect shadow and midtone detail in the subject (glass, liquid, leaves, fabric): a dramatic black point should not erase detail the user wanted to keep.
4. **Colour**: `vibrance` before `saturation`; the colour mixer (`hsl`) to calm or deepen single colours; `colorGrading` for split tones.
5. **Local adjustments**: masks for subject and surroundings, gradients for sky, radial burns for edges. Use `upsert_mask` for one named mask id at a time, with the current revision, then preview it before adding the next. Use `remove_mask` for that id. Coordinates are original-image pixels before crop, not fractional preview positions. See [adjustments.md](adjustments.md#masks) for the container/components format and every unit; replacing the whole `masks` array would replace siblings.
6. **Detail**: noise reduction before sharpening; `clarity`, `structure`, `dehaze` with care.
7. **Effects**: vignette, grain, glow, a LUT. Last, and subtle unless the look calls for more.

Each change is one `update_adjustments` call with the related keys together (for example the four tone sliders of one decision), passing `expectedRevision`. Tell the user in one line what you changed and why: "Exposure +0.4 and Shadows +25: the face was underexposed."

Read `rapidroom://schema/adjustments` when the server offers it, and use only keys in it or [adjustments.md](adjustments.md). Stay inside the generated UI slider ranges, so the user can see your value on the slider and adjust it.

## 4. Check after every step

Render and look after each meaningful decision. Measure when it resolves a clipping or colour question; avoid an automatic `analyze` call for every slider move.

- **Render**: `get_preview` of the result at about 800 px. For a choice between values, preview **proposed** edits by passing `adjustments` to `get_preview`; nothing is applied until you call `update_adjustments`.
- **Measure** (below) when needed: exact clipping, or the colour of a selected neutral patch.
- **Look**: at full frame, and at a small size (about 256 px) to see what the eye lands on first.
- **Adjust**: if the step overshot, take it back now, with a smaller value.

### Measure

- `analyze` gives exact rendered sRGB clipping counts and percentages. Use it when highlight/shadow loss matters, and request `histogram: true` only if the actual bins are needed. This measures rendered pixels, not sensor RAW clipping.
- `sample_region` measures a fractional patch; its optional neutral RGB gains are advisory and are not temperature/tint slider values. Use only a patch known to be neutral.
- `get_histogram_data` shows smoothed, normalized tonal shape. Do not infer clipping percentages from it.
- Check visually that intended shadow and midtone detail survives. Numbers do not choose the look.

## 5. Compare

- Before/after: use `get_preview` with `side_by_side: true` for neutral original/current edit, or preview the starting adjustments you saved separately as a proposed edit. Use fractional `region` for a bounded detail crop. These reads do not apply edits.
- Alternatives: one `render_compare` round per decision, usually two or three named temporary `changes` variants ("Warm", "Clean", "Muted") at about 800 px. This shows one labelled choice without applying it. Explain the difference, then apply only the chosen patch. Recompare if the user changes the brief or the result needs a correction, rather than repeating an accepted choice. Tools in older builds may require matching crop geometry; follow their input schema and errors.
- At the end, make one small native `render_region` check for noise, sharpness and mask edges. Use its discovered coordinate schema: existing builds use native integer pixels; builds offering fractions/`regionUnits` also accept normalized selections. Native pixel coordinates are after rendered geometry, unlike original-image mask coordinates. Do not estimate them from an 800 px preview without mapping dimensions.
- When the user is moving sliders too, use `history_list` actor and changed keys to see what they changed. Fetch the latest state when its revision or changed values are needed for a mutation; preserve their work and keep strict `expectedRevision` guards. Assistant changes keep their normal visible notifications.

For a scored, strength-aware correction with hard round caps and nondecreasing-score retention, see [bounded render review](judge.md). This optional offline helper returns proposals only; render and inspect every candidate through the existing tools, and keep the normal user/revision guards.

## 6. Save

RapidRoom autosaves every change to the photo's sidecar. Nothing extra to do. If the user wants to keep the edit as a starting point for other photos, that's a preset: presets over MCP are coming (#120); until then the user saves one from the Presets panel, and [taste.md](taste.md) explains what belongs in it.

## 7. Export

`export_images` with explicit settings. Ask where to put files, or use a subfolder next to the originals (`destinationType: "originalFolder"`, `subfolder`). Recipes are in [recipes.md](recipes.md). After exporting, say where the files are.

## Safety rules

- The user decides. Propose bigger moves (a crop, a strong look, a reset) before making them.
- Every change goes through the editor and is undoable. Never write sidecars, catalogs or settings files directly.
- Don't touch photos the user didn't mention, and don't export over existing files.
- If a call fails with a revision mismatch, the user changed something. Read the state again and build on their change.
