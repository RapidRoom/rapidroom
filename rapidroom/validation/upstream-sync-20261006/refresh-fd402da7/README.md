# PR179 migrated-main refresh: measured source 52366ea0

Tested application `52366ea01153efac49ffcf0abef673e6ee4d4f6b` integrates migrated
main `fd402da7d01dfb2f45656b61dc82a6cdcbb75bba` and upstream `74597d06`.
This publication changes only evidence. Source/dependency/workflow tree objects and
pinned binary SHA256s are in [source-and-checks.json](source-and-checks.json).
No reference, tolerance, CI policy, recording, approval or merge changed.

## Current result

- Strict locked all-target/all-feature Clippy, fmt, generated schema (219 keys / 84 controls),
  status (108 records) pass. Rust **398** default / **449** terminal,mcp pass (8 ignored each);
  Vitest **381** pass. TypeScript has four inherited main errors and zero new diagnostics:
  full multiline comparison normalizes only source locations and expanded i18n union
  display count/tail. It is not a zero-error TypeScript pass.
- Locked production releases produce 120 fresh exports: default 60 and combined 60
  match each other and previously measured dcff 60 **pixel for pixel and dimension for dimension**.
  [Default](render-default.json) and [combined](render-combined.json) manifests retain file/pixel hashes; generated
  TIFFs were deleted only after audit/comparison. All 180 corpus/reference guards unchanged.
- Actual native masks/Claude second-mask/GUI Undo and three Fuji/Sony native controls
  pass, including trusted GTK wheel, repeated Exposure preview/histogram updates,
  exact reset/Undo and full-size JPEG export. Default and saved Cloud CSP browser checks
  pass with unavailable-auth stubs. Live sign-in and fresh packet tracing
  are not covered; prior native network measurement remains historical.
- Refused curve insertion now stays idle; actual component refusal → accepted drag → mouseup
  and related focused tests pass **29/29**. [Committed input binding](focused-test-binding.json).
  [Independent source approval](https://github.com/RapidRoom/rapidroom/pull/179#issuecomment-6101744763)
  is bound to 52366ea0. Final publication/evidence review is requested separately.

## Native panels and capture-order diagnosis

All 115 successful refreshed native captures and all 7 final CC0 photo ROI panels were
visually inspected by the agent. Full UI/path screenshots remain private.

The original XH2 100% screenshot was visibly coarse: its helper captured a stable
low-resolution frame BEFORE checking decoded dimensions. Supplementary actual-app
probes now wait for the single visible full-size SVG resource BEFORE native capture,
then confirm the same source/dimensions AFTER. The observed 1024→7728 XH2 transition
explains the transient frame. **All three settled photo ROIs exactly match historical
dcff pixels**; XT4/Sony original photo ROIs were already exact. No application fix or
synthetic image was used. [Measured before/after receipts](settled100.json),
[helper changes/hashes](native-harness-provenance.json), [inspection](inspection.json).

| Fixture   | Settled actual 100%                                        | Exact reset                                           |
| --------- | ---------------------------------------------------------- | ----------------------------------------------------- |
| Fuji XH2  | [100%](native-fuji-xh2-40mp-compressed-100-percent.png)    | [Reset](native-fuji-xh2-40mp-compressed-reset.png)    |
| Fuji XT4  | [100%](native-fuji-xt4-26mp-lossless-100-percent.png)      | [Reset](native-fuji-xt4-26mp-lossless-reset.png)      |
| Sony 15mp | [100%](native-sony-a7c2-15mp-uncompressed-100-percent.png) | [Reset](native-sony-a7c2-15mp-uncompressed-reset.png) |

[Actual MCP mask additions and GUI Undo](native-mcp-masks-and-undo.png).
Left panels remain historical dcff captures; right panels are source 523.
The full native harness also needed read-only ContinueSession/reload readiness
repairs, with failed output retained and original successful assertions preserved.
Per-case helper identity and supplemental probe failure are disclosed in provenance.
No claim that successful retry exercised a WebDriver timeout: reload retry count was 0.

## Historical colour and exposure evidence remains historical

[Earlier 43 inspected panels and Sony/settings/main controls](https://github.com/RapidRoom/rapidroom/blob/720dd3572f33ceea384673ba679b4a9fa9c9d77e/rapidroom/validation/upstream-sync-20261006/current-74597d06/README.md)
were measured at dcff2861 and published at 720dd357. Current 60 exactly match those renders,
so there is no changed full60 comparison panel to regenerate or relabel. The prior 36 fresh Sony
comparisons against prior 72 receipts, 80 fresh/legacy settings probes and pinned
main f30 controls were **not rerun at 523**. The Sony WB implementation blob is unchanged
from 720dd357; plain upstream 8387 reproduction remains historical, not fresh upstream.
Linux controls do not establish Zorin Flatpak / Windows AMD parity. No Canon 5D II CR2 or
confirmed Nikon HE* fixture. No physical pointer / live Cloud / new model quality evaluation;
these panels do not establish Lightroom colour calibration.

## Frozen reference and approval hold

[Actual compiled-head hosted checks](hosted-checks.json): eight succeed, refresh skips,
regression fails solely against the frozen reference, with **18/18** repeatability,
6 neutral exact / 12 edited mismatches. Current local 60 remains **19/60 exact against int7**;
this is a failing reference gate. All reference files/policy/tolerances remain frozen.
Evidence-only head checks are separate and must not inherit 523 CI identity.

Josh must visually approve before reference recording/re-bless (using his words),
current-main integration/affected gates, independent review and fresh exact-head
hosted checks before merge. This report is agent evidence, **not human approval**.
