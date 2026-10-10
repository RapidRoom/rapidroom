# Current upstream integration and Sony WB review (#175 / PR179)

Tested application `6b3ac875a35d00d83b75b97a28c8ac486e9cd699`, current upstream `eb3556bc`:73 incoming
commits. Normal merges retain authors/history. These artifacts contain CC0
images and sanitized source/measurement evidence. Personal photos, raw files,
credentials and local paths are excluded. **Josh's visual approval remains
required before any local/CI reference update or merge.**

## Result and attribution

Default and terminal,mcp locked production releases each export60/60; both
configurations match60/60 decoded-pixel exactly. Compared with frozen int7,
20/60 are exact; all20 neutral renders remain exact. The two
busy recipes exercise upstream Kelvin/WB, guided tone, detail and Whites.
Maximum mean absolute difference is3468.217 16-bit codes,
max absolute difference47959, maximum mean dE00
3.450; maxima can be from different cases.

Relative to pre-fix4bea590b,58/60 are exact: **only the Sony A7CR-S two busy
recipes change**. Independently, a release control at06c4eae3 (Sony correction,
before the latest12 upstream commits) matches current upstream60/60 exactly.
Thus the added twelve commits make no pixel change for these corpus recipes.
Fog/relight default disabled; E-M1X is outside the corpus. Enabled fog/relight
output and downloaded normal/depth model operations are not tested here.

The [historical seven-control attribution](../rendering-attribution.json) was
measured at4bea590b and reconstructs old-relative-WB output exactly60/60 against
int7, with zero unexplained residual. It is historical evidence for the earlier
61 upstream commits. The Sony fix and additional twelve commits have the
separate current-source measurements above; historical patches do not claim to
reconstruct this new source. Incremental controls are not additive quality scores.

## Sony cause, minimal fix and plain upstream reproduction

The pinned decoder marks Sony lossless M/S as cpp3 LinearRaw and returns unity
WB gains because camera WB is already applied. Matrix inversion of those gains
invents2358.327K /−194.980 tint. The first small relative edit invokes the
original Kelvin range clamp and jumps tint to−150, producing the cast. Real CFA
sensor coefficients remain valid (full A7CR metadata gives6021.314K/+19.012).
The narrow guard uses reference WB only for Sony cpp3 LinearRaw with unity gains.
Displayed Kelvin/tint for those files now describes this reference basis;
original camera illuminant would require a separate decoder metadata API.

Unmodified upstream8387fc12 reproduces both casts on the identical CC0 source
and recipes. Its older crop defect creates5085×3390 exports with green padding;
RapidRoom retains its crop fix. These triptyches establish shared color failure,
not exact geometry. Kelvin9ba20c02 caused most of the historical change; A/D65
11e20e77 reduces it, the shared-gain refactor contributes very little, and the
later **LMS floor73bc73f4 changes zero pixels for the ordinary recipes**. That
floor's extreme2000K/+150 probe is a distinct operation and remains separately
recorded in the parent evidence. No upstream post/comment was sent.

72 additional Sony exports cover full-size A7CII uncompressed/compressed and
A7RV uncompressed/lossless/compressed at as-shot and3200/6504/9000K. All20
full-size before/after pairs (40exports) are exact. A7CR-S and A7RV M/S get the
same absolute probes; A7RV M/S also get the affected relative recipes. Source
licences/hashes, recipes and measurements are in sony-probes.json.

## Validation

Formatting and strict locked all-target/all-feature Clippy pass. Rust:
326 default /366
combined library tests pass (8 ignored each). Frontend:197
tests pass; generated schema/status pass. There are46 inherited type diagnostics
and943 inherited frontend-src lint messages, with zero
new diagnostics against both parents under the same src scope. Whole-tree lint
also records one literal-string warning in an unchanged validation harness;
frontend-extra-diagnostics.json retains its identical main/current git objects.
Runtime i18n still fails with212 issues, exactly matching current upstream;
this failure remains disclosed.

The current native minimum/network run and actual Claude/Codex compact-MCP run
pass thumbnail/preview, Exposure/Contrast, exact Undo, Compact persistence,
full-size JPEG, three masks, nested edits, statistics/comparison and history.
Fresh default startup/edit/export traffic observes only the retained updater
host and no Clerk/cloud/font hosts or sampled direct TCP bypass. Separate
Chromium CSP default and saved-Cloud cases pass with unavailable auth backend
stubs; these are not live Cloud sign-in or native CSP rewriting tests. Non-Linux
platforms and physical pointer input remain untested.

**Codex Sora inspected all26 individual panels and every Sony WB row;
[observations and hashes](agent-inspection.json) record that inspection. Josh
has not approved them.** Numeric results use original full-resolution16-bit TIFFs;
previews use identical8-bit conversion and downsampling. References, manifest,
approval log and tolerances remain unchanged. Hosted regression on this exact application head still needs
Josh-approved reference reconciliation:18/18 deterministic,6 neutral
exact/12 edited mismatches. All other hosted checks pass; see [hosted CI](hosted-ci.json). No CI policy/tolerance bypass was introduced. The source review identified and
corrected three integration blockers: equal-length map/patch cache identity,
native MCP fog/relight support, and image-bound async map requests. Their focused
regressions and independent re-review remain separate from the full gates above.

## Baseline to current

![Frozen int7 and current candidate](current-busy-agx-lut-effects-apple-iphone12pro-proraw.png)

![Frozen int7 and current candidate](current-busy-agx-lut-effects-om-om1-20mp.png)

![Frozen int7 and current candidate](current-busy-agx-lut-effects-sony-a7c2-15mp-uncompressed.png)

![Frozen int7 and current candidate](current-busy-agx-lut-effects-sony-a7cr-18mp-lossless-s.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-apple-iphone12pro-proraw.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-canon-r50-24mp-craw.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-fuji-xt4-26mp-lossless.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-om-om1-20mp.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-sony-a7c2-15mp-uncompressed.png)

![Frozen int7 and current candidate](current-busy-tone-color-detail-sony-a7cr-18mp-lossless-s.png)

## Sony upstream and correction

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7c2-33mp-compressed-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7c2-33mp-uncompressed-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7cr-18mp-lossless-s-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7cr-s-busy-agx-lut-effects-int7-fix.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7cr-s-busy-agx-lut-effects-upstream-fix.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7cr-s-busy-tone-color-detail-int7-fix.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7cr-s-busy-tone-color-detail-upstream-fix.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6231-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6232-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6233-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6233-busy-agx-lut-effects.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6233-busy-tone-color-detail.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6234-as-shot-absolute.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6234-busy-agx-lut-effects.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6234-busy-tone-color-detail.png)

![Sony as-shot, absolute Kelvin or relative WB comparison](sony-a7rv-pixls-6235-as-shot-absolute.png)

