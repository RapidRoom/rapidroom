<!--
Adapted from photo-style-builder/references/film-emulation.md in Lightweft by sheldonxxxx and the
Lightweft contributors,
https://github.com/sheldonxxxx/Lightweft/blob/d332fbd629faf7e47144ba173d7f2032be62199d/skills/photo-style-builder/references/film-emulation.md
Copyright (c) 2026 Lightweft contributors. MIT licence: this file is under the MIT licence,
not the AGPL; see ../../../LICENSES/Lightweft-MIT.txt. Changes for RapidRoom: shortened, how to
set the look in RapidRoom added, Lightweft's review-app steps and manufacturer links left out.
-->

# Choosing a film emulation

RapidRoom ships six film looks (Spektrafilm LUTs) in its `film_luts` resource folder: **F-Pro 400H**, **K-Ektar 100**, **K-Gold 200**, **K-Portra 400**, **K-Vision 250D Cinematic** and **K-Vision 500T Cinematic**. Use them to support a photograph, not as a default.

## Setting one

A look is the `lutPath` key: the absolute path of the `.cube` file. `lutIntensity` (0–100) blends it. The files are in the app's resources, for example `/usr/lib/RapidRoom/resources/film_luts/K-Portra 400.cube` for the .deb; find the real path on this machine before using it (`find / -name 'K-Portra 400.cube' 2>/dev/null` or ask the user). Setting `lutPath` over MCP is untested: check the preview, and if the look doesn't show, ask the user to pick it in Effects → LUT.

## When to try each look

These observations come from Lightweft's matched comparisons on two photos (an autumn landscape and a portrait in mixed light). They describe those samples, not a universal ranking.

| Look                        | Try it when                                                                                      | What it did in the samples                                              | Reduce or skip when                                                                    |
| --------------------------- | ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| **F-Pro 400H**              | People among foliage, environmental portraits, travel: warm subjects against cooler surroundings | Richer reds and greens, cooler darks; stronger on portraits than Portra | Green or cyan spill reaches skin, or delicate foliage gets loud                        |
| **K-Ektar 100**             | Colour is the picture: autumn, flowers, painted walls, bold travel scenes                        | The strongest red shift and colour boost; warms skin a lot              | Skin must stay understated, reds are near their limit, or mist and faint colour matter |
| **K-Gold 200**              | Warm, familiar everyday light: sunny days, beach, family                                         | Warmer orange foliage and warm skin; less red than Ektar                | Sunset or tungsten light already dominates, or whites must stay neutral                |
| **K-Portra 400**            | Faces and gestures stay central with a gentle film feel; a good first try when skin matters      | Gentler than Ektar; at half strength it kept more of the base           | Skin turns orange or dark hair and clothing lose detail                                |
| **K-Vision 250D Cinematic** | Daylight or window-light scenes that want a coherent cinematic palette and weight in the darks   | Deeper shadows, a different colour balance from the still-film looks    | A shaded face sinks behind bright scenery, or daylight colour must stay natural        |
| **K-Vision 500T Cinematic** | Warm practical lights against cool surroundings, night and interiors                             | More muted colour, heavier darks; also usable in daylight               | Skin goes cold, shadows lose detail or colour noise takes over                         |

## How to suggest one

- Read the photo first: subject, light, dominant colours, how much saturation headroom there is, noise. Decide whether film adds something specific. A neutral, colour-faithful result is a valid recommendation.
- Suggest a leading look and, if useful, one contrast. Say the visible reason and the cost: "Gold could tie the amber leaves together; I'd watch that the far bank still recedes."
- Treat strength as part of the decision: try the look at full strength to see its direction, then lower `lutIntensity` while the gain survives.
- Keep apart: suggested, rendered and looked at, and accepted by the user.

## What a film name doesn't do

A LUT doesn't change ISO, recover highlights, remove noise or add grain and halation. Those are separate adjustments (`grainAmount`, `halationAmount`) with their own judgement. `lutIsSceneReferred` changes where in the pipeline the LUT applies; a wrong setting can overwhelm any artistic choice, so leave it as the user had it unless you know the LUT was made for scene-referred input.
