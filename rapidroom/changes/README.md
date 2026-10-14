# Independent change records

Each PR adds one JSON object in `<issue>-<slug>.json` (use the PR number if there is no issue). Multiple changes for one issue may use different slugs. Keep a stable, unique `id`; do not edit another PR's record. The `legacy-NNN-*.json` files preserve every historical entry and the old display order; do not renumber them.

```json
{
  "id": "example-fix",
  "title": "Describe the user-visible change",
  "category": "fix",
  "landed": "2026-10-13",
  "authors": ["yojen7"],
  "source": "rapidroom (original implementation)",
  "commits": [],
  "upstream_issues": [],
  "upstream_prs": [],
  "rendering_change": false,
  "notes": "What changed and any limitations."
}
```

Required fields are `id`, `title`, `category`, `landed` (YYYY-MM-DD), `authors` (GitHub logins), `source`, `upstream_issues`, `upstream_prs`, and boolean `rendering_change`. Categories are `fix`, `feature`, `performance`, `privacy`, `platform`, or `ci`. Optional fields are `commits` (HTTPS URLs), `notes`, `highlight` (one short user-facing Markdown line), and boolean `rapidroom_only`. Upstream references use `owner/repo#123`; do not notify upstream by posting them unquoted in PR text. Preserve provenance, credits, and rendering-change disclosure.

Run `node rapidroom/status.mjs --check`. It validates **all** records, including your PR's, without editing any file. PR CI additionally compares generated snapshots to the exact base: `CHANGES.md` and the README's generated status block must remain unchanged; ordinary README edits outside the block are allowed. New records sort by date and filename, above the historical entries.

The Linux release workflow generates and attaches the full changelog and status snapshot to the draft release. Main pushes and the weekly status workflow generate preview artifacts, without commits to main. The committed documents remain a historical snapshot. Local previews use `node rapidroom/status.mjs`; discard generated changes before submitting a PR. `--check-generated` checks a freshly generated preview or release snapshot.

## Convert an in-flight PR

1. Save your new entry object from `rapidroom/changes.json`, then merge the migrated main. Keep main's `CHANGES.md` and generated README status block; remove the retired monolithic JSON file if it conflicts.
2. Put only that object in `rapidroom/changes/<issue>-<slug>.json`. Keep its ID, authors, provenance and all fields; do not copy the old surrounding `changes` array or historical records.
3. Run `node rapidroom/status.mjs --check`, commit your record and code, and push. Do not regenerate or commit the shared snapshot files. A new exact-head CI/review rebind is still required after the refresh.
