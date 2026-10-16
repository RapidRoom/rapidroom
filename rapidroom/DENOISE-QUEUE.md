# Denoise marked photos

Mark a selection with **Q**, or use **Mark for denoise** in its context menu. Change the shortcut in Settings > Controls. Marks have their own thumbnail badge and do not change ratings, flags or colour labels. The library view options include **Marked for denoise** and **Denoised** filters.

Open **Denoise marked photos…** from a thumbnail context menu to process the current folder (optionally including subfolders). Pick a method and optional Sharpen. The queue runs one photo at a time in the background; continue culling, navigate to another folder, or cancel from the progress strip. Install the required AI models through the existing Denoise dialog before an unattended run. AI-Free mode offers Basic and, on macOS, Apple RAW 9.

The same queue is available without an editor window:

```sh
rapidroom denoise --marked /path/to/photos --method best
rapidroom denoise --marked /path/to/photos --recursive --method fast --sharpen
rapidroom denoise --marked /path/to/photos --dry-run
```

Methods: `best`, `fast`, `nind`, `bm3d`, and `raw9` (macOS). `--strength 0..100` sets Basic strength or legacy model quality; Best/Fast have fixed model quality. Sharpen is opt-in and applies only to Best/Fast. Best/Fast write sensor DNG beside compatible Bayer RAWs; other methods retain TIFF for RAW and PNG for rendered sources. Existing images are kept. OpenVINO CPU is used when its verified runtime is bundled; stock CPU fallback remains available.

Run the same command again after an interruption. The queue keeps marks and skips verified results matching the source contents, processing options, model/pipeline and runtime. It writes output provenance before publishing an image, then commits a completion receipt to the source `.rrdata`. Files are content-addressed and never overwritten. Corrupt or conflicting outputs are reported and preserved. Mark removal stops future discoveries; Cancel also stops a running job. CLI exits: 0 success (including all-skipped), 1 processing failure, 2 invalid arguments, 130 Ctrl+C cancellation. Force-killed jobs can also be resumed.

Source edits are copied to the result. A later rerun synchronizes updated source edits without repeating sensor denoise; independent edits on a result are preserved and reported as a conflict when synchronization would replace them. Keep the output `.rrdata` and `.rrqueue` files with its image. Sidecar and per-source lock files are persistent advisory lock anchors; stale file presence does not mean a job is running. Source symlinks are refused: mark the original file. Card mode remains read-only. No XMP/DxO mark export or power-policy changes are included.
