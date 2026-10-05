# Look recipes

Starting points for common looks, in RapidRoom keys. Every photo needs its own amounts: apply a recipe as a proposed edit (`get_preview` with `adjustments`), look, then adjust. Build on the photo's base edit (white balance, exposure, crop) rather than replacing it. Ranges are in slider units; see [adjustments.md](adjustments.md) for what each key does.

Colour grading hues are in degrees: 0 red, 30 orange, 50 gold, 60 yellow, 120 green, 180 cyan, 210–230 blue-teal, 240 blue, 300 magenta. The shadow zone's tint is much weaker than the highlight zone's at the same saturation, so shadows usually need higher values.

## Clean and natural

The photo as it looked, at its best. Neutral but not flat.

- White balance true to the light: neutral greys stay neutral, golden light stays golden.
- `highlights` −20 to −40, `shadows` +10 to +30, `whites` and `blacks` small, to set a real white and black.
- `contrast` +5 to +15, `vibrance` +10 to +20, `saturation` 0.
- `clarity` 0 to +10. No grading, no vignette, or `vignetteAmount` −5 to −10.
- Check: skin looks like skin, nothing looks "edited".

## Cinematic

Contrast in colour rather than in tone; teal shadows, warm skin and highlights. Easy to overdo.

- `contrast` +10 to +20, `highlights` −30 to −50, `blacks` +5 to +15 for a slightly lifted black (or a point curve lifting `luma` at 0 to about 10–15).
- `colorGrading.shadows`: hue 200–220, saturation 30–60. `colorGrading.highlights`: hue 30–45, saturation 10–25. `colorGrading.balance` around 0.
- `hsl.greens.saturation` −20 to −40, `hsl.greens.hue` +10 to +30 (towards aqua); `hsl.oranges.saturation` 0 to +10 to keep skin.
- Optional: crop to a wide ratio (`aspectRatio` 2.39 or 1.85), `vignetteAmount` −10 to −20, `grainAmount` 10–20.
- Alternative: the **K-Vision 250D/500T Cinematic** film looks ([film-looks.md](film-looks.md)).
- Check: skin isn't orange, the shadows aren't green-blue mud, the sky isn't cyan.

## Gold and black

Warm, rich light against deep shadow. Suits evening light, interiors, portraits with directional light.

- `temperature` +10 to +25 (or keep the warmth the light already has).
- `exposure` slightly down (−0.1 to −0.3), `blacks` −10 to −30, `shadows` −10 to −20, `highlights` −10 to −20, `contrast` +10 to +25.
- `colorGrading.highlights`: hue 40–50, saturation 20–40. `colorGrading.midtones`: hue 35–45, saturation 5–15. Shadows untinted or very slightly warm.
- `hsl.blues.saturation` and `hsl.aquas.saturation` −30 to −60 so nothing cool competes; `hsl.yellows.hue` −5 to −15 towards gold.
- `vignetteAmount` −15 to −30. Masks: lift the lit subject (`exposure` +0.1 to +0.3), darken the rest.
- Check: the blacks still hold detail where it matters, the gold doesn't turn the whites yellow.

## Film

The feel of a film scan: softer contrast, coloured shadows, grain.

- Start from a film look: **K-Portra 400** for people, **K-Gold 200** for warm everyday scenes, **K-Ektar 100** for strong colour ([film-looks.md](film-looks.md)). `lutIntensity` 50–100.
- Or by hand: a point curve on `luma` lifting black to about 10–20 and pulling white down to about 240–245; `contrast` −5 to +10; `colorGrading.shadows` hue 180–220, saturation 20–40; `saturation` −5 to −15.
- `grainAmount` 15–35, `grainSize` 25–40, `grainRoughness` 40–60. Optional `halationAmount` 10–25 for highlights that bleed.
- Check: grain at 100 % looks like grain, not noise; skin isn't pushed orange.

## Moody and low-key

Most of the frame in shadow, the subject carried by light.

- `exposure` −0.2 to −0.6, `highlights` −20 to −40, `shadows` −10 to −30, `blacks` −10 to −25.
- `saturation` −10 to −25 or `vibrance` −10; cooler `temperature` (−5 to −15) unless the light is warm.
- Masks: subject `exposure` +0.2 to +0.4; edges and background `exposure` −0.3 to −0.8. `vignetteAmount` −20 to −35.
- Check: the subject still separates; the darks aren't empty (a small `shadows` lift if they are).

## Bright and airy

High-key and soft. Suits light scenes, people, food, interiors.

- `exposure` +0.2 to +0.5, `highlights` −20 to −40 (keep texture), `shadows` +20 to +40, `whites` small +.
- `contrast` −5 to −15, `blacks` +5 to +10, `clarity` −5 to 0.
- `temperature` slightly warm (+3 to +10), `vibrance` +5 to +10, `hsl.greens.saturation` −10 to −20.
- Check: the brightest areas still have texture; the image doesn't look washed out at thumbnail size.

## Black and white

When colour adds nothing: graphic light, gesture, pattern, harsh mixed light.

- `saturation` −100. Then shape tones with the colour mixer, which still acts on the original colours: `hsl.blues.luminance` −20 to −50 for a dark sky, `hsl.oranges.luminance` +10 to +20 for brighter skin, `hsl.greens.luminance` to separate foliage.
- `contrast` +10 to +30, set `blacks` and `whites` deliberately, `clarity` or `structure` +10 to +25 for texture.
- Optional: `grainAmount` 10–25; split tone with `colorGrading.highlights` hue 40, saturation 5–10 and `colorGrading.shadows` hue 210, saturation 20–30.
- Check: there is a real black and a clean white, and the main subject is the brightest or most contrasty area.

## Making several looks to compare

See [recipes.md](recipes.md#explore-looks). Name each candidate by its idea, keep them genuinely different, and let the user choose before refining one.
