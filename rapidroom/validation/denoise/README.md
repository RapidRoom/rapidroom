# AI denoise quality controls

The Rust `denoise_quality` fixtures cover actual overlap geometry, periodic reflection, constant/black edges, whole-image colour correction, high-frequency detail, cancellation and a synthetic star field across joins. The geometry control uses an identity model: it cannot establish learned-model image quality.

`star_field.py` measures peak, background-subtracted aperture flux, second-moment spread and aperture colour at fixed original-defined sites. Its NumPy inputs must be aligned **linear RGB** arrays in the same colour space, with identical crop, exposure and white balance. Supply manually/reference-qualified star sites; noise spikes selected on a denoised candidate are not valid stars. Never apply independently fitted tone curves to candidates. The faint survival measurement requires at least 50% of both the original peak and aperture flux. This is a reporting protocol, not a calibrated universal quality threshold or a claim of Lightroom equivalence. Clipping at one is reported; HDR inputs above one need a declared scene-white convention.

```sh
python rapidroom/validation/denoise/test_star_field.py
python rapidroom/validation/denoise/star_field.py \
  --original /local/original-linear.npy \
  --candidate /local/denoised-linear.npy \
  --sites /local/reference-sites.json --out /local/aggregate.json \
  --min-faint-survival 0.75 --min-median-peak-ratio 0.5 \
  --min-median-flux-ratio 0.5 --max-median-spread-ratio 1.5 \
  --max-median-colour-delta 0.05
```

Every acceptance threshold is optional and explicit; the example values are illustrative, not universal defaults. When any threshold is present, the aggregate includes an acceptance result and the command exits 1 for a failed or unmeasured requirement. Input errors exit 2. Choose thresholds from an independent reference and keep them fixed between before/after candidates.

Sites are JSON objects with integer `x`, integer `y` and boolean `faint`, supplied as a list. Each site needs a complete six-pixel surrounding sky aperture. Measurements use a four-pixel core and a five-to-six-pixel sky annulus. Crowded fields, wide/trailed stars and gradients require a more appropriate photometry protocol; inspect their apertures before using these figures. Site coordinates, image paths and image content are omitted from the aggregate report. Keep real photos and reference sites local.

Before human approval, compare original, previous and proposed denoised results using the same development settings and crop. Include seam profiles on flat fields, crushed-shadow/LED colour crops and full-frame/100% panels, and run fixed-site photometry on real night skies with an independent source/reference star set. Report missing references plainly. The AutoShade model-specific Anscombe and 71% luminance return recipe remains deferred; see `rapidroom/DENOISE.md`.
