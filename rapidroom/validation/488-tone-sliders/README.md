# Tone slider process version (#488)

Version 1 uses the approved `rapidroom/design/488-tone-sliders.md` law for
user-facing global **and mask** Highlights, Shadows, Whites and Blacks. Internal
glow/halation/flare calls retain version 0. The fitted law replaces the entire
flat response, including the shadow stretch; the legacy detail reinjection
coefficients and legacy lift amount are retained. These scope choices were
clarified by the coordinating agent after the design approval.

Missing `toneSliderVersion` and explicit 0 select the existing law. Opening an
existing recipe does not add the key. New edits and raw/develop imports select

1. Later integer versions use the newest supported law. Raw import slider
   numbers remain unchanged. Rendered imports retain the previous conversion law.
   Presets capture a present key with tone controls, apply a present key discretely,
   and preserve the target key when the preset omits it.

## Opt-in conversion

From the Basic panel's context menu, choose **Convert tone sliders to the new
response (approximate)**. This converts the global controls and every mask's
controls, adds version 1 and makes one undo step. The notification reports the
largest encoded-luma ramp residual. The app never converts on open, import or
preset application.

For a sidecar, preset or bare adjustments JSON:

```sh
RapidRAW migrate-tone INPUT.json OUTPUT.json
```

The command handles one sidecar, one preset object, or bare adjustments; preset-library arrays are not supported.

The output must not already exist. The command preserves the source, changes
only the four tone numbers and the version, and prints each control's old/new
value and ramp RMS as JSON. Version 1 or later inputs are left unchanged. Each
control is fit independently, with its sign retained, on the approved 0.25 grid
and 22-step ramp. Approximation cannot preserve all shapes or combinations.

## Evidence

`model-fit.txt` reproduces the approved coefficient and migration tables using
`488-tone-slider-fit.py` and the original J40 `comparison.json`. This is a model
check; real-render evidence is recorded separately when complete.
`shader-probe-summary.json` records direct production-WGSL checks on the GPU,
including exact legacy detail samples and the bright-specular gain hold.

Camera calibration covers the J40 Sony A7C II, Canon R6 II and Nikon Zf. The
RX100 VII has no J40 export and requires a future calibration export from the
maintainer. No global raw output curve change is intended.

The baseline table is deliberately independent of `toneSliderVersion`: existing
law-0 edits with `baselineExposure: true` and `cameraExposureCompensation: true`
also receive the calibrated exposure. Empty legacy recipes stay unchanged. The
uncalibrated Sony ILCE-7RM5 entry retains its existing +0.35 EV; unknown models
still fall back to 0 EV. These compatibility choices were explicitly confirmed
by the coordinating agent.
