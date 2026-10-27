# Current upstream sync and Sony WB review — PR179

Tested application `dcff286122e8f384044dd44fc3dae2bbad3e30fc`, upstream `74597d066685283588502edbecd747bfe80404cf` (85 incoming commits;
12 after the previous measured upstream `eb3556bc`). Control main was pinned at
`f30d0e87cc8e2165c52582a9929a713ee07ac3d7` when the refresh started. Normal merges retain authors.
These are measured CC0 exports/captures. Human approval remains required before
any reference recording, local re-bless or merge. No reference or CI policy changed. Application and dependency trees remain those of the tested source; evidence-only commits do not relabel compiled binaries.

## Start with these panels

- [Canon R6II: new default highlight compensation](current-canon-r6m2-24mp-raw.png).
- [Sony A7CR-S: current renderer and relative WB](current-sony-a7cr-18mp-lossless-s.png).
- [Sony A7CII full-size: source WB and absolute Kelvin](sony-a7c2-33mp-uncompressed-as-shot-absolute.png).
- [Nikon Zf: legacy/fresh exposure preferences](baseline-nikon-zf-24mp-compressed.png).

## Rendering result and attribution

Default and terminal,mcp locked production releases each export 60/60 and match
60/60 decoded full-resolution pixel hashes and dimensions exactly. Compared with
frozen int7, **19/60 are exact**: 19 neutral cases exact, 40 edited recipes differ and
one neutral Canon case differs. Maximum mean absolute difference is
3667.306 (16-bit) codes; max absolute difference 47959;
maximum mean dE00 4.700. Maxima may come from different cases.

Compared with the previous measured Sony-fixed application `6b3ac875`, **59/60 are
exact**. The only new change is Canon R6 II neutral: fresh eligibility enables the
camera's **+0.5 EV highlight-preservation compensation** by default. The actual
HL-only probe exactly reproduces that neutral pixel hash; switching compensation
off exactly reconstructs the prior hash. There is no unexplained latest-twelve
residual in the ordinary 60 recipes. `exposure-attribution.json` records this.

Baseline exposure defaults off; its optional amount is +0.7 EV. Highlight compensation
defaults on. New/reset edits carry `baselineExposure:true`; old sidecars missing the
marker remain ineligible. 80 fresh/legacy exports span Nikon Zf/Zfc, Fuji XH2/XT4 and
Canon R6 II, current vs pinned main, with both preferences off/on. All legacy cases
are preference-invariant; all fresh/all-off cases exactly match legacy/main.
The four Nikon/Fuji fixtures have zero HL metadata: HL-only makes no change;
baseline-on visibly raises eligible fresh images. These Nikon fixtures are **not
confirmed HE*** and cannot disprove the original AppleSilicon HE* report.

The [historical seven-control attribution](../rendering-attribution.json), measured
at `4bea590b`, reconstructs old-relative-WB output exactly 60/60 against int7 with zero
unexplained residual. It remains historical. The separate Sony correction and the
latest upstream exposure addition have their own measured comparisons above.

## Sony WB

All 36 newly exported as-shot/3200/6504/9000K and reduced-M/S relative cases match
retained fixed-side measurements from prior 72 exports **exactly**. This is 36 fresh
exports, not 72 new ones. Full-size A7CII CFA and A7RV uncompressed/lossless/compressed
remain stable. The Sony cpp=3/LinearRaw unity-gain guard survived upstream's new
AsShot metadata structure; actual decoded RGB gains are not a sensor neutral.

[Plain upstream 8387 reproduction and the original fix panels](https://github.com/RapidRoom/rapidroom/blob/913f965b0a74704487c6f196c359c269725ce247/rapidroom/validation/upstream-sync-20261006/current-eb3556bc/README.md)
remain historical measured evidence; no new plain-upstream run is claimed here.
Kelvin/interpolation caused the ordinary relative-WB change; the later LMS floor
changed zero ordinary-recipe pixels and its extreme 2000 K/+150 probe is separate.
The [historical upstream Sony guard](https://github.com/RapidRoom/rapidroom/blob/913f965b0a74704487c6f196c359c269725ce247/rapidroom/validation/upstream-sync-20261006/current-eb3556bc/upstream-guard.diff) describes the
minimal behavior; current source adds baseline metadata without changing the guard.

## Native and privacy checks

Strict locked all-target/all-feature Clippy and formatting pass. Rust 378 default /
429 combined tests pass (8 ignored each); frontend 373 tests/44 files pass. Generated
schema/status pass. Default/combined/native/main control releases are pinned.
Native minimum and actual installed-client single-mask tests pass; 19 captures
inspected privately. All 160 captures from the six candidate/main controls were also inspected privately. Six candidate/main native controls cover Fuji XH2/XT4 and Sony,
real 100% preview captures/decoded surfaces, trusted GTK wheel behavior, four
Exposure changes with full-preview and histogram updates, exact reset/Undo and
full-size JPEG export. Private fixture/engine guards remain true. These controls
use public UI gestures and recorded pixels, not React-store mutation or fake images.

No confirmed Nikon HE* or Canon 5D II CR2 fixture is present. Linux Weston/WebKit
cannot establish the original Zorin/Flatpak or Windows/AMD issues. Physical pointer
input and live cloud sign-in are not tested. Fresh default native startup/edit/export
observed only the normal updater host `api.github.com`, no Clerk/cloud/font hosts
or sampled direct TCP bypass. Browser CSP default and saved-Cloud cases pass with
unavailable-auth backend stubs, independently of native network observations.

Hosted exact dcff source checks pass except the frozen-reference regression:
**18/18 repeatability, 6 neutral exact/12 edited mismatches**. [Hosted receipt](hosted-checks.json) and the [regression job](https://github.com/RapidRoom/rapidroom/actions/runs/38355977210/job/115158847622) record the human/reference hold. After the maintainer approves the visual
changes, reference reconciliation and fresh exact-head hosted checks are required.

All 43 current panels were inspected by the agent. This does not claim the maintainer's approval.
Numeric comparisons use original full-resolution 16-bit exports. Panel conversion
and downsampling are identical on each side. Public native images are photo ROI
crops only. Full UI/captures, operational paths, credentials and personal photos
remain private. Measured generated TIFFs were deleted after comparison/panels;
corpus, reference, manifest, tolerance and approval log remain unchanged.

## All panels

### baseline-canon-r6m2-24mp-raw.png

![Measured CC0 comparison](baseline-canon-r6m2-24mp-raw.png)

### baseline-fuji-xh2-40mp-compressed.png

![Measured CC0 comparison](baseline-fuji-xh2-40mp-compressed.png)

### baseline-fuji-xt4-26mp-lossless.png

![Measured CC0 comparison](baseline-fuji-xt4-26mp-lossless.png)

### baseline-nikon-zf-24mp-compressed.png

![Measured CC0 comparison](baseline-nikon-zf-24mp-compressed.png)

### baseline-nikon-zfc-21mp-lossless.png

![Measured CC0 comparison](baseline-nikon-zfc-21mp-lossless.png)

### current-apple-iphone12pro-proraw.png

![Measured CC0 comparison](current-apple-iphone12pro-proraw.png)

### current-canon-r50-24mp-craw.png

![Measured CC0 comparison](current-canon-r50-24mp-craw.png)

### current-canon-r6m2-24mp-raw.png

![Measured CC0 comparison](current-canon-r6m2-24mp-raw.png)

### current-fuji-xh2-40mp-compressed.png

![Measured CC0 comparison](current-fuji-xh2-40mp-compressed.png)

### current-fuji-xt4-26mp-lossless.png

![Measured CC0 comparison](current-fuji-xt4-26mp-lossless.png)

### current-google-pixel7pro-dng.png

![Measured CC0 comparison](current-google-pixel7pro-dng.png)

### current-nikon-zf-24mp-compressed.png

![Measured CC0 comparison](current-nikon-zf-24mp-compressed.png)

### current-nikon-zfc-21mp-lossless.png

![Measured CC0 comparison](current-nikon-zfc-21mp-lossless.png)

### current-om-om1-20mp.png

![Measured CC0 comparison](current-om-om1-20mp.png)

### current-panasonic-s5m2-24mp.png

![Measured CC0 comparison](current-panasonic-s5m2-24mp.png)

### current-ricoh-griiix-24mp-dng.png

![Measured CC0 comparison](current-ricoh-griiix-24mp-dng.png)

### current-sony-a7c2-15mp-compressed.png

![Measured CC0 comparison](current-sony-a7c2-15mp-compressed.png)

### current-sony-a7c2-15mp-uncompressed.png

![Measured CC0 comparison](current-sony-a7c2-15mp-uncompressed.png)

### current-sony-a7c2-33mp-compressed.png

![Measured CC0 comparison](current-sony-a7c2-33mp-compressed.png)

### current-sony-a7c2-33mp-uncompressed.png

![Measured CC0 comparison](current-sony-a7c2-33mp-uncompressed.png)

### current-sony-a7c2-37mp-4x3.png

![Measured CC0 comparison](current-sony-a7c2-37mp-4x3.png)

### current-sony-a7cr-18mp-lossless-s.png

![Measured CC0 comparison](current-sony-a7cr-18mp-lossless-s.png)

### current-sony-a7cr-26mp-uncompressed.png

![Measured CC0 comparison](current-sony-a7cr-26mp-uncompressed.png)

### current-sony-a7cr-61mp-compressed.png

![Measured CC0 comparison](current-sony-a7cr-61mp-compressed.png)

### current-sony-a7cr-65mp-lossless.png

![Measured CC0 comparison](current-sony-a7cr-65mp-lossless.png)

### native-fuji-xh2-40mp-compressed-100-percent.png

![Measured CC0 comparison](native-fuji-xh2-40mp-compressed-100-percent.png)

### native-fuji-xh2-40mp-compressed-reset.png

![Measured CC0 comparison](native-fuji-xh2-40mp-compressed-reset.png)

### native-fuji-xt4-26mp-lossless-100-percent.png

![Measured CC0 comparison](native-fuji-xt4-26mp-lossless-100-percent.png)

### native-fuji-xt4-26mp-lossless-reset.png

![Measured CC0 comparison](native-fuji-xt4-26mp-lossless-reset.png)

### native-sony-a7c2-15mp-uncompressed-100-percent.png

![Measured CC0 comparison](native-sony-a7c2-15mp-uncompressed-100-percent.png)

### native-sony-a7c2-15mp-uncompressed-reset.png

![Measured CC0 comparison](native-sony-a7c2-15mp-uncompressed-reset.png)

### sony-a7c2-33mp-compressed-as-shot-absolute.png

![Measured CC0 comparison](sony-a7c2-33mp-compressed-as-shot-absolute.png)

### sony-a7c2-33mp-uncompressed-as-shot-absolute.png

![Measured CC0 comparison](sony-a7c2-33mp-uncompressed-as-shot-absolute.png)

### sony-a7cr-18mp-lossless-s-as-shot-absolute.png

![Measured CC0 comparison](sony-a7cr-18mp-lossless-s-as-shot-absolute.png)

### sony-a7rv-pixls-6231-as-shot-absolute.png

![Measured CC0 comparison](sony-a7rv-pixls-6231-as-shot-absolute.png)

### sony-a7rv-pixls-6232-as-shot-absolute.png

![Measured CC0 comparison](sony-a7rv-pixls-6232-as-shot-absolute.png)

### sony-a7rv-pixls-6233-as-shot-absolute.png

![Measured CC0 comparison](sony-a7rv-pixls-6233-as-shot-absolute.png)

### sony-a7rv-pixls-6233-busy-agx-lut-effects.png

![Measured CC0 comparison](sony-a7rv-pixls-6233-busy-agx-lut-effects.png)

### sony-a7rv-pixls-6233-busy-tone-color-detail.png

![Measured CC0 comparison](sony-a7rv-pixls-6233-busy-tone-color-detail.png)

### sony-a7rv-pixls-6234-as-shot-absolute.png

![Measured CC0 comparison](sony-a7rv-pixls-6234-as-shot-absolute.png)

### sony-a7rv-pixls-6234-busy-agx-lut-effects.png

![Measured CC0 comparison](sony-a7rv-pixls-6234-busy-agx-lut-effects.png)

### sony-a7rv-pixls-6234-busy-tone-color-detail.png

![Measured CC0 comparison](sony-a7rv-pixls-6234-busy-tone-color-detail.png)

### sony-a7rv-pixls-6235-as-shot-absolute.png

![Measured CC0 comparison](sony-a7rv-pixls-6235-as-shot-absolute.png)

