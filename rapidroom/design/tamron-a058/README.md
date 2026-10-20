# Tamron A058 Sony E alias and calibration contribution note

Checked against RapidRoom main `e9c28f887854f5a18f5a99143589aec13287957a` on 2026-10-14. This records the proposed alias for #184. Automatic correction on Sony E remains blocked until independently validated Sony calibration is available. This note does not change matching or coefficients. No private files or image content belong in a public submission.

## Identity and proposed alias

Observed EXIF `LensModel`: `E 35-150mm F2.0-F2.8 A058`.

Candidate maker: `Tamron`. Candidate canonical profile: `Tamron E 35-150mm f/2-2.8 2.0-2.8 Di III VXD A058Z`, in `src-tauri/lensfun_db/mil-tamron.xml`. The existing display name is `Tamron 35-150mm F/2-2.8 Di III VXD`. This profile lists both `Sony E` and `Nikon Z` mounts. A043 is a different DSLR lens and is excluded.

Tamron lists A058 for Sony E and Nikon Z, with 21 elements in 15 groups: [official specifications](https://www.tamron.com/global/consumer/lenses/a058/spec.html). This establishes product identity, not equivalent camera-dependent shading.

## Why a runtime alias alone is insufficient

The profile's calibration comment explicitly says `Taken with Nikon Z8`; it contains 7 distortion and 92 vignetting entries and no TCA entries. The current matcher considers canonical and English names only, and the exact Sony EXIF spelling fails both. Adding it as another name without an eligibility guard would immediately apply these Nikon-derived coefficients on Sony when Auto is enabled. The existing two-photo post-tone brightness comparison is not an independent calibration set.

Do not remove the Nikon attribution, change coefficients, create a nominal Sony calibration by copying them, or globally loosen fuzzy matching. No Sony-calibrated profile is claimed here. The upstream database may evolve; the bundled database and its hash must be checked again before implementation.

## Narrow implementation after validation

1. Recognize only the complete Sony EXIF alias above after outer whitespace/quote trimming, with case-insensitive comparison. Do not use a bare `A058` substring or focal-length-only heuristic.
2. Resolve the exact Tamron canonical target, requiring the camera's database mount to be `Sony E` and the candidate profile to support `Sony E`. Camera make must not stand in for lens maker.
3. Return no automatic match for a missing/unknown camera mount, an incompatible mount, an absent profile, or more than one eligible profile. Ambiguity must not fall through to a looser A058 fuzzy match.
4. Keep Nikon exact-name behavior and existing Sony matching unchanged. Do not map A043, partial/unknown model codes, or another 35–150mm profile.
5. Gate automatic Sony eligibility on independently reviewed Sony distortion and vignetting evidence. If only identity is available, present a clearly marked manual candidate instead of silently enabling Auto; that requires a separate UI implementation decision.

## Calibration and acceptance plan

Use [Lensfun's calibration instructions](https://lensfun.github.io/calibration/) to collect Sony E RAW distortion targets over the supported zoom range and flat-field vignetting targets over focal lengths/apertures, with exposure, focus distance and in-camera correction state recorded. Keep originals and filenames private; obtain permission before external submission. Compare geometric straightness/framing and linear-light shading independently, including held-out captures. Record the body, firmware, lens and evidence coverage. Preserve unsupported TCA status.

Matcher regression cases: exact Sony alias; harmless outer quotes/space/case; Nikon canonical and display spelling; A043; unknown/partial A058; unknown/incompatible camera; duplicate eligible candidates; known existing Sony lens. Test both import/open Auto and reopened cached corrections. Run full60 against the blessed reference, plus before/after panels from the selected Sony files. A human panel look does not replace the calibration measurements.

No upstream submission, lens-database edit, matcher change, calibration claim, or reference update has been made by this draft. #184 remains open until the Sony automatic-correction requirement is supported or its scope is explicitly reduced.

## 2.3.0 identity-only implementation

The exact Sony EXIF spelling now resolves to the unique compatible A058 identity for a known Sony E camera. Unknown bodies, incompatible mounts, missing profiles and duplicate eligible profiles refuse the alias without fuzzy fallback. The database and Nikon Z8 attribution remain unchanged.

Identity does not authorize those coefficients on Sony. New Auto/v1 recipes continue to use valid physical Sony ARW distortion tables, and the Nikon-derived A058 Lensfun calibration is explicitly ineligible. This also holds when a physical table is missing: Auto cannot silently switch to Nikon geometry or shading. Existing legacy Auto recipes retain their previous matching behavior, and an explicitly chosen manual profile retains its existing controls.

This is partial scope for #184. Camera-data vignetting and TCA still require independent validation; neither is enabled here. Real-app evidence and a full60 pixel comparison must bind the implementation before acceptance.
