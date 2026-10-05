# Lensfun oracle

`expected.json` holds what the Lensfun C++ library computes for a few lenses of the bundled database (`src-tauri/lensfun_db`). The test `lensfun_oracle_*` in `src/image_processing.rs` evaluates the same lenses with RapidRoom's code and compares the results.

Each case names a lens entry, a camera (by its EXIF `Make` and `Model`), the focal length, the aperture and distance for the vignetting, and an image size. For 40 points along four rays from the centre, `oracle.cpp` records:

- `distortion`: `[r, ratio]`, where `r` is the radius of an output pixel divided by half the image diagonal, and `ratio` is the radius of the source pixel that Lensfun samples divided by `r`.
- `vignetting`: `[r, gain]`, the gain Lensfun applies to that pixel.

The cases cover the models ptlens, poly3 and poly5; full frame, APS-C, Micro Four Thirds and compact sensors; calibrations made on another sensor size; and 3:2, 4:3, 16:9 and portrait images. Focal length, aperture and distance match a calibration point, because RapidRoom interpolates between calibrations differently from Lensfun.

## Regenerating

Build Lensfun (tested with the git master of 24 September 2026, version 0.3.99) and run the generator against the bundled database:

```bash
git clone https://github.com/lensfun/lensfun.git && cd lensfun
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTS=OFF -DBUILD_LENSTOOL=OFF -DINSTALL_HELPER_SCRIPTS=OFF -DBUILD_DOC=OFF
make -C build -j8 lensfun
g++ -O2 -std=c++17 -Ibuild $(pkg-config --cflags glib-2.0) \
  <rapidroom>/src-tauri/tests/lensfun_oracle/oracle.cpp \
  -Lbuild/libs/lensfun -llensfun $(pkg-config --libs glib-2.0) -o oracle
LD_LIBRARY_PATH=build/libs/lensfun ./oracle <rapidroom>/src-tauri/lensfun_db \
  > <rapidroom>/src-tauri/tests/lensfun_oracle/expected.json
npx prettier --write <rapidroom>/src-tauri/tests/lensfun_oracle/expected.json
```

Lensfun is LGPL-3.0 and is not part of RapidRoom; only the generator source and its output are kept here.
