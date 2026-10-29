# Print phase 1: ICC export and soft proof

This implements the scoped phase of #122 described in the issue's implementation design. The [original design](README.md) remains the plan for later work. Both controls start **off**; current sRGB rendering/export stays the default.

## Chosen-profile export

In Export → Output colour profile, choose a local `.icc`/`.icm` file, then explicitly enable conversion. Relative colorimetric is the default intent, black point compensation defaults on, and perceptual is optional. Profiles without separate perceptual tables may use colorimetric behaviour under LittleCMS.

JPEG, PNG and TIFF accept RGB output/display/colour-space profiles. Final pixels are converted from the existing sRGB render after geometry, resizing, border, padding, watermark and output sharpening. A selected profile requests 16-bit rendering before format quantization; PNG preserves that precision and alpha. The exact profile bytes used for conversion are embedded, including when Keep Metadata is off and when TIFF resolution tags are requested. CMYK profiles and formats without supported profile embedding are refused for export. No selected profile keeps the existing path.

This selection is local to the Export panel session, not saved in named recipes; choosing another recipe turns it off. It is independent of the proof view. RapidRoom currently treats non-RAW inputs as sRGB when opening them; review a non-sRGB exported file in an ICC-aware viewer rather than reopening it here for a colour comparison. Input ICC conversion is outside this phase.

Headless examples (paths are illustrative):

```sh
rapidroom export input.dng --output converted.tiff --format tiff --tiff-bit-depth 16 --icc-profile output-rgb.icc --icc-intent relative
rapidroom export input.dng --output converted.png --format png --icc-profile output-rgb.icc --icc-intent perceptual --no-icc-bpc
```

## Linux soft-proof view

Use the printer icon in the editor toolbar. Choose an RGB or CMYK **output-class** printer/paper ICC, then enable Preview printer/paper colours. The CPU transform is sRGB → printer/paper → sRGB through LittleCMS proofing, with relative/perceptual intent and optional BPC. The display is assumed to be sRGB. There is no calibrated-monitor or physical-print accuracy claim.

This is session-only view state: it never enters photo adjustments, sidecars, Undo, export recipes or exported pixels. It is carried with each preview request, including crop previews. A late result for a different proof option is discarded. Proof previews are excluded from the normal photo-preview cache. The clipping-colour overlay is suppressed while proofing so it is not treated as photo colour; analytics continue to describe the edit before proofing.

Profiles are local read-only regular files limited to 16 MiB. Invalid/missing/ineligible profiles produce an error; no silent sRGB fallback or unconverted profile tagging. No profiles are downloaded. The native picker and proof UI target Linux; other platforms remain untested.

## Remaining scope and acceptance

Paper-white/ink simulation, gamut warnings, monitor profiles, wide-gamut processing, CMYK encoding, MCP and book support remain follow-ups. These opt-in transforms change output and require independent review, current technical gates, LittleCMS reference comparisons, a synthetic before/after panel and Josh's explicit approval before merge. A panel look does not establish real printer calibration.

Execution evidence is recorded in the PR. This document does not itself claim the pending native, full60, reference-comparison or human gates passed.
