# Sony ARW camera-data correction

The Rust implementation uses the Sony metadata-v1 radial model documented in
[darktable lens.cc](https://github.com/darktable-org/darktable/blob/6eca5133000b64a07c8e967aec2028fecb7ec923/src/iop/lens.cc)
and its EXIF reader at the same revision. Copyright (C) 2019–2026 darktable
developers, GPL-3.0-or-later; independently reimplemented with attribution.

Only Sony ARW sources and explicit camera recipe version 1 use this path.
Signed SHORT tables retain their count word; the supported reader shape is
16 knots. Distortion selects camera data before Lensfun. Each other component
selects its source independently. Renderer-owned payloads and source labels
participate in the existing full, geometry, preview and thumbnail identities.
Saved legacy recipes retain their previous interpretation.

Camera vignetting and camera TCA are disabled in the production resolver until
same-shot validation passes. The vignette-only mathematical regression checks
the linear-image warp; it is not photographic acceptance. Private RAWs,
camera JPEGs, Lightroom exports and Adobe LCP files are never committed here.
Physical validation receipts and review panels remain local. No reference
render is updated automatically.
