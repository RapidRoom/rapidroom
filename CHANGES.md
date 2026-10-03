# RapidRoom changelog

Everything RapidRoom adds on top of upstream [RapidRAW](https://github.com/CyberTimon/RapidRAW), with who made it and where it stands upstream.

<sub>Generated from [rapidroom/changes.json](rapidroom/changes.json) by `node rapidroom/status.mjs`; don't edit by hand.</sub>

**24 changes on top of RapidRAW.** 8 fix upstream issues that had been open a median of 60 days when RapidRoom shipped the fix; 8 of them still open upstream. 4 offered upstream as PRs, 1 merged so far.

| Change                                                                                                                                                                             | Type        | By                                                                                         | Upstream                                                                                                                                                |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)                                                                                                       | platform    | [@yojen7](https://github.com/yojen7)                                                       | PR [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813) merged                                                                                     |
| No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … \| head`)                                                                                                   | fix         | [@yojen7](https://github.com/yojen7)                                                       | PR [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819) open                                                                                       |
| Exports embed an sRGB ICC profile (JPEG, PNG, TIFF), so colour-managed apps and print services read them correctly                                                                 | feature     | [@yojen7](https://github.com/yojen7)                                                       | [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 57 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) open                |
| Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders ⚑                                                                                            | fix         | [@Kheil-Z](https://github.com/Kheil-Z)                                                     | [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 205 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open |
| The Poppins UI font is bundled: no Google Fonts request at startup, and the UI works offline                                                                                       | privacy     | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| The Clerk sign-in SDK loads only when cloud features are turned on, with telemetry off                                                                                             | privacy     | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| The update check looks at RapidRoom releases                                                                                                                                       | platform    | [@yojen7](https://github.com/yojen7)                                                       | RapidRoom only                                                                                                                                          |
| CI builds Android unsigned when no signing key is configured, so forks get green builds                                                                                            | ci          | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| Edit and metadata sidecars (.rrdata, .rrexif, XMP) are saved atomically, so a crash, power loss or full disk can't leave a half-written edit                                       | fix         | [@sheldonxxxx](https://github.com/sheldonxxxx), [@yojen7](https://github.com/yojen7)       | not yet offered                                                                                                                                         |
| One unreadable field in a saved mask no longer removes every mask on the image                                                                                                     | fix         | [@pluja](https://github.com/pluja)                                                         | not yet offered                                                                                                                                         |
| A corrupt saved panel layout no longer erases all settings at startup                                                                                                              | fix         | [@pluja](https://github.com/pluja)                                                         | not yet offered                                                                                                                                         |
| The curve shader no longer indexes past the end of its point array                                                                                                                 | fix         | [@pluja](https://github.com/pluja)                                                         | not yet offered                                                                                                                                         |
| Float images go to the GPU without a full temporary copy (less memory per edit)                                                                                                    | performance | [@VailElla](https://github.com/VailElla)                                                   | not yet offered                                                                                                                                         |
| Keyboard focus is visible again on every button (Tab navigation), without rings on mouse clicks                                                                                    | fix         | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| Export presets keep their TIFF bit depth and "set file timestamp from EXIF" settings                                                                                               | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1397](https://github.com/CyberTimon/RapidRAW/issues/1397) open 77 d                                                                                   |
| EXIF UserComment is written with its character-code header, so other apps read the whole comment                                                                                   | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1161](https://github.com/CyberTimon/RapidRAW/issues/1161) open 146 d                                                                                  |
| The "maximum AI tags" setting is respected                                                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1609](https://github.com/CyberTimon/RapidRAW/issues/1609) open 38 d                                                                                   |
| "RAW only" with "prefer JPEG" grouping shows the RAW files instead of nothing                                                                                                      | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1454](https://github.com/CyberTimon/RapidRAW/issues/1454) open 64 d                                                                                   |
| The library and folder tree refresh after exporting into the source folder                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1674](https://github.com/CyberTimon/RapidRAW/issues/1674) open 30 d                                                                                   |
| Batch export no longer puts another photo into some outputs: colour and luminance masks are built from the image being exported, and masks are paired with the right adjustments ⚑ | fix         | [@yojen7](https://github.com/yojen7)                                                       | [#1571](https://github.com/CyberTimon/RapidRAW/issues/1571) open 43 d                                                                                   |
| Exported JPEG, PNG and WebP files carry your tags as XMP keywords (dc:subject) when metadata is kept                                                                               | feature     | [@chuckhenrich](https://github.com/chuckhenrich), [@yojen7](https://github.com/yojen7)     | not yet offered                                                                                                                                         |
| RapidRoom name, logo, app icon and start-screen photo; the start screen credits RapidRAW as the upstream project                                                                   | platform    | [@yojen7](https://github.com/yojen7)                                                       | RapidRoom only                                                                                                                                          |
| Frontend test foundation: Vitest with a mocked Tauri API, run on every pull request                                                                                                | ci          | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| Pasting adjustments works even when the settings failed to load (it used to do nothing)                                                                                            | fix         | [@yojen7](https://github.com/yojen7)                                                       | not yet offered                                                                                                                                         |
| Rapid navigation shares one decode slot between editor loads and culling previews; superseded editor loads skip decoding                                                           | performance | [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7)             | not yet offered                                                                                                                                         |
| Read-only Card mode: browse a memory card without RapidRoom creating, changing or deleting anything on it                                                                          | feature     | [@TomasLiutvinas](https://github.com/TomasLiutvinas), [@yojen7](https://github.com/yojen7) | not yet offered                                                                                                                                         |

⚑ changes rendered output on purpose. Upstream status as of 2026-10-03.

## Details

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
- **Upstream:** [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 57 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) open
- **Notes:** Pixels unchanged; only the profile tag is added. The TIFF part goes upstream after #1752.

### Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@Kheil-Z](https://github.com/Kheil-Z), from CyberTimon/RapidRAW-DngLab#7 (rebased onto 934af4b by yojen7)
- **Upstream:** [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 205 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open
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
- **Upstream:** [#1397](https://github.com/CyberTimon/RapidRAW/issues/1397) open 77 d

### EXIF UserComment is written with its character-code header, so other apps read the whole comment

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1161](https://github.com/CyberTimon/RapidRAW/issues/1161) open 146 d

### The "maximum AI tags" setting is respected

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1609](https://github.com/CyberTimon/RapidRAW/issues/1609) open 38 d

### "RAW only" with "prefer JPEG" grouping shows the RAW files instead of nothing

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1454](https://github.com/CyberTimon/RapidRAW/issues/1454) open 64 d

### The library and folder tree refresh after exporting into the source folder

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1674](https://github.com/CyberTimon/RapidRAW/issues/1674) open 30 d

### Batch export no longer puts another photo into some outputs: colour and luminance masks are built from the image being exported, and masks are paired with the right adjustments

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-02
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1571](https://github.com/CyberTimon/RapidRAW/issues/1571) open 43 d
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
