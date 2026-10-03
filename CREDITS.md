# Credits

RapidRoom is built on other people's work. This file lists where every non-upstream change came from. Upstream's own thanks are in the [RapidRAW README](README.md#special-thanks).

## Upstream

- **[RapidRAW](https://github.com/CyberTimon/RapidRAW)** by Timon Käch ([@CyberTimon](https://github.com/CyberTimon)) and its contributors. This is the base of everything here.
- **[RapidRAW-DngLab](https://github.com/CyberTimon/RapidRAW-DngLab)**, CyberTimon's fork of [dnglab/rawler](https://github.com/dnglab/dnglab) by Daniel Vogelbacher and contributors (raw decoding).

## Harvested changes

| Change                                                                 | Author                                         | Source                                                                                                                                                                                                                                                                                                                         |
| ---------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Default-crop fix for non-CFA raws (Sony lossless M/S, Canon mRAW/sRAW) | [@Kheil-Z](https://github.com/Kheil-Z)         | [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7), rebased onto `934af4b` at [yojen7/RapidRAW-DngLab@fe08395](https://github.com/yojen7/RapidRAW-DngLab/commit/fe08395)                                                                                                                                |
| Atomic sidecar writes                                                  | [@sheldonxxxx](https://github.com/sheldonxxxx) | [sheldonxxxx/RapidRAW@b54be6f](https://github.com/sheldonxxxx/RapidRAW/commit/b54be6fef1e2533ecaf258143ad61fed6330244c)                                                                                                                                                                                                        |
| Mask recovery, corrupt panel layout guard, curve index fix             | [@pluja](https://github.com/pluja)             | [d4017d3](https://github.com/pluja/RapidRAW-Fork/commit/d4017d349e6c18eb7ffae73206d7ed4baed895a9), [3f9fc5e](https://github.com/pluja/RapidRAW-Fork/commit/3f9fc5e2e02488a68c2dcbc69967c8593bdc4fb2), [3eab73c](https://github.com/pluja/RapidRAW-Fork/commit/3eab73ca2b11840816d7d2236e4453795d7edeba) in pluja/RapidRAW-Fork |
| GPU float upload without a temporary copy                              | [@VailElla](https://github.com/VailElla)       | [VailElla/RapidRAW@d6cda85](https://github.com/VailElla/RapidRAW/commit/d6cda855a5e250480b281cc681ab2f3cb04f478c)                                                                                                                                                                                                              |

## RapidRoom changes

| Change                               | Author                                             | Upstream PR                                                                                                                                                                                    |
| ------------------------------------ | -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri 2.12                           | [@yojen7](https://github.com/yojen7) (AI-assisted) | [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813), merged                                                                                                                              |
| Closed-pipe abort/hang fix           | @yojen7 (AI-assisted)                              | [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819)                                                                                                                                      |
| sRGB ICC profile in exports          | @yojen7 (AI-assisted)                              | [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820); TIFF part to follow [#1752](https://github.com/CyberTimon/RapidRAW/pull/1752) by [@FabianWeiss90](https://github.com/FabianWeiss90) |
| Bundled Poppins font                 | @yojen7 (AI-assisted)                              | not yet proposed                                                                                                                                                                               |
| Clerk loaded only for cloud features | @yojen7 (AI-assisted)                              | not yet proposed                                                                                                                                                                               |

## Third-party assets

- **Start-screen photograph** `public/splash-rapidroom.jpg`: © 2025 [@yojen7](https://github.com/yojen7), **all rights reserved**. It is not covered by the AGPL and is included with permission for RapidRoom only. Forks must replace it; see `public/splash-rapidroom.jpg.license`.
- **RapidRoom logo** (`.github/assets/rapidroom-logo.png`, `public/rapidroom-logo.png`, app icons): by @yojen7 for RapidRoom. Text set in Liberation Sans Bold (SIL OFL 1.1).
- **sRGB ICC profile** `src-tauri/icc/sRGB-v2-magic.icc`: from [saucecontrol/Compact-ICC-Profiles](https://github.com/saucecontrol/Compact-ICC-Profiles), CC0-1.0.
- **Poppins font**: by the Indian Type Foundry, SIL Open Font License 1.1, bundled via [@fontsource/poppins](https://fontsource.org/fonts/poppins).

## Forks we follow

These RapidRAW forks have work we'd like to harvest (with credit) or coordinate with. Being listed here doesn't mean any of their code is in RapidRoom yet. Code moves to the tables above when it lands.

- [sheldonxxxx/RapidRAW](https://github.com/sheldonxxxx/RapidRAW): MCP server, Nonlocal raw denoise, atomic sidecars, preview performance
- [RustRunner/RapidRAW-MKII](https://github.com/RustRunner/RapidRAW-MKII): offline build, low-light and blur recovery
- [vinioliveiras/RapidRAW](https://github.com/vinioliveiras/RapidRAW): colour pipeline fixes against Lightroom
- [cgasgarth/RapidRaw](https://github.com/cgasgarth/RapidRaw): MCP kept in sync with upstream
- [ssarangi/RapidRAW](https://github.com/ssarangi/RapidRAW): interactive raw performance
- [NicoNex/RapidRAW](https://github.com/NicoNex/RapidRAW): native GTK front end, core split
- [pluja/RapidRAW-Fork](https://github.com/pluja/RapidRAW-Fork): bug fixes, tests
- [cl1x/RapidRAW](https://github.com/cl1x/RapidRAW): Immich integration
