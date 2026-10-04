# RapidRoom changelog

Everything RapidRoom adds on top of upstream [RapidRAW](https://github.com/CyberTimon/RapidRAW), with who made it and where it stands upstream.

<sub>Generated from [rapidroom/changes.json](rapidroom/changes.json) by `node rapidroom/status.mjs`; don't edit by hand.</sub>

**39 changes on top of RapidRAW.** 18 fix upstream issues that had been open a median of 68 days when RapidRoom shipped the fix; 17 of them still open upstream. 13 offered upstream as PRs, 3 merged so far.

| Change                                                                                                                                                                             | Type        | By                                                                                                                       | Upstream                                                                                                                                                                                                                                                                           |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Import old Lightroom `.lrtemplate` presets stored as plain Lua tables, without running any Lua                                                                                     | feature     | [@laurensiusadi](https://github.com/laurensiusadi)                                                                       | not yet offered                                                                                                                                                                                                                                                                    |
| White balance picker samples the original linear image (click for a small square, drag for an area) instead of an edited thumbnail, so picks are stable and correct                | fix         | [@lalibertemarc](https://github.com/lalibertemarc)                                                                       | [#1251](https://github.com/CyberTimon/RapidRAW/issues/1251) open 120 d; [#746](https://github.com/CyberTimon/RapidRAW/issues/746) open 219 d; [#1768](https://github.com/CyberTimon/RapidRAW/issues/1768) open 11 d                                                                |
| Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)                                                                                                       | platform    | [@yojen7](https://github.com/yojen7)                                                                                     | PR [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813) merged                                                                                                                                                                                                                |
| No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … \| head`)                                                                                                   | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | PR [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819) open                                                                                                                                                                                                                  |
| Exports embed an sRGB ICC profile (JPEG, PNG, TIFF), so colour-managed apps and print services read them correctly                                                                 | feature     | [@yojen7](https://github.com/yojen7)                                                                                     | [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 58 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) merged                                                                                                                                         |
| Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders ⚑                                                                                            | fix         | [@Kheil-Z](https://github.com/Kheil-Z)                                                                                   | [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 206 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open                                                                                                                            |
| The Poppins UI font is bundled: no Google Fonts request at startup, and the UI works offline                                                                                       | privacy     | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| The Clerk sign-in SDK loads only when cloud features are turned on, with telemetry off                                                                                             | privacy     | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| The update check looks at RapidRoom releases                                                                                                                                       | platform    | [@yojen7](https://github.com/yojen7)                                                                                     | RapidRoom only                                                                                                                                                                                                                                                                     |
| CI builds Android unsigned when no signing key is configured, so forks get green builds                                                                                            | ci          | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Edit and metadata sidecars (.rrdata, .rrexif, XMP) are saved atomically, so a crash, power loss or full disk can't leave a half-written edit                                       | fix         | [@sheldonxxxx](https://github.com/sheldonxxxx), [@yojen7](https://github.com/yojen7)                                     | not yet offered                                                                                                                                                                                                                                                                    |
| One unreadable field in a saved mask no longer removes every mask on the image                                                                                                     | fix         | [@pluja](https://github.com/pluja)                                                                                       | not yet offered                                                                                                                                                                                                                                                                    |
| A corrupt saved panel layout no longer erases all settings at startup                                                                                                              | fix         | [@pluja](https://github.com/pluja)                                                                                       | not yet offered                                                                                                                                                                                                                                                                    |
| The curve shader no longer indexes past the end of its point array                                                                                                                 | fix         | [@pluja](https://github.com/pluja)                                                                                       | not yet offered                                                                                                                                                                                                                                                                    |
| Float images go to the GPU without a full temporary copy (less memory per edit)                                                                                                    | performance | [@VailElla](https://github.com/VailElla)                                                                                 | not yet offered                                                                                                                                                                                                                                                                    |
| Keyboard focus is visible again on every button (Tab navigation), without rings on mouse clicks                                                                                    | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Export presets keep their TIFF bit depth and "set file timestamp from EXIF" settings                                                                                               | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1397](https://github.com/CyberTimon/RapidRAW/issues/1397) open 78 d; PR [#1834](https://github.com/CyberTimon/RapidRAW/pull/1834) open                                                                                                                                           |
| EXIF UserComment is written with its character-code header, so other apps read the whole comment                                                                                   | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1161](https://github.com/CyberTimon/RapidRAW/issues/1161) open 147 d                                                                                                                                                                                                             |
| The "maximum AI tags" setting is respected                                                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1609](https://github.com/CyberTimon/RapidRAW/issues/1609) open 39 d; PR [#1837](https://github.com/CyberTimon/RapidRAW/pull/1837) open                                                                                                                                           |
| "RAW only" with "prefer JPEG" grouping shows the RAW files instead of nothing                                                                                                      | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1454](https://github.com/CyberTimon/RapidRAW/issues/1454) open 65 d; PR [#1835](https://github.com/CyberTimon/RapidRAW/pull/1835) open                                                                                                                                           |
| The library and folder tree refresh after exporting into the source folder                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1674](https://github.com/CyberTimon/RapidRAW/issues/1674) open 31 d; PR [#1836](https://github.com/CyberTimon/RapidRAW/pull/1836) open                                                                                                                                           |
| Batch export no longer puts another photo into some outputs: colour and luminance masks are built from the image being exported, and masks are paired with the right adjustments ⚑ | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1571](https://github.com/CyberTimon/RapidRAW/issues/1571) closed after 43 d; PR [#1825](https://github.com/CyberTimon/RapidRAW/pull/1825) merged                                                                                                                                 |
| Exported JPEG, PNG and WebP files carry your tags as XMP keywords (dc:subject) when metadata is kept                                                                               | feature     | [@chuckhenrich](https://github.com/chuckhenrich), [@yojen7](https://github.com/yojen7)                                   | not yet offered                                                                                                                                                                                                                                                                    |
| RapidRoom name, logo, app icon and start-screen photo; the start screen credits RapidRAW as the upstream project                                                                   | platform    | [@yojen7](https://github.com/yojen7)                                                                                     | RapidRoom only                                                                                                                                                                                                                                                                     |
| Frontend test foundation: Vitest with a mocked Tauri API, run on every pull request                                                                                                | ci          | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Pasting adjustments works even when the settings failed to load (it used to do nothing)                                                                                            | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Rapid navigation shares one decode slot between editor loads and culling previews; superseded editor loads skip decoding                                                           | performance | [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7)                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Read-only Card mode: browse a memory card without RapidRoom creating, changing or deleting anything on it                                                                          | feature     | [@TomasLiutvinas](https://github.com/TomasLiutvinas), [@yojen7](https://github.com/yojen7)                               | not yet offered                                                                                                                                                                                                                                                                    |
| Previews and exports skip GPU blur passes that no active adjustment reads                                                                                                          | performance | [@SandeepSubba](https://github.com/SandeepSubba)                                                                         | not yet offered                                                                                                                                                                                                                                                                    |
| Star ratings set in the camera (embedded XMP or EXIF Rating) show in the library; a rating you set or clear in RapidRoom always wins                                               | feature     | [@csiroqa](https://github.com/csiroqa), [@masmoriya](https://github.com/masmoriya), [@yojen7](https://github.com/yojen7) | [#517](https://github.com/CyberTimon/RapidRAW/issues/517) open 293 d; [#1130](https://github.com/CyberTimon/RapidRAW/issues/1130) open 153 d; PR [#1529](https://github.com/CyberTimon/RapidRAW/pull/1529) open; PR [#1714](https://github.com/CyberTimon/RapidRAW/pull/1714) open |
| Pixel-exact regression check in CI: 18 renders on a software Vulkan renderer (Mesa lavapipe), compared by pixel hash                                                               | ci          | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| BM3D denoise: raising the strength no longer adds noise back ⚑                                                                                                                     | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1404](https://github.com/CyberTimon/RapidRAW/issues/1404) open 74 d                                                                                                                                                                                                              |
| Groundwork for camera-matching profiles: a bounds-checked reader for Adobe DCP files (not used by the app yet)                                                                     | feature     | [@harrytuckerr](https://github.com/harrytuckerr)                                                                         | not yet offered                                                                                                                                                                                                                                                                    |
| Culling: optional auto-advance to the next image after rating with a shortcut, and an "exactly N stars" rating filter                                                              | feature     | [@yojen7](https://github.com/yojen7)                                                                                     | [#1749](https://github.com/CyberTimon/RapidRAW/issues/1749) open 15 d; [#1583](https://github.com/CyberTimon/RapidRAW/issues/1583) open 42 d                                                                                                                                       |
| Import Lightroom and Camera Raw XMP sidecars: Basic, HSL, colour grading, curves, vignette, crop and straighten, rating, label and keywords, for one photo or a whole folder tree  | feature     | [@dimafa](https://github.com/dimafa), [@StephenMasseur](https://github.com/StephenMasseur)                               | PR [#1465](https://github.com/CyberTimon/RapidRAW/pull/1465) open                                                                                                                                                                                                                  |
| Window and DPI changes keep the latest preview size even when a render holds the display lock                                                                                      | fix         | [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7)                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Editor render caches include virtual image identity so a late preview cannot reuse another photo                                                                                   | fix         | [@mlauziertr](https://github.com/mlauziertr), [@yojen7](https://github.com/yojen7)                                       | not yet offered                                                                                                                                                                                                                                                                    |
| Linux large allocations no longer request transparent huge pages from mimalloc, avoiding compaction stalls on fragmented memory                                                    | performance | [@elhigu](https://github.com/elhigu)                                                                                     | PR [#1790](https://github.com/CyberTimon/RapidRAW/pull/1790) open                                                                                                                                                                                                                  |
| Failed headless CLI exports exit with status 1 and say why on stderr (they used to exit 0)                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Linux release packaging: a .deb and an AppImage named RapidRoom that install side by side with RapidRAW                                                                            | platform    | [@yojen7](https://github.com/yojen7)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Denoise dialogs stay open on busy backdrop clicks; Cancel stops waiting and discards the eventual UI result                                                                        | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1697](https://github.com/CyberTimon/RapidRAW/issues/1697) open 28 d                                                                                                                                                                                                              |
| DxO compressed DNG highlights no longer wrap to black dots when lookup-table dithering exceeds 16 bits ⚑                                                                           | fix         | [@yojen7](https://github.com/yojen7)                                                                                     | [#1119](https://github.com/CyberTimon/RapidRAW/issues/1119) open 156 d                                                                                                                                                                                                             |

⚑ changes rendered output on purpose. Upstream status as of 2026-10-04.

## Details

### Import old Lightroom `.lrtemplate` presets stored as plain Lua tables, without running any Lua

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@laurensiusadi](https://github.com/laurensiusadi), from laurensiusadi/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [bb60210](https://github.com/laurensiusadi/RapidRAW/commit/bb60210c609c6939d34deba9c1043fe9d01bde92)
- **Notes:** Rewritten from the fork's regex converter as a bounded Lua-table parser that feeds the existing XMP preset mapping, so values (including Shadows2012 1:1) match .xmp presets. Settings nested in Looks or local corrections are ignored, camera/lens profiles and local corrections are reported as not imported, and malformed files or curves are rejected with a reason. Importing only adds a preset; photos change only when you apply it. Harvested with Claude Code.

### White balance picker samples the original linear image (click for a small square, drag for an area) instead of an edited thumbnail, so picks are stable and correct

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), from lalibertemarc/RapidRAW
- **Upstream:** [#1251](https://github.com/CyberTimon/RapidRAW/issues/1251) open 120 d; [#746](https://github.com/CyberTimon/RapidRAW/issues/746) open 219 d; [#1768](https://github.com/CyberTimon/RapidRAW/issues/1768) open 11 d
- **Commits:** [b41aa56](https://github.com/lalibertemarc/RapidRAW/commit/b41aa56622518ca33526139520011fed73ca129e)
- **Notes:** Rendering pipeline unchanged; only the temperature/tint values the picker produces differ. A hover swatch shows the sampled colour and the resulting temperature/tint.

### Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)

- **Type:** platform
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** PR [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813) merged

### No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … | head`)

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** PR [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819) open

### Exports embed an sRGB ICC profile (JPEG, PNG, TIFF), so colour-managed apps and print services read them correctly

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 58 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) merged
- **Notes:** Pixels unchanged; only the profile tag is added. The TIFF part goes upstream after #1752.

### Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@Kheil-Z](https://github.com/Kheil-Z), from CyberTimon/RapidRAW-DngLab#7 (rebased onto 934af4b by yojen7)
- **Upstream:** [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 206 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open
- **Commits:** [fe08395](https://github.com/yojen7/RapidRAW-DngLab/commit/fe0839580ece6aab0884ebb7d806f34ff01fb52b)
- **Notes:** Only the affected files change: their crop is now correct.

### The Poppins UI font is bundled: no Google Fonts request at startup, and the UI works offline

- **Type:** privacy
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered

### The Clerk sign-in SDK loads only when cloud features are turned on, with telemetry off

- **Type:** privacy
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered

### The update check looks at RapidRoom releases

- **Type:** platform
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** RapidRoom only

### CI builds Android unsigned when no signing key is configured, so forks get green builds

- **Type:** ci
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered

### Edit and metadata sidecars (.rrdata, .rrexif, XMP) are saved atomically, so a crash, power loss or full disk can't leave a half-written edit

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@sheldonxxxx](https://github.com/sheldonxxxx), [@yojen7](https://github.com/yojen7), from sheldonxxxx/RapidRAW (extended by yojen7 to newer upstream save paths)
- **Upstream:** not yet offered
- **Commits:** [b54be6f](https://github.com/sheldonxxxx/RapidRAW/commit/b54be6fef1e2533ecaf258143ad61fed6330244c)

### One unreadable field in a saved mask no longer removes every mask on the image

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@pluja](https://github.com/pluja), from pluja/RapidRAW-Fork
- **Upstream:** not yet offered
- **Commits:** [d4017d3](https://github.com/pluja/RapidRAW-Fork/commit/d4017d349e6c18eb7ffae73206d7ed4baed895a9)

### A corrupt saved panel layout no longer erases all settings at startup

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@pluja](https://github.com/pluja), from pluja/RapidRAW-Fork
- **Upstream:** not yet offered
- **Commits:** [3f9fc5e](https://github.com/pluja/RapidRAW-Fork/commit/3f9fc5e2e02488a68c2dcbc69967c8593bdc4fb2)

### The curve shader no longer indexes past the end of its point array

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@pluja](https://github.com/pluja), from pluja/RapidRAW-Fork
- **Upstream:** not yet offered
- **Commits:** [3eab73c](https://github.com/pluja/RapidRAW-Fork/commit/3eab73ca2b11840816d7d2236e4453795d7edeba)

### Float images go to the GPU without a full temporary copy (less memory per edit)

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@VailElla](https://github.com/VailElla), from VailElla/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [d6cda85](https://github.com/VailElla/RapidRAW/commit/d6cda855a5e250480b281cc681ab2f3cb04f478c)

### Keyboard focus is visible again on every button (Tab navigation), without rings on mouse clicks

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Made with OpenAI Codex. Fixes markallisongit/RapidRAW#30, which applies to upstream too.

### Export presets keep their TIFF bit depth and "set file timestamp from EXIF" settings

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1397](https://github.com/CyberTimon/RapidRAW/issues/1397) open 78 d; PR [#1834](https://github.com/CyberTimon/RapidRAW/pull/1834) open

### EXIF UserComment is written with its character-code header, so other apps read the whole comment

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1161](https://github.com/CyberTimon/RapidRAW/issues/1161) open 147 d

### The "maximum AI tags" setting is respected

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1609](https://github.com/CyberTimon/RapidRAW/issues/1609) open 39 d; PR [#1837](https://github.com/CyberTimon/RapidRAW/pull/1837) open

### "RAW only" with "prefer JPEG" grouping shows the RAW files instead of nothing

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1454](https://github.com/CyberTimon/RapidRAW/issues/1454) open 65 d; PR [#1835](https://github.com/CyberTimon/RapidRAW/pull/1835) open

### The library and folder tree refresh after exporting into the source folder

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1674](https://github.com/CyberTimon/RapidRAW/issues/1674) open 31 d; PR [#1836](https://github.com/CyberTimon/RapidRAW/pull/1836) open

### Batch export no longer puts another photo into some outputs: colour and luminance masks are built from the image being exported, and masks are paired with the right adjustments

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1571](https://github.com/CyberTimon/RapidRAW/issues/1571) closed after 43 d; PR [#1825](https://github.com/CyberTimon/RapidRAW/pull/1825) merged
- **Notes:** Only exports with colour/luminance masks (or an empty mask before another mask) change: they now match the editor. Prior art: R-Laine/RapidRAW@d49c341 (same path-keyed idea, different base).

### Exported JPEG, PNG and WebP files carry your tags as XMP keywords (dc:subject) when metadata is kept

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@chuckhenrich](https://github.com/chuckhenrich), [@yojen7](https://github.com/yojen7), from chuckhenrich/RapidRAW (adapted in RapidRoom: colour labels left out, user: prefix stripped, JPEG/WebP placement fixed)
- **Upstream:** not yet offered
- **Commits:** [8564c6c](https://github.com/chuckhenrich/RapidRAW/commit/8564c6c94240b15a35e96c7e5ae94ecc8c7b7e31)
- **Notes:** Only with "Keep metadata" on. Not yet for TIFF, AVIF or JXL exports, or for TIFF sources (no metadata is written for those today). The fork commit cites `#1618`, probably the upstream issue; not verified.

### RapidRoom name, logo, app icon and start-screen photo; the start screen credits RapidRAW as the upstream project

- **Type:** platform
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** RapidRoom only
- **Notes:** The app identifier (settings and data location) is unchanged, so existing RapidRAW settings, presets and library carry over. The start-screen photo is all rights reserved (not AGPL).

### Frontend test foundation: Vitest with a mocked Tauri API, run on every pull request

- **Type:** ci
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Covers settings saving (success and backend failure) and Tauri event listener cleanup. Written with Claude Code.

### Pasting adjustments works even when the settings failed to load (it used to do nothing)

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** From the type errors harrytuckerr's fork flagged as likely bugs (`harrytuckerr/RapidRAW#1`). The others (image cache, interactive patch, pinned folders, panel moves) were type-only and are fixed without behaviour changes. Written with Claude Code.

### Rapid navigation shares one decode slot between editor loads and culling previews; superseded editor loads skip decoding

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7), from SandeepSubba/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [60e2a0a](https://github.com/SandeepSubba/RapidRAW/commit/60e2a0a840e8056184c65d5ad606e5a8daf1ad09), [dd4ac66](https://github.com/SandeepSubba/RapidRAW/commit/dd4ac6619b15fc141da98f03d4f3bdcd0aecd4cf)
- **Notes:** Editor and culling share a one-permit semaphore. Blocking tasks retain the permit if their awaiting task is cancelled. Culling requests are serialized, not cancelled.

### Read-only Card mode: browse a memory card without RapidRoom creating, changing or deleting anything on it

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@TomasLiutvinas](https://github.com/TomasLiutvinas), [@yojen7](https://github.com/yojen7), from TomasLiutvinas/RapidRAW (extended by yojen7 to every write path)
- **Upstream:** not yet offered
- **Commits:** [f5197a7](https://github.com/TomasLiutvinas/RapidRAW/commit/f5197a74386608b2bf0125b61b39fa88e97903b8), [ac1543c](https://github.com/TomasLiutvinas/RapidRAW/commit/ac1543cc672db8382bf507aad50cc52f0596255c)
- **Notes:** The backend refuses every write under the card folder: sidecars, XMP sync and creation, EXIF refresh, tag cleanup, rename, move, copy into, delete, duplicates, virtual copies, and outputs saved next to the originals (export, denoise, HDR, panorama, focus stack, collage, negatives). Edits stay in memory. Written with Claude Code.

### Previews and exports skip GPU blur passes that no active adjustment reads

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@SandeepSubba](https://github.com/SandeepSubba), from SandeepSubba/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [a27a87b](https://github.com/SandeepSubba/RapidRAW/commit/a27a87bf55546e930cfb6689c4cf39028e570b2d)
- **Notes:** Adapted to RapidRoom's shader with Claude Code: sharpening also reads the tonal blur, highlights doesn't, and RapidRoom has no skin smoothing. Output should be pixel-identical; speed-up not measured yet.

### Star ratings set in the camera (embedded XMP or EXIF Rating) show in the library; a rating you set or clear in RapidRoom always wins

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@csiroqa](https://github.com/csiroqa), [@masmoriya](https://github.com/masmoriya), [@yojen7](https://github.com/yojen7), from csiroqa/RapidRAW (upstream PR, adapted by yojen7 with the explicit-clear idea from masmoriya/RapidRAW)
- **Upstream:** [#517](https://github.com/CyberTimon/RapidRAW/issues/517) open 293 d; [#1130](https://github.com/CyberTimon/RapidRAW/issues/1130) open 153 d; PR [#1529](https://github.com/CyberTimon/RapidRAW/pull/1529) open; PR [#1714](https://github.com/CyberTimon/RapidRAW/pull/1714) open
- **Commits:** [feda53d](https://github.com/csiroqa/RapidRAW/commit/feda53dbbc8daf543fe539451e653e24f1843cd4), [c035b79](https://github.com/csiroqa/RapidRAW/commit/c035b79f110fa09fff443a0c109ffa1544a30124), [8392d6c](https://github.com/masmoriya/RapidRAW/commit/8392d6c9eb55a630d64f176fd833f636aabc33db)
- **Notes:** Reads only the file headers it needs (TIFF-based raws such as Sony ARW, JPEG, Fuji RAF, Canon CR3); never writes a sidecar. A rating cleared before this change looks the same as one never set, so the camera rating shows until it is cleared again. The idea that an empty XMP sidecar must not hide the camera rating comes from moschmdt (upstream PR 1714).

### Pixel-exact regression check in CI: 18 renders on a software Vulkan renderer (Mesa lavapipe), compared by pixel hash

- **Type:** ci
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Runs on pull requests touching src-tauri or the regression harness, on pushes to main and on demand; not a required check yet. Each job renders twice to prove lavapipe is deterministic. The reference (rapidroom/regression/ci-reference.json) is recorded on the runner and committed by a maintainer. Written with Claude Code.

### BM3D denoise: raising the strength no longer adds noise back

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1404](https://github.com/CyberTimon/RapidRAW/issues/1404) open 74 d
- **Notes:** The detail blend re-added up to 50% of the original high frequencies, noise included, and grew with strength. It now matches the old blend up to 33% strength and fades to zero at 100%. Only BM3D denoise output above 33% strength changes; the edit pipeline and its regression renders are untouched. Unit tests check that RMSE against a clean image doesn't rise with strength. Written with Claude Code.

### Groundwork for camera-matching profiles: a bounds-checked reader for Adobe DCP files (not used by the app yet)

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@harrytuckerr](https://github.com/harrytuckerr), from harrytuckerr/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [f9a2c72](https://github.com/harrytuckerr/RapidRAW/commit/f9a2c72c2740f271593ac08aef6ddd84b0ed3574)
- **Notes:** Parser and its synthetic test fixtures only (round-trip, ExtraCameraProfiles, a 10,000-input mutation test); nothing in the render path calls it yet, so output is unchanged. The fork's acceptance test against one vendor file on the author's machine was dropped. Harvested with Claude Code.

### Culling: optional auto-advance to the next image after rating with a shortcut, and an "exactly N stars" rating filter

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1749](https://github.com/CyberTimon/RapidRAW/issues/1749) open 15 d; [#1583](https://github.com/CyberTimon/RapidRAW/issues/1583) open 42 d
- **Notes:** Auto-advance is off by default (Settings → General) and only moves on when a single image is rated with the 0–5 keys; it stops at the last image. The ≥/= button next to the rating filter stars switches between "N and up" and "exactly N". Written with Claude Code.

### Import Lightroom and Camera Raw XMP sidecars: Basic, HSL, colour grading, curves, vignette, crop and straighten, rating, label and keywords, for one photo or a whole folder tree

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@dimafa](https://github.com/dimafa), [@StephenMasseur](https://github.com/StephenMasseur), from dimafa/RapidRAW (upstream PR 1465, which builds on StephenMasseur's PR 1280)
- **Upstream:** PR [#1465](https://github.com/CyberTimon/RapidRAW/pull/1465) open
- **Commits:** [7a6913f](https://github.com/dimafa/RapidRAW/commit/7a6913f563a7ed6a3e9493d8f49471622c8caf76), [b3d0c52](https://github.com/dimafa/RapidRAW/commit/b3d0c52472c77b7d93c7774ec663df0341a6a772)
- **Notes:** Merged with every original commit. Right-click a photo and choose Import XMP Adjustments, or a folder and choose Import Matching XMP Sidecars. The photos are not changed; with XMP sync on, rating, label and keywords are written back to the .xmp as after any edit. An imported Lightroom rating counts as one set in RapidRoom, so it wins over the camera's rating, even when it is 0. RapidRoom routes the sidecar writes through its atomic writer and refuses imports onto a card opened in Card mode. The PR also reworks the existing .xmp preset import (Shadows2012 is now copied 1:1 instead of x1.5, nested Looks are ignored, PV2003/2010 values are read). Lightroom and RapidRAW render differently, so similar values don't give identical images; mapping fixes follow in RapidRoom #54. Harvested with Claude Code.

### Window and DPI changes keep the latest preview size even when a render holds the display lock

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7), from SandeepSubba/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [1084ca9](https://github.com/SandeepSubba/RapidRAW/commit/1084ca96fbad8ea1c64dd58d6a26df06fdf3df60)
- **Notes:** The latest physical surface size is queued outside the display lock and applied on the next transform, resize, or native preview render. Export processing is unchanged.

### Editor render caches include virtual image identity so a late preview cannot reuse another photo

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@mlauziertr](https://github.com/mlauziertr), [@yojen7](https://github.com/yojen7), from mlauziertr/picportal-editor
- **Upstream:** not yet offered
- **Commits:** [8ec31d4](https://github.com/mlauziertr/picportal-editor/commit/8ec31d4f4fcddc50b6217d9d6419d7278d2937ec)
- **Notes:** Transformed, patched/warped and small-preview cache keys include the full virtual image path. The existing path guard and mask-image snapshot from the batch-export fix are retained; its already image-keyed cache uses the equivalent shared helper. No image-processing math changes.

### Linux large allocations no longer request transparent huge pages from mimalloc, avoiding compaction stalls on fragmented memory

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@elhigu](https://github.com/elhigu), from CyberTimon/RapidRAW
- **Upstream:** PR [#1790](https://github.com/CyberTimon/RapidRAW/pull/1790) open
- **Commits:** [a742450](https://github.com/CyberTimon/RapidRAW/commit/a7424509bba7ed44154fa283b1c3d41abf982c5a)
- **Notes:** Allocation-only change: enable mimalloc no_thp and retain the Linux smaps regression test. The allocator version and image-processing math are unchanged. The remaining GPU logging, benchmark and parallel conversion work is tracked in RapidRoom issue #66.

### Failed headless CLI exports exit with status 1 and say why on stderr (they used to exit 0)

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** The run-loop exit handler ignored the requested exit code and always called exit(0). A CI job runs the binary under xvfb for a missing input and an unwritable output. Written with Claude Code.

### Linux release packaging: a .deb and an AppImage named RapidRoom that install side by side with RapidRAW

- **Type:** platform
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Binary `rapidroom`, resources in /usr/lib/RapidRoom, RapidRoom.desktop, window class RapidRoom, and its own single-instance D-Bus name. The app identifier (and so the settings folder) is still shared with RapidRAW. A `v*` tag builds a draft release; see rapidroom/RELEASING.md. Written with Claude Code.

### Denoise dialogs stay open on busy backdrop clicks; Cancel stops waiting and discards the eventual UI result

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1697](https://github.com/CyberTimon/RapidRAW/issues/1697) open 28 d
- **Notes:** Another denoise cannot start while a command is pending. Cancellation does not interrupt the backend: single processing may continue and batch work may still write files. Real interruption and job IDs are tracked in RapidRoom #68.

### DxO compressed DNG highlights no longer wrap to black dots when lookup-table dithering exceeds 16 bits

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7), from yojen7/RapidRAW-DngLab
- **Upstream:** [#1119](https://github.com/CyberTimon/RapidRAW/issues/1119) open 156 d
- **Commits:** [5a44585](https://github.com/yojen7/RapidRAW-DngLab/commit/5a44585da25c7ff783c83a4bff25281f0cf28159), [4c96062](https://github.com/yojen7/RapidRAW-DngLab/commit/4c960626978287ab8e4aabe70a6cfa60b4582273)
- **Notes:** Dither values above 65535 now saturate instead of wrapping. Unsaturated values and the random-state update are preserved. The fork retains Kheil-Z's Sony/Canon default-crop fixes; the extra tile-fixture change is test-only.
