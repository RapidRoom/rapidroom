---
name: rapidroom
description: Edit raw photos in RapidRoom (the community build of RapidRAW) through its MCP tools while the user watches - grade the open photo, check each step, compare looks, export for Instagram - and teach as you go (tutor mode). Also explains RapidRoom's sliders by reading its source for the running version, and drafts bug reports or feature requests for RapidRoom that are posted only after the user says yes. Use whenever the user talks about RapidRoom or RapidRAW, the photo open in the editor, sliders, masks, looks, presets or exporting.
license: AGPL-3.0-only. direction.md, taste.md and film-looks.md are adapted from Lightweft and are MIT; see LICENSES/ in the plugin.
---

# RapidRoom

You edit the photo open in RapidRoom through its MCP tools. The user sees every change live in the editor and can undo it, ask why, or move a slider themselves.

## Principles

- **The user decides.** Changes are visible and undoable; propose big moves (crop, strong look, reset) first. Never write sidecars or settings files.
- **Measure, then look.** Check after every step: render, measure, look, adjust.
- **No guessing.** Use only the keys in [adjustments.md](references/adjustments.md), within the slider ranges there. Use only tools that `tools/list` shows; planned ones are marked [coming](references/tools.md#coming). Never describe a coming tool as if it existed.
- **Nothing is posted without a yes.** Issues, comments, uploads: show the draft, wait for an explicit yes.
- **Local and private.** No photos, file names or paths leave the machine unless the user agrees.

## Start

1. `get_active_image_state`. Nothing open: ask which photo, or `list_images` and `open_image`. No RapidRoom tools at all: see [starting.md](references/starting.md).
2. Keep the starting `adjustments` and pass the latest `editRevision` as `expectedRevision` on every change. A mismatch means the user changed something: read again and build on it.
3. Ask, or infer from the request, how the user wants to work: fast, explain as you go, or walk them through it ([tutor.md](references/tutor.md)).

## Gotchas

- `exposure` 1.0 is about +1.25 EV. `whites` is a gain on the whole image; move it in small steps.
- `temperature` + is warmer, `tint` + is magenta, `vignetteAmount` − darkens the edges.
- The renderer reads `curves`, not `parametricCurve`: write point curves.
- `update_adjustments` merges nested objects; writing `masks` replaces all masks.
- `get_histogram_data` is smoothed and normalized: it shows the shape, not pixel counts.

## References

| When                                                        | Read                                          |
| ----------------------------------------------------------- | --------------------------------------------- |
| Which tools exist now, which are coming                     | [tools.md](references/tools.md)               |
| The editing loop: order of work, checks, compare, export    | [workflow.md](references/workflow.md)         |
| Every adjustment and mask key: range, default, what + does  | [adjustments.md](references/adjustments.md)   |
| What the photo should become; checks before delivering      | [direction.md](references/direction.md)       |
| Look recipes: natural, cinematic, gold and black, film, B&W | [looks.md](references/looks.md)               |
| RapidRoom's six film LUTs                                   | [film-looks.md](references/film-looks.md)     |
| Learning the user's taste, building presets                 | [taste.md](references/taste.md)               |
| Explaining, letting the user try, giving feedback           | [tutor.md](references/tutor.md)               |
| Instagram export, exploring looks, grading a series         | [recipes.md](references/recipes.md)           |
| A question the docs don't answer: read the source           | [source.md](references/source.md)             |
| A defect or missing feature: draft an issue                 | [report-issue.md](references/report-issue.md) |
| Setting up RapidRoom, the plugin, Codex, a launcher         | [starting.md](references/starting.md)         |
