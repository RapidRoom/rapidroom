# XMP regression inputs

`j11-native-settings.json` reproduces the fractional point curve and grading
settings from the J11 export regression. It contains edit settings only, with
no image, EXIF, profile table, name, path or other photo metadata.

The external Lightroom-written reference stays private and is not committed.
Run its opt-in reference check with an unmodified Lightroom-written sidecar
containing a Custom point curve and nonzero shadow and midtone grading:

```sh
RAPIDROOM_LIGHTROOM_XMP_FIXTURE=/private/reference.xmp cargo test --lib real_lightroom_custom_curve_and_grading_round_trip -- --ignored
```

The check exercises the actual photo parser, export projection and native
payload reconciliation. It is not proof of Lightroom accepting a new export;
that remains a human application check.
