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
