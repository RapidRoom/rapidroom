# Saved edit history panel and history file format

Issue #306. Design only: no implementation is included or authorized by this document. The maintainer approved decisions 1–6 on 2026-10-29 (see the end); decisions 7 and 8 are pending.

## Goals and current base

- Show the current photo's history newest first, with time and actor.
- Jump to an editable step, then replace the later steps if a new edit branches from it, as Lightroom does.
- Persist history beside the photo so it survives restart and moving the folder.
- Name and restore immutable snapshots.
- Mark assistant/MCP edits explicitly and optionally show Lightroom-imported history as read-only provenance.
- Keep the history file bounded without sacrificing the current edit or a named snapshot, and keep the `.rrdata` sidecar small.

PR #460 already supplies the session model: each exact photo path has an in-memory stack of full `Adjustments` snapshots, parallel `HistoryDetails`, and a current index. Switching photos flushes the pending push and saves the stack; a stack is restored only when its current step equals the loaded adjustments. Fifty photos are retained in an LRU, and each stack has at most 50 steps. The persistence design extends that model rather than adding a second history engine.

The `.rrdata` sidecar's root `adjustments` remains the authoritative state for rendering and export. History lives in its own companion file, `<photo>.rrhistory`, next to the photo and its `.rrdata` (decision 2). The sidecar format does not change, the library never reads history, and a damaged history file can't affect the edit.

## Panel proposal

Add **History** as a dockable editor panel using the existing panel docking and visibility controls. Its open/closed and dock position are UI settings, not photo edits and not history data. The existing right-click Undo/Redo menu remains available.

```text
┌ History ──────────────────────────── [ + Snapshot ] [×] ┐
│ SNAPSHOTS ▾                                             │
│  ◈ Before sky                               29 Oct 14:02│
├─────────────────────────────────────────────────────────┤
│  ● AI: Sky: Exposure                 -0.60   0.35    AI │
│  ○ Saturation                          +14     13       │
│  ○ Contrast                            +12     12       │
│  ○ Snapshot: Before sky                                 │
│  ○ Crop 3:2                                             │
│  ○ Preset: Warm film (Library)                          │
│  ◇ LR: Basic settings                                LR │
│  ○ Initial state                                        │
└─────────────────────────────────────────────────────────┘
```

- **Newest step is at the top.** The filled marker and `aria-current="step"` identify the current step; actor is conveyed by text/icon, never color alone.
- **Rows follow Lightroom's History format:** the setting's name, then the change, then the resulting value, both right-aligned in fixed columns (`Saturation  +14  13`). The change column uses a dimmer text color than the value, as in Lightroom. Steps that are not one numeric setting (crop, curve, mask, preset, paste, reset) show their label only. A small badge marks `AI` and `LR` steps; user steps have no badge. A tooltip gives the actor, the full localized date/time and the complete label. Unknown imported times are `—`.
- Clicking an editable step renders it and moves the current index without adding a step. A subsequent edit discards the later steps and appends the new branch. A small status line, `New edits replace 3 later steps`, appears while later steps exist; no confirmation dialog is proposed.
- **+ Snapshot** names the current state in a small dialog. Snapshot rows can be renamed or deleted through a menu. Restoring a snapshot adds an editable step labelled `Snapshot: <name>`; it does not move or rewrite the immutable snapshot.
- `◊`/◈ marks a named snapshot. `◇ LR` marks read-only imported provenance. Selecting an imported row shows its details but does not change adjustments.
- Keyboard behavior: Up/Down moves row focus, Home/End jumps to the ends, Enter activates, and Delete is available only for a named snapshot after confirmation. Focus order follows visual order. Fifty steps and twenty snapshots do not require virtualization in the first version.

## Storage options

Where saved history should live is debated upstream in `CyberTimon/RapidRAW#1059`: one user wants sidecars in app data, others want them beside the photo so a folder moves between machines with its edits, and a subfolder (for example `_rrd/` with `.rrdata` and `.rrhistory` files) is also proposed.

| Option                                  | Portability (move, copy, sync folders)                                                                                                               | Sidecar size and sync cost                                                                                                                                                                                                 | Interoperability                                                                                               | Conflict risk                                                                                                                                                                         |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **In the `.rrdata` sidecar**            | Moves with the photo's one existing file. Nothing extra to copy.                                                                                     | The sidecar grows by up to 2 MiB (more with AI masks). XMP sync is unaffected: it writes only rating, label and tags. Immich sync uploads the whole sidecar JSON as asset metadata, so every edit uploads the history too. | Other tools ignore `.rrdata` either way. Upstream readers keep the unknown root field (`ImageMetadata.extra`). | Root adjustments and history are written in one locked atomic transaction, so they can't disagree. Two devices editing the same photo conflict on one file, as today.                 |
| **Separate `.rrhistory` file (chosen)** | Moves only if the user, the file manager or sync tool moves both files. A photo copied with only its `.rrdata` silently loses history.               | `.rrdata` stays small; the history file can be excluded from Immich or synced at its own pace.                                                                                                                             | Same as above.                                                                                                 | Two files must be kept consistent. A crash or partial sync between them leaves a history whose current step doesn't match the sidecar (the integrity rule then ignores that history). |
| **App data**                            | Lost when the folder moves, is renamed outside RapidRoom, or is opened on another machine. This is the main objection in `CyberTimon/RapidRAW#1059`. | No sidecar or sync cost.                                                                                                                                                                                                   | None needed.                                                                                                   | No file conflicts, but history goes stale or orphaned after renames and has to be keyed by path or file hash.                                                                         |

A subfolder layout (sidecars and history under `_rrd/`) changes where all sidecars live, not just history, so it belongs in its own issue. It is compatible with either of the first two options.

**Decision (maintainer, 2026-10-29): a separate `<photo>.rrhistory` file next to the photo.** The sidecar stays small for fast library browsing, history is read only when a photo opens in the Editor, and a damaged history can't affect the edit. The costs in the table are handled as follows:

- **Portability.** `.rrhistory` is a companion file like `.rrdata` and `.rrexif`. Every RapidRoom operation that renames, moves, copies, duplicates, deletes or clears a photo's companion files carries it along (see [Companion files](#companion-files)). Copying a folder outside RapidRoom carries it as long as the whole folder is copied.
- **Consistency.** The two files are written separately, so a crash between the writes, or an edit by a tool that doesn't know about history, can leave a history whose current step doesn't match the sidecar. That history is ignored (see [Loading and integrity](#loading-and-integrity)), as #460's in-memory history already is.
- **Sync.** The maintainer does not use Immich. History is never uploaded to Immich: its sync keeps uploading `.rrdata` only.

## History file schema

`<photo>.rrhistory` is a UTF-8 JSON file, written atomically (temporary file plus rename) under the same per-photo lock as the sidecar. A virtual copy, whose sidecar is `<photo>.<copy-id>.rrdata`, has its own `<photo>.<copy-id>.rrhistory`. The file's `version` controls only this schema; the sidecar's `version` is independent.

```jsonc
{
  "version": 1,
  "currentStepId": "6f2d694c-5e2f-4c4f-b5f4-78b73de0b992",
  "compactedSteps": 0,
  "steps": [
    {
      "id": "58f168f5-6ce9-4f0e-a51a-c59fb2ea8070",
      "label": "Initial state",
      "actor": "system",
      "timestamp": 1761732000000,
      "changedKeys": [],
      "adjustments": { "exposure": 0 },
    },
    {
      "id": "6f2d694c-5e2f-4c4f-b5f4-78b73de0b992",
      "label": "AI: Sky: Exposure -0.60",
      "actor": "assistant",
      "timestamp": 1761732420000,
      "changedKeys": ["masks"],
      "adjustments": { "exposure": 0.35 },
    },
  ],
  "snapshots": [
    {
      "id": "b5f27ed1-0db4-4f4b-a6bb-8f257fbdc90e",
      "name": "Before sky",
      "createdAt": 1761732120000,
      "adjustments": { "exposure": 0.35 },
    },
  ],
}
```

The adjustment objects above are abridged; real steps contain complete restorable `Adjustments` values.

### Fields

| Field                     | Rules                                                                                                                                                                                |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `version`                 | Integer, initially `1`. Unknown newer versions are ignored for editing and are not silently overwritten.                                                                             |
| `currentStepId`           | Must identify an item in `steps`; the identified step is the current editable state.                                                                                                 |
| `compactedSteps`          | Cumulative count of older steps removed by compaction, used for a `+N earlier steps compacted` note.                                                                                 |
| `steps`                   | Oldest to newest in memory; the panel reverses them for display. Maximum 50 after routine compaction. Only this active linear branch is stored, not a tree.                          |
| `steps[].id`              | UUID generated once and stable across autosaves. Array indices are not persistent identity.                                                                                          |
| `steps[].label`           | Existing readable history label, bounded to 240 UTF-8 bytes. UI wraps and exposes the full text.                                                                                     |
| `steps[].actor`           | `user`, `assistant`, `system`, or `imported`.                                                                                                                                        |
| `steps[].timestamp`       | Unix milliseconds, or `null` when an imported time is unknown. Display uses local time; storage is UTC.                                                                              |
| `steps[].changedKeys`     | Existing changed-top-level-key list, capped at 64 known keys; used for details, not reconstruction.                                                                                  |
| `steps[].display`         | Optional `{ "name", "delta", "value" }` for steps that change one numeric setting, recorded when the step is pushed. Drives the Lightroom-style row columns; absent for other steps. |
| `steps[].adjustments`     | Complete state, not a delta. This makes every remaining row independently restorable and compaction safe.                                                                            |
| `snapshots`               | At most 20, oldest to newest. Named snapshots are immutable and are never removed by history compaction.                                                                             |
| `snapshots[].id`          | UUID, independent of history-step IDs.                                                                                                                                               |
| `snapshots[].name`        | User-visible name, trimmed and bounded to 120 Unicode scalar values.                                                                                                                 |
| `snapshots[].createdAt`   | Unix milliseconds.                                                                                                                                                                   |
| `snapshots[].adjustments` | Complete immutable state. Duplicating a step's state is intentional: branching or deleting steps cannot invalidate a snapshot.                                                       |

Optional imported steps additionally use `actor: "imported"` and `source: "lightroom-catalog"` or `source: "xmp"`. They are provenance rows and are never current. Do not store catalog paths, raw SQL/Lua blobs, assistant prompts, tool arguments, or other unnecessary private data.

## Loading and integrity

1. Read the sidecar's root adjustments as today. Read `<photo>.rrhistory` only when the photo opens in the Editor. A missing history file produces one initial step from the root adjustments, matching current behavior.
2. Restore the saved stack only when the saved current step's adjustments deep-equal the root adjustments. This is the persistent form of #460's cache guard.
3. If they differ, the root adjustments win and the saved history is ignored, as #460's in-memory history already is: the panel starts from one initial step equal to the root adjustments, and the next history write replaces the stale file. This covers a crash between the sidecar and history writes, a sidecar changed by another tool or machine, and a sidecar restored from backup. Never replace newer edits with an older history current step.
4. Invalid JSON, invalid or duplicate IDs, an absent current ID, invalid actor values, or an unsupported history version make only the history unavailable. Show a warning and retain the root edits. Do not overwrite an unreadable or newer-version history file, so a newer RapidRoom can still read it.
5. Under the photo's lock, write the sidecar first, then the history file, each atomically. If the history write fails, the edit is still saved; on the next open, the current-step check (rule 3) ignores the stale history. Undo, redo, panel jumps and snapshot restoration update the persisted current ID even when the root adjustments happen to be unchanged.
6. No absolute photo or catalog path is stored. Moving the folder with its companion files therefore preserves history. Virtual copies retain their own exact-path sidecars and history files; ordinary adjustment copy/paste creates one step rather than copying a stack.

### Companion files

`.rrhistory` joins `.rrdata` and `.rrexif` wherever RapidRoom handles a photo's companion files. Each of these must carry, copy or remove it:

- `find_all_associated_files`, which drives delete, move and rename of a photo and its companions;
- `duplicate_file_on_disk`, which copies the history with the duplicated photo's sidecar (the copy then evolves independently);
- virtual-copy creation (starts without history) and virtual-copy deletion (removes that copy's `.rrhistory`);
- import and file-transfer paths that pair sidecars with photos;
- `clear_all_sidecars` and `deletion_stem_for`, so clearing sidecars and recognising orphaned companions include `.rrhistory`;
- the Immich file filter (`immich/files.rs`), which must skip `.rrhistory` like `.rrdata` and `.rrexif` so it is never uploaded as a photo.

Folder scans already ignore unknown extensions, so the library never lists `.rrhistory` files.

## Size cap and compaction

Routine limits are **50 steps**, **20 named snapshots**, and a **2 MiB history-file target**. Labels, names, counts, and JSON nesting are also validated on read. A 16 MiB read ceiling covers valid oversized states while bounding malformed input.

Compaction runs before writing when there are more than 50 steps or the serialized history exceeds 2 MiB:

1. Keep the current step and every named snapshot. Snapshots own their states and are not dependent on retained steps.
2. Drop the oldest read-only imported rows first, then the oldest non-current editable rows. Keep the initial row when space permits. Increase `compactedSteps` by the number removed.
3. Stop when there are at most 50 steps and the history is at most 2 MiB. Since every row is a full snapshot, no replay or delta chain is broken.
4. The 2 MiB target is not permission to lose required state. If the current step plus named snapshots is larger, retain them and allow the history to exceed the target up to the 16 MiB ceiling.
5. Above 16 MiB, save no new history revision and leave the previous history file untouched. Root adjustments still save, and the user gets a persistent warning that history could not be updated. Do not silently delete the current edit, snapshots, or the last readable history.

This favors recent editable work and named snapshots while acknowledging that AI mask payloads can be large. A future compressed/delta schema would need a new history version and is not part of the first implementation.

## Recording edits and assistant steps

- Reuse `pushHistory`, `describeHistoryChange`, `HistoryDetails`, and the existing 500 ms debounced push. Flush it before switching photos and before serializing history, as #460 already does.
- One slider gesture remains one coalesced step. Explicit actions such as presets, paste, reset, and Auto retain their existing labels.
- `useMcpBridge.applyEdit` already calls `describeHistoryChange(..., "assistant")` and pushes one undoable step. Persist that as `actor: "assistant"` and show the AI badge. Every MCP mutation must use this path; no label-prefix heuristic should be the sole source of actor identity.
- Background AI completion should produce one atomic assistant step after its adjustments commit, not a step per progress event. Prompts, responses, model logs, and tool arguments are excluded.
- `system` is reserved for initial states and edits detected outside the saved history. `imported` is read-only provenance. Existing in-memory user/assistant details map directly; system/imported metadata is added at the persistence boundary.

### Bulk library actions

`CyberTimon/RapidRAW#1059`'s users asked for history entries when they act on photos outside the editor: applying presets from the library, paste/sync to a selection, and Reset. These photos have no in-memory stack, because they aren't open. So the batch commands (`apply_adjustments_to_paths` and the preset/reset paths) append one step per photo to its `.rrhistory` under the same lock, right after the write that changes its `.rrdata` adjustments, for example `Preset: Warm film (Library)` or `Reset (Library)`, with `actor: "user"`. The usual compaction rules apply. The next time the photo opens, the panel shows the step, and Undo can go back past it.

Ratings, flags, color labels and tags are not adjustments and don't change the rendered image. Lightroom keeps them out of develop history too. This proposal keeps them out of history, so that culling a shoot doesn't bury the edit steps. Recording them is listed as a decision.

## Lightroom-imported history

This is optional and should be a later implementation PR after the panel and persistence work.

- Ordinary XMP normally supplies current develop settings, not a develop-step list. Do not invent a sequence from one XMP state.
- Lightroom catalogs can contain `Adobe_libraryImageDevelopHistoryStep` with develop text and optional `dateCreated`; the current importer already reads this table for as-shot white balance.
- Map available steps through the existing develop-settings converter, in catalog order. Store only successfully mapped bounded states, actor `imported`, source `lightroom-catalog`, and a null timestamp when absent. Rows that cannot be mapped may retain a generic read-only label but must not claim a restorable state.
- Imported rows are shown for provenance only. Add a normal editable `Imported from Lightroom` current step for the root adjustments, so selecting an imported row never replaces current edits.
- Apply the same 50-step/2 MiB compaction rules and report omitted imported history through `compactedSteps`.

## Implementation sequence after approval

1. History file schema, validation, atomic persistence, current-state guard, branching, compaction, companion-file handling, and round-trip tests.
2. Panel with editable steps, actor/time display, keyboard behavior, and snapshot create/rename/delete/restore.
3. Assistant/background completion audit, library batch steps (presets, paste/sync, Reset), and restart/move/virtual-copy integration tests.
4. Optional read-only Lightroom catalog history import.

Required checks include schema round trips, an unchanged `.rrdata` format, mismatched current state (including a crash between the two writes), invalid/newer history, redo truncation, independent snapshots, 50-step and byte compaction, large AI masks, restart and folder moves, exact-path virtual copies, assistant actor persistence, library batch steps, `.rrhistory` carried by rename/move/duplicate/delete and skipped by Immich, and keyboard accessibility. Default rendered output must remain unchanged; the full regression check is required for the implementation PRs.

## Decisions for the maintainer

Decided by the maintainer on 2026-10-29:

1. **Approved:** History panel placement, row content and mockup, including newest-first order, immediate click-to-jump, and Lightroom's row format (setting, change, resulting value).
2. **Decided:** history is stored in a separate `<photo>.rrhistory` file next to the photo (one per virtual copy), not in the `.rrdata` sidecar. Rename, move, copy and delete carry it; a history whose current step doesn't match the photo's adjustments is ignored. Immich is not used, and history is never uploaded to it.
3. **Approved:** library presets, paste/sync and Reset record one step per photo; ratings, flags, labels and tags do not.
4. **Approved:** a linear active branch with later-step replacement, rather than a persistent history tree.
5. **Approved:** full adjustment snapshots and the 50-step / 20-snapshot / 2 MiB target / 16 MiB ceiling, including the oversized-history warning behavior (can be revisited).
6. **Approved:** independent full states for named snapshots, restored as new editable steps.

Pending:

7. Approve read-only Lightroom provenance as optional later scope, with no fabricated XMP history.
8. Confirm the implementation sequence above.
