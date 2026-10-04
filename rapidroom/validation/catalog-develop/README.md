# Catalog photo-edit import

The source is Laurensius Adi's catalog importer at
[dda6cc51c6](https://github.com/laurensiusadi/RapidRAW/commit/dda6cc51c69dc6a17906609dbb667eff3507aeeb).
The RapidRoom adaptation uses the existing read-only collections catalog reader,
root relinker, bounded Lua-table parser and image-aware XMP converter.

Open **Import Lightroom Catalog**, review the shared root-folder mappings, then
choose **Photo edits**. Review each photo's converted controls and unsupported
settings. New readable photos are selected initially; existing sidecars are not.
Choose photos explicitly and import. Saved edits and ratings have independent
replacement choices. A new virtual copy inherits unrelated master metadata
without changing the master's sidecar. Repeating the import identifies the same
copy and preserves its existing edits by default.

Use a closed or safely copied catalog. The existing catalog reader rejects
active lock files and snapshots a readable WAL database without modifying it.
Rows are limited to 4 MiB before and after decompression. Unknown schema, missing
photos, malformed settings and unreadable saved edits are reported. Catalog,
photo and sidecar changes invalidate the preview. Atomic writes preserve
unrelated metadata and report individual write failures; the batch is not a
multi-file transaction. Card mode remains read only.

White balance uses explicit as-shot history when available, never a folder
median. Crop and straighten use the current converter. Catalog rotations are
supported relative to source EXIF for non-mirrored orientations; unknown or
mirrored geometry is omitted and reported. Nested profile luma curves use the
existing composition mapper, but profile color/LUTs, local and AI masks, virtual
copy titles and other unsupported settings are shown as not transferred. No
Adobe rendering parity is promised.

## Verification and limits

Native fixtures cover read-only previews, selection, preserved metadata and
cleared ratings, independent replacement, stale previews, repeated virtual
copies with formerly colliding IDs, compressed and malformed rows, portrait
crop/rotation, unsupported fields, Card mode and write failures. Frontend tests
exercise the two import tabs, folder relinking, default selection, independent
replacement choices, fresh fingerprints after errors, partial reports and Card
mode. The regular 60-render corpus checks unchanged rendering behavior.

No real Lightroom catalog was available locally. Private calibration uses a
closed **synthetic catalog with global settings derived from a real Lightroom
sidecar**, on a copied portrait RAW and its Lightroom reference export. This
checks the shared mapper and rendered result, not Adobe's real catalog schema.
All private artifacts stay local and are excluded from this repository.

A human still needs to test a real closed catalog in the desktop app, relinking
from another machine, the per-photo review and replacement flow, virtual-copy
visibility after import/restart, unsupported reports and appearance against
Lightroom, plus macOS and Windows behavior. Those checks belong in issue #63.
