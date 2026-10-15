# Public MCP gap reproduction

Issue #39 supplies a permitted, repeatable subset of the
[September assessment](https://github.com/sheldonxxxx/RapidRAW/blob/4494beb9bae0ee57f420a8292c4f4f6e1dba4bd2/docs/mcp/history/gap-assessment-2026-09.md).
It targets **Sheldon's native fork protocol**, using a pinned public checkout.
RapidRoom's GUI MCP has different tools and schemas; this package does not claim
protocol compatibility or identical rendering between forks.

The genuine Sony A7C II APS-C RAW is CC0 from raw.pixls.us, identified by its
published SHA256 and byte count in [fixtures.json](fixtures.json). It is an
existing regression photograph, never a fresh holdout. A separate generated RGB
ramp is procedural contract evidence, never photographic evidence. No private
catalog, photographs, account credentials, model download or model request is
needed. The generated ramp pixels are released under CC0-1.0; package code and
the externally imported fork helpers retain their respective AGPL-3.0 licences.

## Clean checkout

The measured target is Linux with `flock`, `nice`, Python >=3.11, Node >=22.12, Rust >=1.98,
the fork's normal native build dependencies and a working native renderer.
Other operating systems and GPU drivers need their own execution receipts.
Use a local filesystem and at least 25 GiB free. Build steps own only
`/tmp/rapidroom-build.lock`; the replay owns only `/tmp/rapidroom-gpu.lock`.

From a clean RapidRoom checkout:

```sh
git clone https://github.com/sheldonxxxx/RapidRAW.git samples/mcp-gap-fork
git -C samples/mcp-gap-fork checkout --detach 4494beb9bae0ee57f420a8292c4f4f6e1dba4bd2
(cd samples/mcp-gap-fork && npm ci --ignore-scripts && npm ci --ignore-scripts --prefix mcp)
python3 rapidroom/validation/mcp-gap-repro/build.py \
  --fork "$PWD/samples/mcp-gap-fork" \
  --out "$PWD/samples/mcp-gap-engine" \
  --target "$PWD/samples/mcp-gap-target"
node rapidroom/validation/mcp-gap-repro/fetch.mjs \
  "$PWD/samples/mcp-gap-photo.ARW"
node rapidroom/validation/mcp-gap-repro/replay.mjs \
  --fork "$PWD/samples/mcp-gap-fork" \
  --binary "$PWD/samples/mcp-gap-engine/rapidraw-mcp" \
  --build-receipt "$PWD/samples/mcp-gap-engine/build.json" \
  --photo "$PWD/samples/mcp-gap-photo.ARW" \
  --out "$PWD/samples/mcp-gap-run-1"
```

The downloader is an explicit network action, accepts only this fixture's bytes,
caps the response at its expected size and refuses to replace an existing file.
Alternatively, pass an existing copy to `--photo`: its hash and size must match.
The build pins its executable before releasing the compile lock and records the
clean source commit, command and binary hash. The replay requires that receipt,
the matching clean fork checkout and unchanged source/binary files. It uses the
fork's own SDK/native evidence and PNG decoder helpers, imported from the pinned
checkout without copying or modifying them. Their original authorship stays in
that repository.

Use a new output directory for every retry. Keep first failures; do not combine
calls from different source/binary identities into one passing report. The
replay never invokes an arbitrary RapidRAW executable with guessed CLI flags:
the fork's pinned MCP host owns the documented native bridge startup.

## Exact subset and expected results

Both fixtures use these operations through the real SDK/native bridge:

| Step              | Exact edit or operation                                                                                  | Required result                                                                       |
| ----------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Baseline          | `open_photo(inherit_sidecar:false)`, PNG preview long edge 800                                           | Genuine RAW dimensions 4608×3072; procedural dimensions 512×384                       |
| Global edit       | `set_adjustments`, current `expected_revision`, `exposure:0.5`                                           | Independently decoded preview pixels change                                           |
| Shared history    | `undo`, then `redo`                                                                                      | Undo restores baseline pixels exactly; Redo restores edited pixels exactly            |
| Manual local lift | Radial centre `(width/2,height*0.75)`, radii `(width/5,height/6)`, rotation 0, feather 0.2, exposure 0.5 | Central 64×64 native pixels change; the upper-left 64×64 native pixels stay identical |
| Persistence       | `save_session`, close session, reconnect native process                                                  | Adjustments/revision and decoded final preview pixels survive exactly                 |
| Delivery          | PNG, 8 bit, sRGB profile, long edge 800, no capture metadata, GPS stripped                               | Decoded RGB8 layout/dimensions and every pixel equal the final-session render         |
| Preservation      | Hash/mtime plus the fork's original-sidecar and source/binary guards                                     | Inputs and original sidecars remain unchanged                                         |

The export comparison allows **zero** channel-code difference. At the pinned
fork revision, previews (`mcp_bridge/render.rs:encode_preview_image`) and
profiled 8-bit exports (`delivery.rs:encode_profiled_raster`) both convert the
native frame with `to_rgb8`; both use Lanczos3 resizing to the same dimensions.
The explicit sRGB profile embeds metadata without a colour transform. PNG is
lossless, so there is no additional quantization allowance. RGB8/channel layout
is checked explicitly: a transparent RGBA replacement cannot pass by hiding
otherwise matching RGB samples. A constant final reference is refused too.

The full call ledger preserves exact parameters and returned identities locally.
The expected public result is 16 passed assertions and `passed:true`, **after**
the fork's close-time preservation checks pass too. Missing, duplicate or failed
assertions, a different binary, a dirty checkout or a close-time failure cannot
produce a passing public summary. Exit status is 0 only for that complete pass.

The package's lightweight guards run without a native engine:

```sh
node --test rapidroom/validation/mcp-gap-repro/contract.test.mjs
```

## Share only the public projection

`public-summary.json` is rebuilt from a closed field list: fixture provenance,
source and executable hashes, validated component versions, assertion IDs and
booleans. Native metadata, arbitrary errors, tokens and absolute paths never
enter this object. It records the replay script hash so changed runners cannot
silently share an identity. Inspect it before posting.

The fork's `evidence.jsonl`, `coverage.json`, `summary.json`, stderr and native
workspace **are local diagnostics**, and include operational paths. Do not post
them wholesale. Inspect the CC0 overview and matched native crops before sharing
images; no private file can substitute for the checksummed photograph. Failed
startup may leave only local diagnostics; an absent public report is not a pass.

## Assessment boundary

This subset addresses the assessment's reproducible-public-fixture prerequisite
and representative rendering, manual local editing, history and persistence.
It does not reproduce the historical call/requirement counts or certify the
entire assessment. A visible local change and a protected corner are contract
checks, not proof of a useful photographic edit or fine mask boundaries.

AI selection/refinement, hair/feather extraction, genuine HDR brackets, panoramas,
focus stacks, lens calibration, film negatives, model/provider behavior and
fresh macOS/Windows setup remain outside this subset. One public capture cannot
establish general quality. There is no automatic visual pass, aesthetic score,
fresh-holdout claim or RapidRoom-versus-fork renderer comparison. The rendered
artifacts are review material; native execution and photographic judgment remain
separate evidence.
