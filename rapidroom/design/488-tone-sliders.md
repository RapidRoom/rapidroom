# Tone sliders: strength, version, and per-camera baseline

Issue #488. This note is the design for the tone-panel change. It changes no renderer, no sidecar, and no preset. The decisions at the end are for the maintainer. Implementation follows in a later pull request after those decisions.

Code was read at main `1bcb83f0`. The J40 renders this note fits were made at `cd3484fe`. The flat-field model of the current slider functions, read at `1bcb83f0`, reproduces those ramp renders to a mean absolute error of at most 0.13 encoded luma, so this note treats the two as the same law. The four functions are the ones tracked upstream as `CyberTimon/RapidRAW#2061`. The version switch below is RapidRoom behaviour.

## Summary

- Whites, Blacks, and Highlights get a new response law so that slider ±100 on a new edit lands near the J40 Lightroom ramp. Shadows uses the same law, because one formula covers all four and Shadows +100 is the end that is still far from Lightroom on the ramp.
- The law sits behind a process-version key. Edits with no key keep today's response and stay byte-identical. New edits and raw Lightroom imports write the key and use the new law. This is the recommended choice. Extending the slider ends keeps today's weak ±100 and cannot fix the shape.
- The default-brightness gap is a per-camera baseline exposure in the existing camera table. It is not a change to the global raw output curve. The grey ramp has no camera profile and stays on the current curve.
- A migration utility rewrites an old edit onto the new key, one slider at a time, matching the ramp as closely as one number can. It does not run on open.
- `rapidroom/design/488-tone-slider-fit.py` evaluates the published coefficients against the J40 comparison JSON.

## What the ramp shows

J40 measured one slider at a time on a 22-step ramp, from −6 EV to +1 EV around linear 0.18, and on three camera files. Deltas below are encoded Rec.709 luma on the 8-bit sRGB JPEG, relative to that engine's own default. The current UI value is divided before the shader: Highlights and Shadows by 120, Whites and Blacks by 40 (`SCALES` in `image_processing.rs`). The ramp centres are flat, so the local-contrast terms are idle. The model here is that flat-field path plus the raw default output (`linear_to_srgb`, power 1/1.1, then 0.25 identity + 0.75 smoothstep).

The miss is the window, not only the gain. A single scale on today's delta leaves a large shape residual:

| Slider end | Least-squares scale of today's ramp delta onto Lightroom | Residual RMS after that scale |
| --- | ---: | ---: |
| Highlights −100 | ×2.28 | 13.7 |
| Highlights +100 | ×1.26 | 7.7 |
| Shadows −100 | ×1.43 | 4.0 |
| Shadows +100 | ×1.98 | 23.5 |
| Whites −100 | ×4.17 | 9.9 |
| Whites +100 | ×4.70 | 23.1 |
| Blacks −100 | ×2.82 | 22.8 |
| Blacks +100 | ×1.41 | 10.2 |

Highlights and Whites in the current law do nothing on the ramp until about −1 EV. Lightroom's Highlights and Whites are already moving by −3 EV. Blacks is a spike at the toe (the shader exponent is 12) while Lightroom's Blacks −100 still pulls the midtones. Shadows −100 is the closest of the eight ends. Shadows +100 lifts the midtones and barely lifts the toe, where Lightroom's lift is several times larger.

On the three photographs the same pattern shows up as very small mean-luma movement for Whites, and as camera-dependent Highlights numbers. Matching overall brightness first does not repair it. That measurement is already on #488.

## Process version or a wider range

Both options were asked for. They do different jobs.

**Process version, recommended.** A sidecar key selects the law. Stored ±100 stays ±100. On version 1 those numbers use the new law, so a Lightroom Highlights +100 imports as Highlights +100 and renders near the Lightroom ramp. On a missing key, ±100 keeps today's law, which is what makes old edits and the matched full60 byte-identical. MCP, presets, and the UI keep the range −100 to 100.

**Wider slider ends.** The current law stays, and the UI allows values past ±100, large enough that some position matches Lightroom's amplitude. The scale table above puts that position around ×2 to ×5, and Whites on the Sony file was far beyond that in the original pixel metric. The stored number that matches Lightroom is then no longer 100, so a Lightroom import cannot copy the number across. The shape residual in the table remains, because a longer travel on the same window still starts too late. Old ±100 would stay byte-identical without a version key. That is the only advantage, and the process version has the same property for key-absent files.

The recommendation is the process version.

## Proposed response law

Version 1 replaces the flat luma mapping for these four sliders. Detail reinjection stays at today's coefficients. The J40 ramp centres do not measure it.

Let the UI slider be a value in −100…100 and `s = value / 100`. Let `t = max(linear, 0) ^ 0.4545`. For each slider the note publishes seven coefficients: positive amplitude `A+`, negative amplitude `A−`, positive exponent `g+`, negative exponent `g−`, threshold `t0`, power `p`, and rolloff `q`.

```
g = g+ if s >= 0 else g−
A = A+ if s >= 0 else −A−
delta = A * |s| ^ g * max(t − t0, 0) ^ p / (1 + t) ^ q
t' = max(t + delta, 0)
linear' = t' ^ (1 / 0.4545)
```

The divisor `(1 + t) ^ q` stays defined above perceptual 1, so a specular is not cut off by a `(1 − t)` term. The ramp only reaches linear 0.36. Above that, hold the gain from linear 0.36:

```
linear' = linear'_at_0.36 * (linear / 0.36)    when linear > 0.36
```

That hold does not move any ramp step. It stops Highlights −100 from driving a bright linear value to black, which the raw power does at linear 4. The gain it freezes is the gain at the top ramp step, and that step is already a few luma stronger than Lightroom for Highlights +100 and Whites +100. A bright specular is part of the human check in the test plan.

Coefficients, four decimal places:

| Slider | A+ | A− | g+ | g− | t0 | p | q |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Highlights | 5.3936 | 6.0134 | 0.9184 | 1.0243 | −0.1437 | 5.2500 | 4.5634 |
| Shadows | 5.5057 | 3.8280 | 1.2969 | 1.0059 | 0.0437 | 0.8285 | 10.9305 |
| Whites | 3.9336 | 1.8512 | 1.9202 | 0.6126 | 0.0024 | 2.8656 | 4.0217 |
| Blacks | 2.6903 | 6.8315 | 1.0891 | 1.6427 | 0.0448 | 0.9347 | 9.9743 |

Version 0 does not use this law and does not use these coefficients. It keeps `SCALES` (120 and 40) and the current functions `apply_tonal_adjustments` and `apply_highlights_adjustment`.

Separate exponents per sign are there because one shared exponent cannot put Whites ±50 and Whites ±100 on Lightroom together. Highlights and Shadows are close to linear (`g` near 1) and still use the same formula so there is one implementation.

### Ramp curves at ±100

Encoded-luma delta from each engine's own default. "cur" is today's flat-field law. "new" is the law above. "LR" is the J40 Lightroom export. EV is the ramp step.

#### Highlights

| EV | linear | cur −100 | new −100 | LR −100 | cur +100 | new +100 | LR +100 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| −6.00 | 0.0028 | 0.0 | −0.2 | −0.0 | 0.0 | 0.2 | 0.1 |
| −5.67 | 0.0035 | 0.0 | −0.3 | −0.2 | 0.0 | 0.3 | 0.1 |
| −5.33 | 0.0045 | 0.0 | −0.4 | −0.2 | 0.0 | 0.3 | 0.3 |
| −5.00 | 0.0056 | 0.0 | −0.4 | −0.4 | 0.0 | 0.4 | 0.4 |
| −4.67 | 0.0071 | 0.0 | −0.6 | −0.5 | 0.0 | 0.5 | 0.4 |
| −4.33 | 0.0089 | 0.0 | −0.7 | −0.6 | 0.0 | 0.7 | 0.7 |
| −4.00 | 0.0112 | 0.0 | −0.9 | −0.9 | 0.0 | 0.9 | 0.9 |
| −3.67 | 0.0142 | 0.0 | −1.2 | −1.2 | 0.0 | 1.1 | 1.4 |
| −3.33 | 0.0179 | 0.0 | −1.6 | −1.7 | 0.0 | 1.5 | 1.7 |
| −3.00 | 0.0225 | 0.0 | −2.1 | −2.2 | 0.0 | 1.9 | 2.3 |
| −2.67 | 0.0283 | 0.0 | −2.8 | −2.8 | 0.0 | 2.6 | 2.9 |
| −2.33 | 0.0357 | 0.0 | −3.7 | −3.4 | 0.0 | 3.4 | 3.6 |
| −2.00 | 0.0450 | 0.0 | −5.0 | −4.1 | 0.0 | 4.5 | 4.4 |
| −1.67 | 0.0567 | 0.0 | −6.6 | −4.9 | 0.0 | 6.0 | 5.2 |
| −1.33 | 0.0714 | −0.0 | −8.8 | −5.9 | 0.0 | 8.0 | 6.2 |
| −1.00 | 0.0900 | −0.0 | −11.7 | −7.0 | 0.5 | 10.6 | 7.4 |
| −0.67 | 0.1134 | −0.4 | −15.5 | −8.5 | 2.4 | 14.0 | 8.7 |
| −0.33 | 0.1429 | −1.6 | −20.4 | −19.7 | 6.9 | 18.2 | 19.2 |
| 0.00 | 0.1800 | −5.0 | −26.7 | −31.7 | 14.8 | 23.4 | 29.3 |
| 0.33 | 0.2268 | −11.8 | −34.5 | −41.7 | 25.0 | 29.2 | 35.6 |
| 0.67 | 0.2857 | −20.3 | −44.2 | −48.4 | 31.4 | 35.3 | 37.4 |
| 1.00 | 0.3600 | −27.7 | −55.7 | −50.0 | 32.3 | 40.3 | 32.4 |

#### Shadows

| EV | linear | cur −100 | new −100 | LR −100 | cur +100 | new +100 | LR +100 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| −6.00 | 0.0028 | −3.9 | −4.5 | −2.3 | 4.9 | 31.3 | 20.8 |
| −5.67 | 0.0035 | −5.1 | −5.8 | −4.1 | 6.0 | 38.2 | 31.8 |
| −5.33 | 0.0045 | −6.4 | −7.5 | −6.2 | 7.4 | 44.4 | 41.9 |
| −5.00 | 0.0056 | −7.9 | −9.4 | −8.7 | 8.9 | 49.7 | 49.7 |
| −4.67 | 0.0071 | −9.5 | −11.7 | −11.3 | 10.7 | 54.0 | 55.8 |
| −4.33 | 0.0089 | −11.1 | −14.4 | −14.1 | 12.7 | 57.2 | 61.0 |
| −4.00 | 0.0112 | −12.5 | −17.5 | −17.0 | 15.0 | 59.1 | 64.0 |
| −3.67 | 0.0142 | −13.8 | −21.1 | −20.1 | 17.4 | 59.8 | 65.5 |
| −3.33 | 0.0179 | −15.1 | −24.7 | −23.3 | 19.8 | 59.2 | 65.0 |
| −3.00 | 0.0225 | −16.2 | −27.7 | −26.4 | 22.3 | 57.4 | 62.2 |
| −2.67 | 0.0283 | −17.1 | −28.9 | −29.6 | 24.5 | 54.5 | 58.4 |
| −2.33 | 0.0357 | −17.7 | −28.9 | −31.5 | 26.4 | 50.6 | 53.0 |
| −2.00 | 0.0450 | −18.0 | −27.8 | −31.5 | 27.6 | 46.0 | 46.8 |
| −1.67 | 0.0567 | −17.7 | −25.9 | −29.0 | 27.9 | 40.9 | 39.5 |
| −1.33 | 0.0714 | −16.9 | −23.3 | −24.7 | 27.2 | 35.5 | 31.1 |
| −1.00 | 0.0900 | −15.5 | −20.3 | −18.4 | 25.4 | 30.0 | 21.1 |
| −0.67 | 0.1134 | −13.5 | −17.0 | −10.2 | 22.5 | 24.6 | 10.3 |
| −0.33 | 0.1429 | −11.2 | −13.7 | −9.2 | 18.6 | 19.5 | 8.9 |
| 0.00 | 0.1800 | −8.5 | −10.6 | −8.2 | 14.1 | 15.0 | 7.9 |
| 0.33 | 0.2268 | −5.9 | −7.8 | −6.8 | 9.5 | 11.0 | 6.7 |
| 0.67 | 0.2857 | −3.6 | −5.5 | −5.4 | 5.5 | 7.8 | 5.3 |
| 1.00 | 0.3600 | −1.9 | −3.7 | −4.1 | 2.6 | 5.2 | 4.0 |

Shadows +100 still lifts the upper ramp more than Lightroom (at −0.67 EV, +24.6 against +10.3) and the deepest toe more than Lightroom (+31.3 against +20.8). The middle of the toe is close.

#### Whites

| EV | linear | cur −100 | new −100 | LR −100 | cur +100 | new +100 | LR +100 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| −6.00 | 0.0028 | 0.0 | −0.1 | 0.0 | 0.0 | 0.2 | 0.1 |
| −5.67 | 0.0035 | 0.0 | −0.1 | 0.0 | 0.0 | 0.3 | 0.1 |
| −5.33 | 0.0045 | 0.0 | −0.2 | −0.1 | 0.0 | 0.5 | 0.2 |
| −5.00 | 0.0056 | 0.0 | −0.3 | −0.1 | 0.0 | 0.6 | 0.3 |
| −4.67 | 0.0071 | 0.0 | −0.4 | −0.1 | 0.0 | 0.9 | 0.4 |
| −4.33 | 0.0089 | 0.0 | −0.6 | −0.2 | 0.0 | 1.2 | 0.6 |
| −4.00 | 0.0112 | 0.0 | −0.8 | −0.2 | 0.0 | 1.7 | 0.9 |
| −3.67 | 0.0142 | 0.0 | −1.0 | −0.4 | 0.0 | 2.3 | 1.4 |
| −3.33 | 0.0179 | 0.0 | −1.4 | −0.6 | 0.0 | 3.1 | 2.0 |
| −3.00 | 0.0225 | 0.0 | −1.9 | −0.8 | 0.0 | 4.1 | 3.0 |
| −2.67 | 0.0283 | 0.0 | −2.5 | −1.2 | 0.0 | 5.4 | 4.4 |
| −2.33 | 0.0357 | 0.0 | −3.3 | −1.6 | 0.0 | 7.1 | 6.0 |
| −2.00 | 0.0450 | 0.0 | −4.2 | −2.3 | 0.0 | 9.3 | 8.3 |
| −1.67 | 0.0567 | 0.0 | −5.5 | −3.2 | 0.0 | 11.9 | 11.5 |
| −1.33 | 0.0714 | 0.0 | −6.9 | −4.6 | 0.0 | 15.0 | 15.8 |
| −1.00 | 0.0900 | 0.0 | −8.6 | −6.6 | 0.0 | 18.6 | 21.5 |
| −0.67 | 0.1134 | 0.0 | −10.6 | −9.3 | 0.0 | 22.6 | 26.7 |
| −0.33 | 0.1429 | −0.0 | −12.8 | −12.0 | 0.0 | 27.0 | 30.6 |
| 0.00 | 0.1800 | −0.4 | −15.2 | −14.9 | 0.5 | 31.3 | 33.6 |
| 0.33 | 0.2268 | −1.5 | −17.6 | −18.0 | 1.9 | 35.3 | 35.4 |
| 0.67 | 0.2857 | −3.8 | −19.9 | −21.3 | 4.8 | 38.1 | 35.6 |
| 1.00 | 0.3600 | −7.6 | −21.7 | −24.4 | 9.5 | 39.1 | 33.7 |

#### Blacks

| EV | linear | cur −100 | new −100 | LR −100 | cur +100 | new +100 | LR +100 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| −6.00 | 0.0028 | −4.5 | −4.5 | −2.5 | 11.5 | 8.8 | 10.7 |
| −5.67 | 0.0035 | −5.8 | −5.8 | −4.3 | 12.8 | 11.3 | 11.6 |
| −5.33 | 0.0045 | −7.4 | −7.5 | −6.7 | 13.9 | 13.8 | 12.3 |
| −5.00 | 0.0056 | −9.0 | −9.4 | −9.5 | 14.9 | 16.3 | 13.4 |
| −4.67 | 0.0071 | −10.7 | −11.7 | −12.6 | 15.6 | 18.6 | 14.8 |
| −4.33 | 0.0089 | −12.2 | −14.4 | −16.2 | 16.0 | 20.7 | 16.6 |
| −4.00 | 0.0112 | −13.1 | −17.5 | −20.4 | 16.1 | 22.4 | 18.6 |
| −3.67 | 0.0142 | −13.0 | −21.2 | −25.6 | 15.8 | 23.7 | 20.6 |
| −3.33 | 0.0179 | −12.5 | −25.5 | −32.0 | 15.1 | 24.6 | 22.3 |
| −3.00 | 0.0225 | −11.5 | −30.5 | −39.5 | 13.9 | 24.9 | 23.6 |
| −2.67 | 0.0283 | −10.2 | −36.3 | −48.3 | 12.5 | 24.7 | 24.4 |
| −2.33 | 0.0357 | −8.5 | −42.2 | −55.6 | 10.8 | 24.0 | 24.6 |
| −2.00 | 0.0450 | −6.8 | −45.7 | −53.2 | 8.9 | 22.8 | 24.7 |
| −1.67 | 0.0567 | −5.1 | −45.8 | −49.1 | 6.9 | 21.1 | 24.5 |
| −1.33 | 0.0714 | −3.5 | −43.9 | −43.5 | 5.0 | 19.1 | 23.8 |
| −1.00 | 0.0900 | −2.2 | −40.4 | −38.4 | 3.3 | 16.8 | 22.5 |
| −0.67 | 0.1134 | −1.2 | −35.7 | −34.0 | 1.9 | 14.4 | 19.7 |
| −0.33 | 0.1429 | −0.6 | −30.3 | −28.8 | 1.0 | 12.0 | 15.8 |
| 0.00 | 0.1800 | −0.2 | −24.7 | −21.7 | 0.4 | 9.6 | 12.1 |
| 0.33 | 0.2268 | −0.1 | −19.2 | −15.5 | 0.1 | 7.4 | 8.7 |
| 0.67 | 0.2857 | −0.0 | −14.2 | −10.6 | 0.0 | 5.4 | 5.9 |
| 1.00 | 0.3600 | 0.0 | −9.9 | −6.8 | 0.0 | 3.8 | 3.5 |

Blacks −100 is the end the single window fits least well. Around −2.3 EV the proposed pull is about 13 luma short of Lightroom (−42 against −56). The toe and the midtones are much closer than today's spike. A second term could chase that notch. This note does not add one. That is a decision below.

## Expected ratios after the change

Two ramp metrics, both on the 22 steps:

- **Mean ratio.** `100 * sum(RapidRoom delta) / sum(Lightroom delta)`.
- **Pixel strength.** The J40 definition: `100 * (d · d) / (d · d_LR)` on steps where both deltas are at least 0.25 luma and have the same sign. Pixel strength near 100 means the best scale of the proposed delta onto Lightroom is 1. It can sit next to a real shape residual. The RMS column is that residual, in encoded luma.

| Slider | value | mean now | mean proposed | pixel now | pixel proposed | proposed RMS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Highlights | −100 | 28.3 | 103.4 | 44.1 | 99.6 | 3.09 |
| Highlights | −50 | 32.5 | 103.2 | 52.4 | 99.9 | 1.50 |
| Highlights | +50 | 56.5 | 102.8 | 76.8 | 100.1 | 1.36 |
| Highlights | +100 | 56.4 | 101.4 | 79.0 | 100.6 | 2.91 |
| Shadows | −100 | 72.8 | 104.5 | 69.3 | 101.1 | 2.36 |
| Shadows | −50 | 65.2 | 107.4 | 62.1 | 103.1 | 1.91 |
| Shadows | +50 | 52.3 | 105.1 | 58.6 | 100.2 | 3.04 |
| Shadows | +100 | 44.0 | 105.0 | 50.6 | 99.9 | 6.05 |
| Whites | −100 | 10.7 | 111.3 | 24.9 | 100.0 | 1.31 |
| Whites | −50 | 8.1 | 104.6 | 23.9 | 99.9 | 0.50 |
| Whites | +50 | 12.0 | 105.0 | 33.8 | 99.8 | 0.37 |
| Whites | +100 | 6.2 | 101.4 | 20.5 | 100.3 | 1.99 |
| Blacks | −100 | 24.0 | 93.3 | 35.2 | 91.2 | 5.26 |
| Blacks | −50 | 32.4 | 99.8 | 39.1 | 97.8 | 2.94 |
| Blacks | +50 | 47.7 | 96.5 | 61.0 | 100.1 | 1.26 |
| Blacks | +100 | 52.4 | 97.7 | 70.9 | 99.8 | 2.99 |

Whites −100 has mean ratio 111 and pixel strength 100. The extra mean comes from small deltas on the dark steps, where Lightroom is almost still. The shape on the steps that actually move matches.

These ratios are the ramp, at baseline exposure 0, with one slider moved. They are the numbers the implementation should hit before anyone looks at a photograph. Photograph ratios are not estimated here. A per-luma scale taken from the ramp and applied to today's photo bins mis-states a window that moves. The J40 rerun measures the photographs. Step A already showed that lifting overall brightness, without this law, leaves Whites and Blacks weak.

## Per-camera baseline

The remaining default-brightness gap is not a global curve. The ramp's +40 luma at 18% grey (RapidRoom 125.9, Lightroom 165.9) is Lightroom's base curve. The point curve on those exports is identity, and the ramp has no look name. The maintainer's direction puts the camera gap into camera correction and leaves that base curve alone. Unprofiled files, including the ramp, stay on the current output curve and stay darker than Lightroom. A single exposure cannot close the ramp: Lightroom is darker at −6 EV and lighter at 18% grey.

The table already exists. `camera_baseline(make, model)` in `baseline_exposure.rs` returns 0.35 EV for Sony ILCE-7CM2 and ILCE-7RM5, and nothing for every other model. `RawExposure::read` uses an explicit DNG `BaselineExposure` plus `BaselineExposureOffset` when those tags exist, including an explicit zero, and only then falls through to the table. Highlight preservation from the maker note is a separate per-file number, added in `camera_compensation` when the sidecar asks for camera compensation. `default_adjustments()` turns that request on. An empty object `{}` does not, and renders at 0 EV from this path. Growing the table therefore does not move old empty sidecars.

**Where the new numbers live.** In that same `camera_baseline` table, one EV per make and model. Version 1 of this work does not add a profile stack. The number is the whole default gap the calibration measures, including the Adobe Color look that the J40 camera exports used. A later profile system can split those. The app-wide baseline setting (`enable_baseline_exposure`, default off, amount 0.7) is a user preference and is not the fallback for a missing model.

**How to calibrate a model.** Same shape as J40.

1. Export one or more raws from Lightroom at Process Version 15.4, tone sliders 0, with the camera default used in J40 (Adobe Standard profile and the Adobe Color look on the three bodies).
2. Render the same file in RapidRoom with the four sliders at 0, the camera table forced to 0, and highlight preservation left as the file already has it. Canon's lighting-optimizer compensation stays on during the search so it is not stored twice.
3. Search the EV which, added as `camera_baseline`, brings RapidRoom's mean encoded luma closest to the Lightroom export. Record a grey patch too when the frame has one. The stored target is the mean luma.
4. Replace any previous table entry for that model with the winning EV. Sony's 0.35 is replaced by the search, not added to it.

The search has to be a render. Two existing points are only seeds. Posted on #488, legacy mean luma against the Lightroom default: Canon R6 Mark II 66.6 versus 89.7 (gap 23.0), Nikon Zf 102.4 versus 136.6 (gap 34.3), Sony A7C II 70.6 versus 116.7 (gap 46.1). With today's fresh path (Canon highlight preservation +0.50, Sony table +0.35, Nikon 0) the gaps left are 8.5, 34.3 and 34.4. A single EV will still miss the toe. The calibration records that toe error and does not chase it with a curve.

**Fallback.** A model with no table entry contributes 0 EV from this table. DNG baseline and maker-note highlight preservation still apply as they do today. The global 0.7 setting is not substituted.

The +0.65 EV trial in the #488 measurement comment is a global exposure in front of the current formula. It is a measurement of the ramp, and this design does not ship it. Stacking a ramp-shaped curve on Canon's existing +0.50 overshot that camera in the same measurement.

## Version, migration, MCP, and presets

The key is `toneSliderVersion`, an integer on the adjustments object.

| Value | Law |
| --- | --- |
| absent, or 0 | today's functions and `SCALES` |
| 1 | the law in this note |
| any later integer | the newest law this build implements, which is 1 until a later note says otherwise |

Opening a file must not insert the key. That is the same discipline as `normalize_lens_policy_version` for `cameraLensCorrectionVersion`: a missing version stays missing. It is deliberately unlike `copy_exposure_policy`, which treats a null source as baseline-on. Copying a tone version copies the key only when the source has it.

`default_adjustments()` writes `toneSliderVersion: 1` for a new raw. With all four sliders at 0 the two laws match, so a fresh default that never moves a slider is still the same picture as today, plus whatever camera baseline the table contributes once calibration has landed. Old recipes that already set a slider and have no key stay on law 0 even after `default_adjustments` starts writing the key for new files.

**Lightroom imports.** Raw and develop XMP keep the current one-to-one copy of `Highlights2012`, `Shadows2012`, `Whites2012`, and `Blacks2012` (`BASIC_MAPPINGS` in `preset_converter.rs`) and write `toneSliderVersion: 1`. The numbers are already in Lightroom's ±100. The old idea of scaling those numbers on import was a compensation for the weak law. Version 1 removes the need for it.

The rendered-JPEG PV2012 path is a different mapping. It sends exposure through brightness, scales Highlights by a fitted slope near 0.47, and splits Blacks into Blacks, Shadows, and Contrast. Those coefficients were fit to today's law. This note leaves that path on version 0: it does not write `toneSliderVersion`. Refitting it onto version 1 is a separate calibration. Doing both at once would apply the old affine and the new law together.

**MCP.** The four sliders stay documented as −100…100 in `mcp/adjustments.rs`. The schema gains an optional integer `toneSliderVersion`. The description states that version 1 is the Lightroom-fitted law and that an absent key keeps the previous law. A patch that sets a slider does not write the key. A new adjustments object built from `default_adjustments()` includes the key, as above.

**Presets.** A preset saved from a version 1 edit stores the key. Applying a preset that has the key sets it. Applying a preset that lacks the key leaves the target's key as it was. An old preset applied to an old edit therefore keeps law 0. An old preset applied to a new edit keeps law 1 and interprets the preset's slider numbers on the new law, which will look stronger. That is the cost of not rewriting every shipped preset. The migration utility is how a user converts a preset or a sidecar on purpose.

**Migration utility.** Opt-in. It does not run on open, on import, or on preset apply. For each of the four sliders independently, it finds the version 1 value whose ramp delta is closest in mean square to the version 0 delta at the stored value, keeping the sign, and then sets `toneSliderVersion: 1`. Other controls are copied unchanged. J40 moved one slider at a time, so the utility does not invent a joint fit. The search on a 0.25 grid:

| Slider | −100 → | ramp RMS | −50 → | ramp RMS | +50 → | ramp RMS | +100 → | ramp RMS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Highlights | −39.75 | 3.11 | −22.75 | 1.80 | 34.00 | 1.89 | 68.00 | 3.54 |
| Shadows | −59.50 | 2.10 | −27.75 | 1.28 | 28.00 | 3.82 | 50.00 | 8.04 |
| Whites | −4.25 | 1.29 | −1.50 | 0.69 | 19.50 | 0.81 | 27.75 | 1.61 |
| Blacks | −33.75 | 3.59 | −23.75 | 2.27 | 27.25 | 1.76 | 59.25 | 4.79 |

Whites −100 becoming about −4 is the strength change, not a bug: today's Whites −100 is a small movement, and the new law reaches that movement early because `g−` is 0.61. Shadows +100 becoming +50 still leaves 8 luma RMS, because the old lift peaks in the midtones and the new one is a toe lift. One number cannot match both shapes. The utility reports that residual. The maintainer has almost no personal edits; the utility exists for other users and is allowed to be approximate.

## Test plan

No render is part of this note. The implementation pull request does the following, and stays open under `needs-human` and `rendering-change` until a person has looked.

1. **Ramp, before photographs.** Run `488-tone-slider-fit.py` against the J40 comparison JSON and confirm it still prints the ratio table above. That locks the coefficients.
2. **J40 rerun.** Same four files, same Lightroom exports, JPEG quality 100, sRGB, full size, no output sharpening. Version 1, one slider at ±50 and ±100, camera table still 0 so the ramp comparison is the law alone. Hold the GPU lock only for the render. The ramp's pixel strength should land within 5 points of the "pixel proposed" column, and the step RMS within 1 luma of the "proposed RMS" column. Blacks −100 is accepted at about 91, not forced to 100. Then repeat the three cameras and publish their mean-luma ratios and pixel strengths. Those photo numbers are evidence, not a gate this note can set.
3. **Camera baseline, after a model is calibrated.** Slider 0, table value applied, highlight preservation as the file has it. Report the mean-luma gap to the Lightroom default. The ramp is not required to close.
4. **Old edits.** The matched full60 stays byte-identical to the blessed reference. In this repo that check is the pixel-exact full60, 60/60, on sidecars that do not contain `toneSliderVersion`. Re-render the J40 version-absent slider set with the same export settings and require it to be pixel-exact against the existing J40 renders. A zero slider under version 1 is included in that set and must match version 0.
5. **Migration.** Run the utility on copies of the J40 one-slider sidecars. The chosen values stay within one 0.25 step of the table above, and the ramp RMS stays at or under that table. The migrated render is not required to be byte-identical to the old render.
6. **Imports.** A raw XMP with Whites +100 writes `toneSliderVersion: 1` and `whites: 100`. A rendered-JPEG XMP that uses the PV2012 policy writes no `toneSliderVersion`. An MCP patch that sets Highlights does not insert the key. Applying a preset that lacks the key does not remove a key already on the target.

## Decisions for the maintainer

1. Adopt the process version, as recommended, or adopt wider slider ends instead.
2. Adopt the coefficients, the per-sign exponents, and the gain hold above linear 0.36. The hold's bright-end gain is whatever the top ramp step does, which is a few luma stronger than Lightroom for Highlights +100 and Whites +100.
3. Accept the remaining shape error, or ask for another term before implementation. The two ends to look at are Blacks −100 near −2.3 EV (about 13 luma short) and Shadows +100 through the upper half of the ramp (still lifting where Lightroom has mostly stopped).
4. Put the default-brightness gap in `camera_baseline`, calibrated per model by the mean-luma search above, with fallback 0. Leave the global raw output curve as it is, including on the ramp.
5. Keep rendered-JPEG imports on law 0 until that policy is refit. Raw and develop imports write law 1 and copy the four slider numbers unchanged.
6. Ship the migration utility as opt-in, with the one-slider ramp search, and do not run it on open.

## What this pull request does not do

It does not change the shader, the scales, import, MCP, or presets. It does not add a change record. It does not render, and it does not create a `target/` directory. #488 stays open.
