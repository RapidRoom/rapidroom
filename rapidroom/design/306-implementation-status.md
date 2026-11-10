# Saved History implementation status (parked)

Implementation for #306 and #498 follows the approved design in #475. The PR
is parked for shutdown and remains draft. See `306-PARKED.md` for the current
validation gap and next step. No checks are currently running.

Implemented: versioned bounded `.rrhistory` reads/writes and protected invalid or
newer files; authoritative sidecar-first/history-second transactions; stable step
IDs and current-state integrity; compaction retaining current edits and snapshots;
all companion paths including the separate batch-rename classifier, physical and
virtual copies, transfers, clearing and Immich exclusion; editor persistence;
dockable newest-first panel with actor/time, numeric columns, keyboard navigation,
branching and named snapshot create/rename/delete/restore; numeric before→after
labels; library paste/sync/reset/Style steps and atomic background-AI completion.

Reference checks cover actual develop controls from a sanitized existing ARW
sidecar, round trips, crash-between-writes recovery, protected bad/newer history,
branching, immutable snapshots, byte/count compaction, oversized mask payloads,
exact virtual-copy paths and companion operations. The native saved-History
scenario exercises real controls and a WebView restart without store access.

Validation so far: frontend 695/695 with two workers and a 30-second test timeout;
TypeScript, locale runtime checks, and lint ratchet (229 baseline / 229 candidate)
pass. The default five-second schema-generator test timeout was hit under load,
then the test passed with the longer timeout. Full locale extraction reports the
same unrelated missing keys on main; no new History keys are missing. New
non-English History overlays intentionally use English until translated.

Still required before readiness: local strict Clippy and Rust reference suite,
native screenshots and full smoke, exact rendering regression and full hosted
checks. Optional Lightroom provenance import stays later scope. The maintainer
reviews and merges; this draft must not be merged before those gates finish.
