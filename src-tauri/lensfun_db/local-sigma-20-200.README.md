# Sigma 20–200 mm Sony E: distortion and vignetting only

Adapted from mpasson/RapidRAW commit
`e8811879180122e571ad229a59997aed648dca11`, measured and fitted by
Marco Passoni from Sony correction metadata in 207 ILCE-7CM2 ARW frames.
The data is CC-BY-SA 3.0; see
`rapidroom/LICENSES/sigma-20-200-CC-BY-SA-3.0.txt`. The unchanged donor XML
and original provenance note are in
`rapidroom/lens-profiles/sigma-20-200/source/`.

The active profile retains all 51 corrected poly5 distortion anchors and
99 PA-vignetting entries. Its coefficients use the donor's Lensfun convention:
for a 3:2 full-frame sensor, k1 = k1_sony / 3.25 and
k2 = k2_sony / 3.25². RapidRoom's existing evaluator supplies the inverse
radius conversion once. Existing Amount and auto-crop behavior are unchanged.

**Polynomial TCA is unsupported.** All 51 donor TCA entries are excluded
from the active database; the model selector reports “TCA unsupported”.
RapidRoom currently supports constant channel scales but not the donor's
nonzero br/bb terms. The original data is retained intact for issue #267,
which must coordinate with subpixel CA issue #18. No constant-term partial
approximation is applied.

**Unvalidated on real files in RapidRoom.** Synthetic coordinate/illumination
checks do not establish the correction's quality on the actual lens.
The donor reports comparison with native Lensfun, but that is separate from
our own checks. Correction stays opt-in through the existing Lens controls;
unknown lenses and old saved coefficients retain their existing behavior.
No Adobe LCP file, donor native-wrapper package, model weights or photos
are imported.
