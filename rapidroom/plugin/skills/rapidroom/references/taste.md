<!--
Adapted from photo-style-builder (SKILL.md and references/taste-learning.md) in Lightweft by
sheldonxxxx and the Lightweft contributors,
https://github.com/sheldonxxxx/Lightweft/tree/d332fbd629faf7e47144ba173d7f2032be62199d/skills/photo-style-builder
Copyright (c) 2026 Lightweft contributors. MIT licence: this file is under the MIT licence,
not the AGPL; see ../../../LICENSES/Lightweft-MIT.txt. Changes for RapidRoom: shortened, mapped to
RapidRoom presets, Lightweft's review app and collection workflow left out.
-->

# Learning the user's taste, and building presets

Learning should change the next edit. A note about preferences that doesn't change what you render hasn't done its job.

## Ground preferences in evidence

- Before editing, read the brief and what the user said about earlier edits in this conversation, and any profile they keep (below).
- Record what the user valued or rejected as a **visible relationship**: "pale subject against a deeper background", "small colour accents stay readable", "darkness that keeps the time of day". Not "liked temperature +12".
- Don't infer a universal palette from one liked photo, and don't treat every feature of a preset they liked as endorsed.
- Keep apart: a suggestion you made, a version the user accepted, and a preference they stated. Your recommendation is never their acceptance.

## Turn feedback into changed decisions

The user's current instruction wins. Then accepted examples and repeated corrections that fit this photo, then broader preferences, then general judgement. "Make this one brighter" doesn't cancel a liking for dark twilight scenes.

When an edit missed, find out why:

- **The preference was unknown**: note it and test it.
- **It was known but the edit contradicted it**: change the decision, not just the wording of the note.
- **The decision was right but the render failed**: fix the execution and look again.

A correction that repeats is a check for every later photo it applies to. Don't make the same global change more cautiously when the user asked for a different relationship between regions.

## Apply before delivering

Before rendering, decide in a few lines: what should draw attention; which preference applies and which accepted example shows it; how subject, foreground and sky should differ in light and colour; what rejected look must be absent. Then translate that into RapidRoom adjustments. Different photos can need opposite moves to reach the same relationship.

After rendering, judge twice: **taste** (at viewing size, is the requested drama, separation or quiet visible?) and **technique** (at full size, no halos, noise or broken edges). If a known miss is still there, revise before showing it.

## Two kinds of saved style

- **Edit profile**: words, not numbers. How the user likes light, colour, contrast, texture and atmosphere to relate, with the accepted examples behind each point and where it doesn't apply. Keep it in the user's own files (for example `rapidroom-style.md` in their photo folder), only when they ask, never in this skill.
- **Preset**: real adjustment values from an edit the user accepted, checked against the render. Only the keys that generalise: tone, colour, grading, effects. Leave out crops, masks, geometry, lens corrections and anything that depends on this scene. Don't invent values for a preset; take them from `get_active_image_state` of the accepted edit. Presets over MCP are [coming](tools.md#coming) (#120). Until then, give the user the values and ask them to save a preset from the Presets panel, or apply the values to the next photo with `update_adjustments`.

Save only on the user's request or acceptance. When a preset is meant for a whole shoot, try it on two or three different photos from it first, and say which situations it hasn't been tried on.
