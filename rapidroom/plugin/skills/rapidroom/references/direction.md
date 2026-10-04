<!--
Adapted from photo-edit-master in Lightweft by sheldonxxxx and the Lightweft contributors,
https://github.com/sheldonxxxx/Lightweft/tree/d332fbd629faf7e47144ba173d7f2032be62199d/skills/photo-edit-master
Copyright (c) 2026 Lightweft contributors. MIT licence: this file is under the MIT licence,
not the AGPL; see ../../../LICENSES/Lightweft-MIT.txt. Changes for RapidRoom: shortened, tool
and slider names mapped to RapidRoom, Lightweft's review app and genre files left out.
-->

# Artistic direction

Make a photograph a viewer stops for. A clean correction that looks like the camera file with a little more contrast is an unfinished edit, not a safe one. The user's instructions and the edits they've accepted outrank everything here.

## Print it, don't process it

1. **Visualise the result first.** Before any adjustment, write two sentences: what the viewer sees first, what they feel, where the light falls. Name the tonal key (low-key, high-key or full-range) and the colour idea.
2. **Shape the light by region.** Global sliders set the base; local work makes the picture: subject lifted, surroundings subdued, edges burned, light paths emphasised.
3. **Judge the result as a picture** on a wall or a phone screen, not as a set of plausible slider values.

Be decisive. If you can't tell the edit from the raw at thumbnail size, it isn't finished. Restraint is a choice with a reason ("this fog is the subject"), not a default. But when the user wants a light touch, that is the reason.

## Lessons from the masters, as editing moves

| Lesson                                     | Master                 | In RapidRoom                                                                                                                          |
| ------------------------------------------ | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Visualise, then place tones deliberately   | Ansel Adams            | Decide the deepest black, the brightest white and where the subject sits. Set `blacks` and `whites` on purpose.                       |
| Light the subject, let the world fall away | Rembrandt, chiaroscuro | Subject brightest, most contrasty and most detailed; surroundings fall off towards the edges (a mask and its inverse).                |
| Burn the edges                             | Darkroom printing      | `vignetteAmount` negative, or radial/linear masks with negative `exposure`. Subtle enough that the vignette itself isn't seen.        |
| Warm against cool                          | Galen Rowell           | Warm light on the subject against a cooler environment: `temperature` per mask, or `colorGrading` with warm highlights, cool shadows. |
| Simplicity and negative space              | Michael Kenna          | Crop or tone down anything that competes. Empty space works when its tone is even and quiet.                                          |
| Geometry carries the moment                | Henri Cartier-Bresson  | When the frame missed, use the crop to restore it: gaze room, strong diagonals, a clean border.                                       |

These are documented working practices, not looks to imitate. Use one when this photograph gives a reason.

## Steps that average edits skip

1. **Read the photograph** (brief, written): subject and story; light (direction, quality, colour, the time of day it shows; a blue-hour scene stays blue hour); the strongest thing in the frame; what stops it being great.
2. **Geometry first.** Make a crop decision on every photo; keeping the full frame is a decision too. Level horizons, place the subject, trim cluttered edges. A crop that keeps less than about 40 % of the frame needs a reason.
3. **White balance serves the mood.** Warm the golden hour, keep shade and snow blue. Correct a cast only when it fights the story; teal water and green skin are usually casts. **Don't brighten dim scenes by default**: place the subject's brightness and let the rest follow. Set real black and white points; pull `highlights` to keep texture in sky and snow. Keep global `contrast` moderate.
4. **Shape the light** (the main work, with masks):

   | Region                      | Starting move                                                         | Purpose                      |
   | --------------------------- | --------------------------------------------------------------------- | ---------------------------- |
   | Subject                     | `exposure` +0.15 to +0.6; `shadows` +5 to +20; `clarity` +5 to +20    | Presence and detail          |
   | Environment (inverted mask) | `exposure` −0.3 to −1.0; `saturation` −10 to −30; `clarity` −5 to −20 | Quiet, depth, separation     |
   | Frame edges                 | `vignetteAmount` −10 to −35, or radial burns −0.3 to −0.8 EV          | Keeps the eye inside         |
   | Sky or bright foreground    | Linear mask, `exposure` −0.3 to −1.0                                  | Balance and depth            |
   | Light path (rim, sunlit)    | Small mask, `exposure` +0.1 to +0.4, warmer                           | Shows where light comes from |

   Starting points, not answers: judge the render, not the number. RapidRoom's `exposure` unit is about 1.25 EV, so these values are slightly stronger than the same EV numbers; that's fine as a start. Build subject and environment from the same selection so they share one edge, then check the edge for halos. AI subject masks over MCP are [coming](tools.md#coming); until then use radial and linear masks, or ask the user to make the subject mask in the GUI.

5. **Colour: one idea per image.** Warm–cool contrast, one saturated accent, a restrained palette, or monochrome. Use `hsl` to calm competing colours and deepen the ones that carry the mood. `vibrance` before `saturation`; global `saturation` past about +15 needs a reason. Offer monochrome as an alternative when colour adds nothing.
6. **Detail and finish.** Denoise before sharpening; for high-ISO files the AI denoise in the GUI beats the sliders. Sharpen the subject, not the background. Glow, grain and lens blur only when they support the mood.
7. **Deliver alternatives** for a new photo: two or three real interpretations (for example natural, dramatic, monochrome), named by their idea. When the user already has a style or asks for one version, deliver one strong version.

## Check before delivering

**Defects: fix these.**

- **Mask coverage**: every part of the subject is in the mask; background seen through the subject is not.
- **Broken local relationships**: neighbouring parts of one surface keep roughly their raw brightness relationship. A sharp step across a small feature means a mask edge runs through it.
- **Edges**: no halo or dark rim around the subject, no noisy lifted shadows.
- **Unintended casts**: whites carry the colour of the light you meant, not a side effect of another move.

**Questions to answer for this photograph** (prompts, not thresholds):

- Does the subject separate, by brightness, colour or texture?
- Does the frame hold the eye?
- Is the mood the light's mood? If the edit is much brighter than the raw, know why.
- Does it read at thumbnail size, with the eye landing where you meant?
- Is it clearly better than the raw, or only different?

## Common failures

- **Timid globals**: exposure +0.3, contrast +20 and nothing else gives a brighter, flatter raw.
- **Brightening the mood away**: blue hour turned to daylight, dusk noise exposed.
- **No separation**: subject and background equally bright, coloured and sharp. The fix is a subject mask and its inverse, not more global contrast.
- **A spotlight look**: a lifted oval or glowing halo around the subject.
- **Colour casts presented as style**: teal water, cyan snow, magenta shadows.
- **Every photo the same**: one grade on a whole series regardless of light.
