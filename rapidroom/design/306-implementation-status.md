# Saved History implementation: parked draft

This is unfinished work for #306, based on the approved design in PR #475. It is parked for later development and is not ready to merge.

The current scaffold adds versioned `.rrhistory` types, bounded reads, validation of identifiers, actors, labels and nesting, current-step integrity checks, compaction that retains the current step and named snapshots, and protection for unreadable or unsupported history files. A read command and an opt-in save command are registered. The save path writes the authoritative sidecar first and the history second under the existing sidecar lock, returning a history warning without discarding the saved edit. Existing save callers retain their original API.

Only Rust formatting was run before the pause. This code has not been compiled or tested. The editor does not yet use the new commands. No claim is made that persistent History works in the application.

Resume in this order:

1. Review and compile the storage scaffold. Add reference checks using real sidecars for round trips, a crash between the two writes, malformed/newer history protection, exact-path virtual copies, bounded compaction and oversized states.
2. Complete every companion-file path from the design: associated-file discovery, physical duplication, virtual-copy creation/deletion, import and transfer pairing, clearing sidecars, deletion stem recognition, and the Immich upload filter. Verify rename/move/delete preserve the pairs.
3. Connect the existing per-photo editor history to the commands, preserving stable step IDs, actor/time details, branching, autosave ordering and persistent warnings. Keep the `.rrdata` format unchanged and keep History reads out of library browsing.
4. Include #498 in this storage/history work: numeric labels show start and end (`Highlights 0 → +25`), including masks, HSL and color grading; booleans remain on/off; preserve the six-change cap and `AI:` prefix. Add `Closes #498` only when implemented.
5. Finish the dockable History panel and immutable named snapshots, then batch preset/paste/reset and assistant-completion recording. Ratings, flags, labels and tags remain outside develop history. Optional Lightroom provenance stays later scope.
6. Run the required local, hosted, rendering and private-native checks and attach the panel screenshots before readiness.

The intended split remains storage/companions first, panel/integration second. No new build should be started while this work is parked.
