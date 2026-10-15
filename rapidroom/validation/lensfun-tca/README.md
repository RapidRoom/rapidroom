# Polynomial TCA reference fixtures

The XML is an original, synthetic test lens, not a calibration for a real product. It deliberately includes both signs and nonzero linear and quadratic radial terms in both colour channels. No photos or proprietary lens profiles are included.

`coordinates.json` contains 600 independent coordinate measurements from the public C API of installed Lensfun **0.3.4.0**. It covers all five anchors, ten intermediate/out-of-range focal lengths, landscape/portrait/4:3/square images, crop factors 1/1.6/2, fractional centres, edges and interior points. Six coordinates per record are red x/y, green x/y, blue x/y. Coefficients also come from the library's interpolation API. This is numerical convention evidence, not real-lens quality acceptance.

Regenerate with `python3 rapidroom/validation/lensfun-tca/generate_reference.py`, then `npx prettier --write rapidroom/validation/lensfun-tca/coordinates.json`. The generator requires the Lensfun 0.3.4.0 shared library and development package; neither becomes an application dependency. It never invokes RapidRoom's implementation.

The [Lensfun model documentation](https://lensfun.github.io/manual/v0.3.1/group__Lens.html) defines per-channel radial scale as `v + c*r + b*r*r`. The [0.3.4 coordinate setup](https://github.com/lensfun/lensfun/blob/v0.3.4/libs/lensfun/modifier.cpp) uses pixel centres and converts from the actual sensor diagonal to the calibration's half-short-side units. The explicit `tca_poly3.radius_scale` records the calibration aspect/crop factor; the renderer divides it by the actual half pixel-centre diagonal. It does not reuse distortion's legacy centre.

[0.3.4 TCA interpolation](https://github.com/lensfun/lensfun/blob/v0.3.4/libs/lensfun/lens.cpp) uses cubic Hermite interpolation; radial c/b coefficients are weighted by focal length, while v coefficients are unweighted. Existing saved coefficients without `tca_poly3` retain constant scales and the legacy sampler. Selecting a profile again opts into the complete model. Amount blends each channel's displacement from zero to full correction; green stays fixed.

The Rust tests check extracted/interpolated coefficients, actual channel coordinates, and complete renderer output on a three-channel analytic image at Amount 0/50/100. Lensfun is not needed to run those tests: the recorded measurements are committed. Code and synthetic fixtures are original RapidRoom work under the repository licence; Lensfun's independent reference implementation is credited here, with no library implementation or native wrapper copied.


## Bundled Sony trip-lens data

Run the same generator with --trip to capture trip-coordinates.json from the repository's mil-sony.xml through Lensfun's public C API. The file binds the database SHA256 and contains 48 actual-data coefficient/channel-coordinate records for the FE 35mm F1.4 GM and FE 24-70mm F2.8 GM II at full A7C II dimensions. The prime has constant-only poly3 data; the zoom has nonzero radial terms and duplicate 24/70mm anchors. The external measurements establish Lensfun's last-declaration-wins behavior for duplicate focal anchors, including interpolation and out-of-range endpoints. Rust reuses the same independent-coordinate assertions for these records. These are database/model checks, not photographs from those lenses or physical lens acceptance.
