# Parked again 2026-10-31 (Codex limited to one agent)

Head 039d2513 (review fixes; Rust changes in 60354038), main c49d7d66 merged.
Done since the first park:

- vono's two blocking findings fixed (Highlights detail keeps v0 reach; preset_converter sets the version only with tone sliders), plus items 3 and 6.
- Native library suite 748 passed, 0 failed, 23 ignored. Frontend checks pass. Ramp gate 16/16.
- Both full60 sets rendered against the shared exact-main control (`~/code/.cache/control-main-c49d7d66`, receipt with sha256) and compared with `samples/tone-488/compare-full60.py` (summary: `evidence/full60-summary-039d2513.json`):
  - law-0 proof (legacy recipes): 60/60 pixel-identical.
  - stock: 53 identical, 7 changed, all expected (calibrated camera baseline): Sony A7C II x5 +0.872 EV, Nikon Zf +0.816 EV, Canon R6 II +0.270 EV. No unexpected changes; same environment.
    Left: before/after panels for the maintainer (make-panels.py) from the stock set, the J40 byte checks against the rebuilt control, hosted CI on the head, an independent re-review (vono) of 60354038..039d2513, then the maintainer's render approval.
    Local renders live in ~/code/.cache/rani-488-tone/samples/tone-488/ (not committed).

# Historical parking record

Resumed on 2026-10-31. The following records the earlier parked state; see the
neighboring README for current validation evidence.

Parked at the maintainer's request before a trip. PR #495 remains draft and is
not rendering-ready. No further builds or fixes were started during wrap-up.
The implementation is commit `4d034da1`; the parking commit adds evidence only.

Completed:

- Native all-feature check and Clippy passed earlier in implementation; they were
  not rerun after the final import/table edits. The executable build succeeded
  with those edits included.
- Library tests: **740 passed, 4 failed, 23 ignored**. See `library-test-failures.txt`.
- Existing frontend tests: 108 passed, plus three throwaway version/preset probes.
  TypeScript, Prettier, schema generation, runtime i18n and change records passed.
  ESLint has ten existing `any` errors in `adjustments.ts`; i18n extraction has
  existing unrelated catalog drift.
- Real version-1 J40 grey-ramp renders passed all **16/16** approved pixel/RMS
  gates. `real-render-fit.txt` records measured ratios against the model.
- Candidate camera search (`candidate-calibration-search.json`) found R6 II
  +0.270 EV (mean gap -0.0026 luma), Zf +0.816 EV (+0.0202), A7C II +1.222 EV
  (+0.0137). Source currently retains the preliminary +0.270/+0.812/+1.220 EV
  entries; final refined values have **not** been applied or verified as defaults.

Required on resume, before rendering approval:

1. Address the independent review on #495 and coordinating agent's directions:
   retain the legacy Highlights detail blend while leaving the fitted flat law
   unblended; only attach the imported version when tone controls are present;
   make editor/library Reset agree; resolve float-encoded versions and MCP bounds.
2. Fix the four library failures: two rendered-denoise eligibility failures after
   adding the default key, the real-XMP test's `100.0` versus integer assertion,
   and the partial colour-only import unexpectedly gaining a tone version.
3. Apply/validate the final camera calibration; rerun applicable checks after fixes.
4. Render the three natural J40 photos, the version-absent J40 set, the legacy
   matched full60 (explicit empty neutral), and the stock fresh-default full60.
   Require legacy 60/60 exact; list every intended stock camera-baseline change.
   Neither full60 set has been run for this PR.
5. Produce before/after panels, including a law-0 camera-compensation edit and
   the Shadows +100 toe/desaturation behavior. No panels or human UI acceptance
   checks are complete. Obtain the maintainer's visual review.

The existing A7R V +0.35 EV stays uncalibrated. RX100 VII needs a future J40-style
export. A CLI preset-library migration can be handled in a follow-up.

Build targets, render outputs and installed frontend dependencies were removed
for laptop cleanup. Source worktrees and the pushed branch are retained. Small
local helper scripts are archived in the clone's Git metadata for resumption.
