# Saved History parked for shutdown

The maintainer paused this task for travel on 2026-10-31. PR #500 remains draft.
Keep source and branch `codex/306-saved-history`. No new builds or renders until
an explicit resume request.

Implemented at `3ceadb7256d2b9aa5b5baf36641ac89460a5e8c6`: portable storage,
companion handling, editor persistence, dockable panel, immutable snapshots,
branching, #498 numeric transitions, library steps and explicit AI completion.
The Auto lens app-handle ownership error is fixed. The native scenario checks
observed slider values to account for coordinate quantization.

Validation: frontend 695/695 before final small refinements; focused History
10/10, TypeScript, locale runtime and lint ratchet 229/229 passed. Local Rust
compilation never began: queued Clippy jobs were cancelled before acquiring the
build lock. Pipeline exit -15 receipts record cancellation, not Clippy findings.
No native engine or screenshots exist.

Next: fix hosted strict Clippy dead-code wrappers `update_sidecar` in
`exif_processing.rs` and `save_develop_metadata` in `file_management.rs`, after
reviewing their callers. Then run fmt, strict all-target/all-feature Clippy and
Rust/reference tests, the native panel scenario with screenshot review, full60,
and exact-head hosted checks. Independent review is required before readiness.

Full60 control is read-only `.cache/control-main-c49d7d66/RapidRAW`, exact main
`c49d7d665eebe3b9a8aa92f57343c00e5d1232ac`, executable SHA256
`7cf2aaac8857e4ea01e39cb3633be5ea2b0ea122bc90d23036109b3301e0e5e9`.
The coordinator owns control cleanup. Own target directories and build/watch
processes are absent. Local `.cache/lote-306-*` notes and pipeline are retained;
the pipeline is stopped and must not restart while parked.
