# Selective denoise experiment

`denoise-selective` is an opt-in rendered-image prototype for issue #314. It blends an already prepared original/denoised pair through an existing saved AI Subject mask. It does not run a model, change the editor, alter either input, or save changes to the source photo. Quality and owner taste acceptance remain pending.

```sh
rapidroom denoise-selective --pair pair.json --output background.tiff --invert --amount 100
```

The saved Subject mask selects the subject by default. `--invert` selects its background; `--amount` sets how much of the denoised result appears there, from 0 to 100. Repeating with a new amount or mask snapshot reuses the same inference result. Every output must have a new TIFF filename in an existing directory.

## Preparing a pair

Use upright, uncropped, opaque 16-bit sRGB TIFFs with exactly matching pixel geometry. The original must be the actual developed view used for inference, not a separately developed RAW with different WB, tone, repairs or lens settings. Record the denoise job and its prepared-input pixel hash when creating the pair. For the rendered NIND queue, compare its prepared-view SHA-256 trace with the normal export's decoded RGB16 pixels. Use the same source photo for the mask. The command verifies the declared checksums, dimensions and original prepared-input hash; it cannot infer whether arbitrarily chosen pictures depict the same scene or development.

Save a copy of the original photo's adjustment/sidecar JSON containing the existing Subject mask. The mask must be in the prepared view's pixel coordinates, with a full-resolution saved AI Subject raster. The prototype accepts either an adjustments object or an object containing `adjustments`. It uses the app's existing mask renderer, including mask inversion, opacity and feathering. Cropped, rotated, flipped or automatically lens-corrected source masks require reprojection first and are refused. Color/luminance range refinements also require a working image and are outside this prototype.

Create this manifest alongside the files (paths can also be absolute):

```json
{
  "version": 1,
  "width": 4608,
  "height": 3072,
  "colorSpace": "srgb",
  "maskCoordinateSpace": "prepared-view",
  "original": { "path": "prepared.tiff", "sha256": "FILE_SHA256" },
  "denoised": { "path": "denoised.tiff", "sha256": "FILE_SHA256" },
  "mask": { "path": "subject-adjustments.json", "sha256": "FILE_SHA256" },
  "maskId": "SAVED_MASK_ID",
  "preparedRgb16Sha256": "PIXEL_SHA256"
}
```

File hashes are SHA-256 of the exact file bytes. The prepared pixel hash covers row-major RGB16 samples in little-endian order, excluding alpha and file headers. Inputs above 50 megapixels or 512 MiB per file are refused to bound this experiment. The output preserves 16-bit RGB precision and assumes sRGB encoding; preview it with compatible Basic/non-RAW processing settings. It does not create a persistent RAW layer or copy source edits into the TIFF.

## What the result means

Amount 0 preserves the original pixels. Fully selected pixels at amount 100 use the denoised pixels exactly; unselected pixels retain the original exactly. Partial coverage blends in linear light using the sRGB transfer function. This avoids a dark transition from averaging encoded values. It does not establish that a particular mask or denoiser produces natural noise texture, smooth backgrounds or clean subject edges.

The command writes a JSON receipt to stdout with pair/input/output fingerprints, mask ID, amount, inversion and zero inference runs. Save that receipt with panel inputs. Changed file bytes require a new manifest, preventing a same-path cached pair from silently changing. Existing outputs are never overwritten. No image, mask, model or path is sent over the network, and the command starts before Tauri or model initialization.

## Acceptance and later UI

The prototype needs a real saved model-produced Subject mask and native CLI checks for amount 0/100, partial transitions, background inversion, changed pair/mask refusal and repeat reuse. Inspect full views and true1:1 subject/background/edge crops. Compare with both original and full-frame denoise; keep sharpened and unsharpened model chains identifiable. Synthetic mask/black-white TIFF tests protect publication and blend arithmetic but do not establish model quality.

Prepare owner-selected night-photo panel inputs locally for round 4, with full desktop views at least 3840px on the long edge and natural-size crops at least 1024px. Keep private photos local and wait for the later owner taste call. Do not ask for that review merely because technical checks pass.

A persistent editor layer still needs pair lifetime/ownership, asset relocation and recovery, undo, source-edit invalidation, reprojected masks, and bounded full-resolution export. Those are follow-up design decisions after the paired-view experiment, rather than promises made by this command.
