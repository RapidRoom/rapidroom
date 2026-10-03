# Pixel-exact regression check

`baseline.py` renders a corpus of CC0 raws (`corpus.json`: 20 files from [raw.pixls.us](https://raw.pixls.us), with URLs and SHA-256 hashes) × the adjustment sets in `adjustments/` through RapidRoom's headless CLI export. It then compares the renders pixel by pixel against a reference.

```bash
python3 rapidroom/regression/baseline.py fetch --raw-dir <dir>              # download and verify the raws (default ~/.cache/rapidroom-corpus or $RAPIDROOM_CORPUS)
python3 rapidroom/regression/baseline.py render --engine <RapidRAW binary> --out <renders> --raw-dir <dir>
python3 rapidroom/regression/baseline.py compare <reference renders> <renders>
```

- It needs Python 3 and numpy. ImageMagick is only a fallback TIFF decoder.
- Each render uses fresh, empty XDG data, config and cache dirs, so local settings can't change the result.
- `tolerances.json` is exact (0) by default. Changing a tolerance, or updating the reference, is a rendering decision that needs a maintainer's explicit sign-off (see GOVERNANCE). Agents must never run `bless` or edit `tolerances.json` to make a compare pass.

The maintainers run this locally on a real GPU. Issue #6 tracks running it in CI on a software renderer, where the reference has to be recorded on the CI runner itself.

## CI

`.github/workflows/regression.yml` runs a subset of the check on GitHub Actions (ubuntu-24.04, no GPU). It runs on pull requests that touch `src-tauri/**` or `rapidroom/regression/**`, on pushes to `main`, and on demand (workflow_dispatch). It isn't a required check yet.

- **Renderer:** Mesa lavapipe, the software Vulkan driver (`mesa-vulkan-drivers`). The headless export still starts GTK, so it runs under `xvfb-run`.
- **Subset:** 6 raws × all 3 adjustment sets = 18 renders: `sony-a7cr-18mp-lossless-s` (Sony lossless M/S), `sony-a7c2-15mp-compressed`, `canon-r50-24mp-craw`, `nikon-zfc-21mp-lossless`, `fuji-xt4-26mp-lossless` (X-Trans) and `google-pixel7pro-dng` (phone DNG). The raws are cached with `actions/cache`, keyed on `corpus.json`'s hash.
- **Reference:** lavapipe renders differently from a GPU, so the reference can't come from the maintainers' machines. It is recorded on the runner as `ci-reference.json`: per image, the dimensions and the SHA-256 of the decoded 16-bit pixels. `baseline.py check` compares only those.
- **Determinism:** every job renders the subset twice and requires identical hashes before it compares with the reference. If the two runs differ, the job fails and `baseline.py compare` reports which images vary, and by how much.
- **Measured** (first run, [run 37111145732](https://github.com/RapidRoom/rapidroom/actions/runs/37111145732), 4-vCPU runner, cold caches): 18/18 renders bit-identical between the two runs. Build 15 min 45 s, corpus download 20 s, each render pass about 18 min 20 s (30–35 s per neutral render, 40–110 s per busy one), so about 55 min for the whole job. Rendering twice is most of that; dropping the second pass would save about 18 min.
- **On a mismatch** the job fails and uploads the `regression-diff` artifact: `diff-summary.json`, the differing renders (16-bit TIFF), their logs and the render manifest.
- **Recording:** run the workflow by hand with **record** ticked. It uploads the new `ci-reference.json` as the `ci-reference` artifact. Nothing is committed automatically: a maintainer reviews it and commits it (it's a reference update, see GOVERNANCE). While no `ci-reference.json` is committed, every run uploads a candidate and skips the compare with a warning.
- **Environment drift:** the hashes depend on Mesa, LLVM and the CPU features lavapipe uses. The reference records the Vulkan driver and package versions. If a runner image update changes them, `check` prints a note, and a maintainer re-records after checking that only the environment changed.

```bash
python3 rapidroom/regression/baseline.py hashes <renders> --out ci-reference.json --by "<who>"   # record hashes
python3 rapidroom/regression/baseline.py check ci-reference.json <renders>                       # or two render dirs
```
