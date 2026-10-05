# RapidRoom changelog

Everything RapidRoom adds on top of upstream [RapidRAW](https://github.com/CyberTimon/RapidRAW), with who made it and where it stands upstream.

<sub>Generated from [rapidroom/changes.json](rapidroom/changes.json) by `node rapidroom/status.mjs`; don't edit by hand.</sub>

**75 changes on top of RapidRAW.** 19 fix upstream issues that had been open a median of 63 days when RapidRoom shipped the fix; 18 of them still open upstream. 14 offered upstream as PRs, 3 merged so far.

| Change                                                                                                                                                                                                                                                                            | Type        | By                                                                                                                             | Upstream                                                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Keep Library and Editor content inside resized docks and give the terminal one compact header                                                                                                                                                                                     | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Include MCP and its adapter in Linux packages with default-off AI control and assistant launch consent                                                                                                                                                                            | platform    | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Read-only MCP contact sheets, exact clipping/color statistics and native-resolution detail crops                                                                                                                                                                                  | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Label AI edits in shared undo history and expose editor context, generated adjustment schema and bounded original/comparison/region previews                                                                                                                                      | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Movable terminal panel with persistent tabs, bounded PTY output, shell/font preferences and built-in/external assistant launch controls                                                                                                                                           | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Keep webview scripts local and disable Cloud sign-in before adding the terminal panel                                                                                                                                                                                             | privacy     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Fix intermittent green OM System/Olympus RAW exports by reading image metadata from its correct directory                                                                                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Faster RAW library thumbnails: unedited raws use the smallest sufficient embedded preview, edited linear DNGs render from their embedded proxy                                                                                                                                    | performance | [@SebastianEggli](https://github.com/SebastianEggli)                                                                           | PR [#1809](https://github.com/CyberTimon/RapidRAW/issues/1809); PR [RapidRAW-DngLab#10](https://github.com/CyberTimon/RapidRAW-DngLab/issues/10)                                                                                                                                   |
| Apply Adobe lossy DNG polynomial mappings before developing the full image, fixing pink and blown-out photos ⚑                                                                                                                                                                    | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1542](https://github.com/CyberTimon/RapidRAW/issues/1542)                                                                                                                                                                                                                        |
| Preview and selectively import Lightroom catalog photo edits, crop, straighten, rotation, star ratings and virtual copies, with unsupported-setting reports and independent replacement choices                                                                                   | feature     | [@laurensiusadi](https://github.com/laurensiusadi), [@yojen7](https://github.com/yojen7)                                       | not yet offered                                                                                                                                                                                                                                                                    |
| Import Lightroom Classic collections and collection sets from a .lrcat catalog as albums, with a preview of found and missing photos and relinking of moved root folders                                                                                                          | feature     | [@Bennyyy27](https://github.com/Bennyyy27)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Faster raw decoding: highlight recovery and pixel-format conversions run on all CPU cores; the GPU adapter is logged; `rapidraw bench` measures the image pipeline without a window                                                                                               | performance | [@elhigu](https://github.com/elhigu)                                                                                           | PR [#1790](https://github.com/CyberTimon/RapidRAW/pull/1790) open                                                                                                                                                                                                                  |
| Batch rename treats a photo as one unit: RAW+JPEG/HEIF pairs and every sidecar (.rrdata incl. virtual copies, both XMP styles, .acr, .dop, .pp3) move together, with metadata, burst and group tokens, a preview with collision checks, a rollback-safe two-phase rename and undo | feature     | [@yojen7](https://github.com/yojen7), [@SandeepSubba](https://github.com/SandeepSubba)                                         | PR [#1309](https://github.com/CyberTimon/RapidRAW/issues/1309)                                                                                                                                                                                                                     |
| Lightroom XMP import: lens profile turns on lens correction, profile-look tone curves are kept, dead keys are dropped, and an import report lists what was not transferred ⚑                                                                                                      | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Editor Reference View: pin a read-only reference photo beside the image you are editing (Shift+R)                                                                                                                                                                                 | feature     | [@cgasgarth](https://github.com/cgasgarth)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |
| White balance picker samples the original linear image (click for a small square, drag for an area) instead of an edited thumbnail, so picks are stable and correct                                                                                                               | fix         | [@lalibertemarc](https://github.com/lalibertemarc)                                                                             | [#1251](https://github.com/CyberTimon/RapidRAW/issues/1251) open 120 d; [#746](https://github.com/CyberTimon/RapidRAW/issues/746) open 219 d; [#1768](https://github.com/CyberTimon/RapidRAW/issues/1768) open 11 d                                                                |
| Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)                                                                                                                                                                                                      | platform    | [@yojen7](https://github.com/yojen7)                                                                                           | PR [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813) merged                                                                                                                                                                                                                |
| No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … \| head`)                                                                                                                                                                                                  | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | PR [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819) open                                                                                                                                                                                                                  |
| Exports embed an sRGB ICC profile (JPEG, PNG, TIFF), so colour-managed apps and print services read them correctly                                                                                                                                                                | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 58 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) merged                                                                                                                                         |
| Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders ⚑                                                                                                                                                                                           | fix         | [@Kheil-Z](https://github.com/Kheil-Z)                                                                                         | [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 206 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open                                                                                                                            |
| The Poppins UI font is bundled: no Google Fonts request at startup, and the UI works offline                                                                                                                                                                                      | privacy     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| The Clerk sign-in SDK loads only when cloud features are turned on, with telemetry off                                                                                                                                                                                            | privacy     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| The update check looks at RapidRoom releases                                                                                                                                                                                                                                      | platform    | [@yojen7](https://github.com/yojen7)                                                                                           | RapidRoom only                                                                                                                                                                                                                                                                     |
| CI builds Android unsigned when no signing key is configured, so forks get green builds                                                                                                                                                                                           | ci          | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Edit and metadata sidecars (.rrdata, .rrexif, XMP) are saved atomically, so a crash, power loss or full disk can't leave a half-written edit                                                                                                                                      | fix         | [@sheldonxxxx](https://github.com/sheldonxxxx), [@yojen7](https://github.com/yojen7)                                           | not yet offered                                                                                                                                                                                                                                                                    |
| One unreadable field in a saved mask no longer removes every mask on the image                                                                                                                                                                                                    | fix         | [@pluja](https://github.com/pluja)                                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| A corrupt saved panel layout no longer erases all settings at startup                                                                                                                                                                                                             | fix         | [@pluja](https://github.com/pluja)                                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| The curve shader no longer indexes past the end of its point array                                                                                                                                                                                                                | fix         | [@pluja](https://github.com/pluja)                                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| Float images go to the GPU without a full temporary copy (less memory per edit)                                                                                                                                                                                                   | performance | [@VailElla](https://github.com/VailElla)                                                                                       | not yet offered                                                                                                                                                                                                                                                                    |
| Keyboard focus is visible again on every button (Tab navigation), without rings on mouse clicks                                                                                                                                                                                   | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Export presets keep their TIFF bit depth and "set file timestamp from EXIF" settings                                                                                                                                                                                              | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1397](https://github.com/CyberTimon/RapidRAW/issues/1397) open 78 d; PR [#1834](https://github.com/CyberTimon/RapidRAW/pull/1834) open                                                                                                                                           |
| EXIF UserComment is written with its character-code header, so other apps read the whole comment                                                                                                                                                                                  | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1161](https://github.com/CyberTimon/RapidRAW/issues/1161) open 147 d                                                                                                                                                                                                             |
| The "maximum AI tags" setting is respected                                                                                                                                                                                                                                        | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1609](https://github.com/CyberTimon/RapidRAW/issues/1609) open 39 d; PR [#1837](https://github.com/CyberTimon/RapidRAW/pull/1837) open                                                                                                                                           |
| "RAW only" with "prefer JPEG" grouping shows the RAW files instead of nothing                                                                                                                                                                                                     | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1454](https://github.com/CyberTimon/RapidRAW/issues/1454) open 65 d; PR [#1835](https://github.com/CyberTimon/RapidRAW/pull/1835) open                                                                                                                                           |
| The library and folder tree refresh after exporting into the source folder                                                                                                                                                                                                        | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1674](https://github.com/CyberTimon/RapidRAW/issues/1674) open 31 d; PR [#1836](https://github.com/CyberTimon/RapidRAW/pull/1836) open                                                                                                                                           |
| Batch export no longer puts another photo into some outputs: colour and luminance masks are built from the image being exported, and masks are paired with the right adjustments ⚑                                                                                                | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1571](https://github.com/CyberTimon/RapidRAW/issues/1571) closed after 43 d; PR [#1825](https://github.com/CyberTimon/RapidRAW/pull/1825) merged                                                                                                                                 |
| Exported JPEG, PNG and WebP files carry your tags as XMP keywords (dc:subject) when metadata is kept                                                                                                                                                                              | feature     | [@chuckhenrich](https://github.com/chuckhenrich), [@yojen7](https://github.com/yojen7)                                         | not yet offered                                                                                                                                                                                                                                                                    |
| RapidRoom name, logo, app icon and start-screen photo; the start screen credits RapidRAW as the upstream project                                                                                                                                                                  | platform    | [@yojen7](https://github.com/yojen7)                                                                                           | RapidRoom only                                                                                                                                                                                                                                                                     |
| Frontend test foundation: Vitest with a mocked Tauri API, run on every pull request                                                                                                                                                                                               | ci          | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Pasting adjustments works even when the settings failed to load (it used to do nothing)                                                                                                                                                                                           | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Rapid navigation shares one decode slot between editor loads and culling previews; superseded editor loads skip decoding                                                                                                                                                          | performance | [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7)                                                 | not yet offered                                                                                                                                                                                                                                                                    |
| Read-only Card mode: browse a memory card without RapidRoom creating, changing or deleting anything on it                                                                                                                                                                         | feature     | [@TomasLiutvinas](https://github.com/TomasLiutvinas), [@yojen7](https://github.com/yojen7)                                     | not yet offered                                                                                                                                                                                                                                                                    |
| Previews and exports skip GPU blur passes that no active adjustment reads                                                                                                                                                                                                         | performance | [@SandeepSubba](https://github.com/SandeepSubba)                                                                               | not yet offered                                                                                                                                                                                                                                                                    |
| Star ratings set in the camera (embedded XMP or EXIF Rating) show in the library; a rating you set or clear in RapidRoom always wins                                                                                                                                              | feature     | [@csiroqa](https://github.com/csiroqa), [@masmoriya](https://github.com/masmoriya), [@yojen7](https://github.com/yojen7)       | [#517](https://github.com/CyberTimon/RapidRAW/issues/517) open 293 d; [#1130](https://github.com/CyberTimon/RapidRAW/issues/1130) open 153 d; PR [#1529](https://github.com/CyberTimon/RapidRAW/pull/1529) open; PR [#1714](https://github.com/CyberTimon/RapidRAW/pull/1714) open |
| Pixel-exact regression check in CI: 18 renders on a software Vulkan renderer (Mesa lavapipe), compared by pixel hash                                                                                                                                                              | ci          | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| BM3D denoise: raising the strength no longer adds noise back ⚑                                                                                                                                                                                                                    | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1404](https://github.com/CyberTimon/RapidRAW/issues/1404) open 74 d                                                                                                                                                                                                              |
| Groundwork for camera-matching profiles: a bounds-checked reader for Adobe DCP files (not used by the app yet)                                                                                                                                                                    | feature     | [@harrytuckerr](https://github.com/harrytuckerr)                                                                               | not yet offered                                                                                                                                                                                                                                                                    |
| Culling: optional auto-advance to the next image after rating with a shortcut, and an "exactly N stars" rating filter                                                                                                                                                             | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | [#1749](https://github.com/CyberTimon/RapidRAW/issues/1749) open 15 d; [#1583](https://github.com/CyberTimon/RapidRAW/issues/1583) open 42 d                                                                                                                                       |
| Import Lightroom and Camera Raw XMP sidecars: Basic, HSL, colour grading, curves, vignette, crop and straighten, rating, label and keywords, for one photo or a whole folder tree                                                                                                 | feature     | [@dimafa](https://github.com/dimafa), [@StephenMasseur](https://github.com/StephenMasseur)                                     | PR [#1465](https://github.com/CyberTimon/RapidRAW/pull/1465) open                                                                                                                                                                                                                  |
| Window and DPI changes keep the latest preview size even when a render holds the display lock                                                                                                                                                                                     | fix         | [@subbajeu](https://github.com/subbajeu), [@yojen7](https://github.com/yojen7)                                                 | not yet offered                                                                                                                                                                                                                                                                    |
| Editor render caches include virtual image identity so a late preview cannot reuse another photo                                                                                                                                                                                  | fix         | [@mlauziertr](https://github.com/mlauziertr), [@yojen7](https://github.com/yojen7)                                             | not yet offered                                                                                                                                                                                                                                                                    |
| Linux large allocations no longer request transparent huge pages from mimalloc, avoiding compaction stalls on fragmented memory                                                                                                                                                   | performance | [@elhigu](https://github.com/elhigu)                                                                                           | PR [#1790](https://github.com/CyberTimon/RapidRAW/pull/1790) open                                                                                                                                                                                                                  |
| Failed headless CLI exports exit with status 1 and say why on stderr (they used to exit 0)                                                                                                                                                                                        | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Linux release packaging: a .deb and an AppImage named RapidRoom that install side by side with RapidRAW                                                                                                                                                                           | platform    | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Denoise dialogs stay open on busy backdrop clicks; Cancel stops waiting and discards the eventual UI result                                                                                                                                                                       | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1697](https://github.com/CyberTimon/RapidRAW/issues/1697) open 28 d                                                                                                                                                                                                              |
| DxO compressed DNG highlights no longer wrap to black dots when lookup-table dithering exceeds 16 bits ⚑                                                                                                                                                                          | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1119](https://github.com/CyberTimon/RapidRAW/issues/1119) open 156 d                                                                                                                                                                                                             |
| Cancelling a denoise stops the backend work, and each job's progress, preview and saved result are tied to its own job ID                                                                                                                                                         | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | [#1697](https://github.com/CyberTimon/RapidRAW/issues/1697) open 28 d                                                                                                                                                                                                              |
| LinearRaw gamma modes use the sRGB exponent 2.4 when removing gamma ⚑                                                                                                                                                                                                             | fix         | [@pluja](https://github.com/pluja)                                                                                             | PR [#1633](https://github.com/CyberTimon/RapidRAW/issues/1633)                                                                                                                                                                                                                     |
| Saturated red highlights use a continuous magenta correction instead of abrupt green lifts ⚑                                                                                                                                                                                      | fix         | [@3048mm](https://github.com/3048mm)                                                                                           | PR [#1824](https://github.com/CyberTimon/RapidRAW/issues/1824)                                                                                                                                                                                                                     |
| HSL mixer hue and saturation are evaluated in sRGB perceptual space while luminance stays linear ⚑                                                                                                                                                                                | fix         | [@lalibertemarc](https://github.com/lalibertemarc)                                                                             | [#1775](https://github.com/CyberTimon/RapidRAW/issues/1775); PR [#1777](https://github.com/CyberTimon/RapidRAW/issues/1777)                                                                                                                                                        |
| Sync upstream main: shared RAW embedded previews, Nikon lens metadata fallback and mask Escape/cache fixes ⚑                                                                                                                                                                      | fix         | [@lalibertemarc](https://github.com/lalibertemarc), [@CyberTimon](https://github.com/CyberTimon)                               | PR [#1823](https://github.com/CyberTimon/RapidRAW/issues/1823); PR [#1815](https://github.com/CyberTimon/RapidRAW/issues/1815); PR [#1827](https://github.com/CyberTimon/RapidRAW/issues/1827)                                                                                     |
| Inactive HSL preserves linear RGB exactly; saturated colours no longer turn grey under positive vibrance ⚑                                                                                                                                                                        | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Import old Lightroom `.lrtemplate` presets stored as plain Lua tables, without running any Lua                                                                                                                                                                                    | feature     | [@laurensiusadi](https://github.com/laurensiusadi)                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| Pick and reject flags, separate from star ratings: P / X / U shortcuts, badges, a flag filter (picked, unflagged, hide rejected, rejected) and Delete rejected                                                                                                                    | feature     | [@lalibertemarc](https://github.com/lalibertemarc), [@yojen7](https://github.com/yojen7)                                       | [#1599](https://github.com/CyberTimon/RapidRAW/issues/1599); [#994](https://github.com/CyberTimon/RapidRAW/issues/994); [#1057](https://github.com/CyberTimon/RapidRAW/issues/1057)                                                                                                |
| Presets move to the left sidebar with compact rows, hover previews, favorites and folder reordering                                                                                                                                                                               | feature     | [@laurensiusadi](https://github.com/laurensiusadi)                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| Color Grading and Color Mixer get their own visibility eye, globally and in masks; the Color panel eye still bypasses every colour tool                                                                                                                                           | feature     | [@lalibertemarc](https://github.com/lalibertemarc)                                                                             | PR [#1833](https://github.com/CyberTimon/RapidRAW/issues/1833)                                                                                                                                                                                                                     |
| Lights Out viewing in Library and Editor: L cycles Normal → Dim → Black, Shift+L goes back, Escape restores                                                                                                                                                                       | feature     | [@zeromeridian](https://github.com/zeromeridian), [@yojen7](https://github.com/yojen7)                                         | not yet offered                                                                                                                                                                                                                                                                    |
| Expanded Color Mixer: Hue, Saturation and Luminance tabs with all eight bands, and a pipette that adjusts the bands under the cursor as you drag up or down on the photo                                                                                                          | feature     | [@lalibertemarc](https://github.com/lalibertemarc)                                                                             | not yet offered                                                                                                                                                                                                                                                                    |
| Open Terminal Here: open your own terminal in a library folder, with RAPIDROOM_VERSION and RAPIDROOM_FOLDER set, so you can run Claude Code or Codex beside the editor                                                                                                            | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| RapidRoom skill and Claude Code plugin: editing workflow, adjustment reference generated from the code, tutor mode, source lookup and assistant-drafted issues                                                                                                                    | feature     | [@yojen7](https://github.com/yojen7), [@sheldonxxxx](https://github.com/sheldonxxxx)                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Optional compact adjustment sliders: label, track and value on one row, with aligned columns, in the Develop and mask panels                                                                                                                                                      | feature     | [@pluja](https://github.com/pluja), [@Bennyyy27](https://github.com/Bennyyy27)                                                 | not yet offered                                                                                                                                                                                                                                                                    |
| Sony 4:3, 1:1 and 16:9 camera framing opens as an editable crop ⚑                                                                                                                                                                                                                 | fix         | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Lens correction evaluates Lensfun profiles the way Lensfun does: right radius per model, rescaled terms instead of ×2.5, calibration that fits the sensor ⚑                                                                                                                       | fix         | [@beneedict](https://github.com/beneedict), [@yojen7](https://github.com/yojen7)                                               | [#1688](https://github.com/CyberTimon/RapidRAW/issues/1688); [#1689](https://github.com/CyberTimon/RapidRAW/issues/1689); PR [#1705](https://github.com/CyberTimon/RapidRAW/issues/1705)                                                                                           |
| Export recipes: export presets from the CLI (`--preset`), opt-in output sharpening for screen or print, and Instagram 4:5, 1:1 and 1.91:1 presets ⚑                                                                                                                               | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Native Linux UI smoke: exercise the release app, GPU preview, edits, Undo, Compact settings and JPEG export on an isolated compositor                                                                                                                                             | ci          | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| AI raw denoise: Best and Fast presets with optional sharpening, cached sensor output, and basic sliders under Quick noise reduction                                                                                                                                               | feature     | [@yojen7](https://github.com/yojen7), [@rymuelle](https://github.com/rymuelle), [@ProGamerGov](https://github.com/ProGamerGov) | not yet offered                                                                                                                                                                                                                                                                    |
| Lightroom XMP import brings over linear and radial gradients and brush masks with their local adjustments ⚑                                                                                                                                                                       | feature     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Authenticate local MCP requests and connect Claude Code/Codex through a separate stdio adapter                                                                                                                                                                                    | privacy     | [@yojen7](https://github.com/yojen7)                                                                                           | not yet offered                                                                                                                                                                                                                                                                    |
| Optional MCP server (cargo feature `mcp`, off by default): AI agents can open images, read and change adjustments, render previews, read the histogram and export through the running editor                                                                                      | feature     | [@cgasgarth](https://github.com/cgasgarth)                                                                                     | not yet offered                                                                                                                                                                                                                                                                    |

⚑ changes rendered output on purpose. Upstream status as of 2026-10-04.

## Details

### Keep Library and Editor content inside resized docks and give the terminal one compact header

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original implementation with Codex)
- **Upstream:** not yet offered
- **Notes:** Library and Editor flex panes shrink with the terminal dock; the virtualized Library grid clips within its own viewport. The terminal has one 32px row with tabs, launch buttons, settings and collapse. Narrow docks use labelled icons, launch mode lives in terminal preferences, and side resizers support keyboard steps. Native stepped overlap, compact-header and control validation pending.

### Include MCP and its adapter in Linux packages with default-off AI control and assistant launch consent

- **Type:** platform
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original implementation with Codex; existing MCP authorship retained)
- **Upstream:** not yet offered
- **Notes:** Official deb/AppImage builds include the existing terminal and MCP features plus the stdio adapter. AI control starts off; Settings and assistant start buttons explicitly enable it. Disabling stops the listener and accepted connections and removes the endpoint; re-enabling uses a fresh key. Built-in/external launch registration uses the installed adapter without changing global client configuration. No new dependency or licence. The 18-step private Linux native pass verifies four real Claude/Codex launches using app-generated registration, default-off and Cancel, painted consent, private fresh-key endpoints, listener/connection removal, unchanged edits/history across settings-only restart and clean exit.

### Read-only MCP contact sheets, exact clipping/color statistics and native-resolution detail crops

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original implementation with Codex; designs from sheldonxxxx and SandeepSubba, no copied code)
- **Upstream:** not yet offered
- **Commits:** [4494beb](https://github.com/sheldonxxxx/RapidRAW/commit/4494beb9bae0ee57f420a8292c4f4f6e1dba4bd2), [4a60d6d](https://github.com/SandeepSubba/RapidRAW/commit/4a60d6d69610cb8f013e52b35c10b3d1ab64a816)
- **Notes:** Optional MCP tools render 2–6 variants at the same geometry, preserve current GUI virtual-copy labels, and return bounded PNGs. Exact sRGB channel clipping/count histograms and linear Rec.709 luminance use pre-JPEG rendered pixels; region white-balance gains are advisory only. Native crops use post-transform 1:1 pixel coordinates with explicit edge/source/payload limits. Read-only calls preserve edits, revision, history and sidecars. Embeds unmodified Poppins Regular with OFL licence and adds an optional direct edge to the existing ab_glyph version; default dependency graph remains unchanged. The 12-step Linux native scenario verifies actual Claude calls, independently decoded exact statistics, stable variants, real virtual-copy labels, nested native crops and unchanged state/history/sidecars. Final release regression validation pending; macOS/Windows and packaged runtime untested.

### Label AI edits in shared undo history and expose editor context, generated adjustment schema and bounded original/comparison/region previews

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original implementation with Codex, retaining the cgasgarth MCP server authorship)
- **Upstream:** not yet offered
- **Notes:** Optional MCP feature: undo/redo use the editor store and preserve labelled AI entries; reads do not change edits or history. Schema defaults and UI ranges come from source and have a drift test. Original and comparison previews use generated neutral defaults, fraction regions are validated before rendering, and JPEG payload/long edge are bounded. The 11-step Linux native scenario verifies actual Claude Code three edits, labelled GUI history and undo twice, generated schema/bounded previews, and independent Codex read-only tools with unchanged state/history. The required human live check remains pending before merge; macOS/Windows and packaged runtime are untested.

### Movable terminal panel with persistent tabs, bounded PTY output, shell/font preferences and built-in/external assistant launch controls

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original work with Codex; optional portable-pty and xterm dependencies credited separately)
- **Upstream:** not yet offered
- **Notes:** Requires cargo feature terminal; official Linux release configuration enables it. Uses existing left/right panel docking plus a resizable full-width bottom region. Saved tabs reopen fresh shells without persisted transcripts or automatic commands. Output is bounded with acknowledgement backpressure, OSC 52 clipboard commands are blocked, and owned PTY sessions are closed on tab/app exit. Claude/Codex start buttons type fixed shell commands or use the existing external terminal infrastructure. The 15-step Linux native scenario verifies real Claude Code and Codex from both launch paths over authenticated stdio, with the same folder and standard configuration, automatic endpoint discovery, changed photo revision/slider/preview, external mode keeping the panel closed, and PTY cleanup on quit. Default dependency graph is unchanged. Native macOS/Windows and physical terminal focus are untested.

### Keep webview scripts local and disable Cloud sign-in before adding the terminal panel

- **Type:** privacy
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original policy integration, AI-assisted; measured design from #125)
- **Upstream:** not yet offered
- **Notes:** Implements Josh’s #119 choice: strict script CSP with Clerk off. Existing CPU and AI Connector choices remain available. Production and development directives retain required asset, IPC, style, map and update origins; only styles bypass automatic Tauri CSP modification. CI checks the configured policy, saved Cloud settings, and blocked nonlocal script execution. The native panel scenario enforces the response-header policy and blocks a nonlocal script control while PTY and authenticated MCP editing work.

### Fix intermittent green OM System/Olympus RAW exports by reading image metadata from its correct directory

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original rawler decoder fix, AI-assisted)
- **Upstream:** not yet offered
- **Commits:** [a98bd05](https://github.com/yojen7/RapidRAW-DngLab/commit/a98bd053c4a14c0c1105ee08a8deba8ce5e6ca5d)
- **Notes:** Scope ORF white balance, black levels, valid bits and crop to the ImageProcessing maker-note IFD, rejecting truncated fields. CameraSettings preview tags can no longer shadow image metadata through unordered traversal. Preserve legacy root white balance, RAW thumbnails, DNG proxies and Adobe lossy-DNG corrections. Restores the approved reference output without changing render settings.

### Faster RAW library thumbnails: unedited raws use the smallest sufficient embedded preview, edited linear DNGs render from their embedded proxy

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@SebastianEggli](https://github.com/SebastianEggli), from SebastianEggli/MyRR
- **Upstream:** PR [#1809](https://github.com/CyberTimon/RapidRAW/issues/1809); PR [RapidRAW-DngLab#10](https://github.com/CyberTimon/RapidRAW-DngLab/issues/10)
- **Commits:** [4a64607](https://github.com/SebastianEggli/MyRR/commit/4a646075d557f6faa5310377f26ae66120af1aa4), [f0a5c66](https://github.com/SebastianEggli/MyRR/commit/f0a5c66ccad1e1a82b880dc8e469c6fb21144d66), [e93c711](https://github.com/SebastianEggli/MyRR/commit/e93c7110b218727ce4fb49c3019415ad0f7e188e), [094189d](https://github.com/SebastianEggli/RapidRAW-DngLab/commit/094189d738f30ec8e973b998fc59a84d00d94d8c)
- **Notes:** Exports preserve the preceding full-image decode; library thumbnails may differ because they use smaller embedded JPEGs or correctly normalized DNG proxies. The rawler dependency f13d56ec retains RapidRoom crop, dither and full-image Adobe polynomial fixes alongside Sebastian Eggli's preview paths. Adaptations preserve metadata orientation and parallel conversions, reject real or malformed crops from the unedited-preview gate, and retry the full image if a proxy fails. Native real-file and cold/warm sample-probe results are recorded on the PR.

### Apply Adobe lossy DNG polynomial mappings before developing the full image, fixing pink and blown-out photos

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (original rawler decoder fix, AI-assisted)
- **Upstream:** [#1542](https://github.com/CyberTimon/RapidRAW/issues/1542)
- **Notes:** Apply validated DNG OpcodeList2 MapPolynomial operations in order, after linearization and black/white normalization and before demosaic. Respect per-plane areas and pitches, clip each mapped result, and prevent converted DNGs replaying already baked mappings. Unknown-only lists keep the existing decoder behavior; required unsupported operations mixed with polynomial mappings report an error. Private real-file before/after evidence and corpus validation are recorded in the PR. Human rendering approval is required.

### Preview and selectively import Lightroom catalog photo edits, crop, straighten, rotation, star ratings and virtual copies, with unsupported-setting reports and independent replacement choices

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@laurensiusadi](https://github.com/laurensiusadi), [@yojen7](https://github.com/yojen7), from laurensiusadi/RapidRAW, adapted in RapidRoom with the shared read-only catalog reader and current image-aware XMP mapper
- **Upstream:** not yet offered
- **Commits:** [dda6cc5](https://github.com/laurensiusadi/RapidRAW/commit/dda6cc51c69dc6a17906609dbb667eff3507aeeb)
- **Notes:** Catalogs and originals are read only. Reuses the collections root resolver and bounded Lua-table parser; caps compressed rows at 4 MiB. Preview lists converted controls, unsupported settings, missing originals and unreadable sidecars before an explicit selection is applied. Existing edits, cleared ratings, keywords, EXIF and other metadata are preserved unless the relevant replacement choice is enabled. Uses atomic sidecars, Card mode guards, stale-preview checks and stable full-ID virtual-copy identities. As-shot white balance requires an explicit history reference; no folder median is estimated. Supported nested profile tone curves use the existing mapper; Adobe profiles, local/AI masks and other unsupported fields are reported. Synthetic native/frontend coverage and private sidecar-derived synthetic-catalog calibration only; real Lightroom catalogs, desktop interaction and other platforms still need human checks. Adobe rendering parity is not promised. Adapted with Codex, with source authorship retained.

### Import Lightroom Classic collections and collection sets from a .lrcat catalog as albums, with a preview of found and missing photos and relinking of moved root folders

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@Bennyyy27](https://github.com/Bennyyy27), from Bennyyy27/RapidRAW, branch scs/compact-ui (adapted in RapidRoom: shared read-only catalog reader, preview dialog)
- **Upstream:** not yet offered
- **Commits:** [3bd74e2](https://github.com/Bennyyy27/RapidRAW/commit/3bd74e26c6424d57daa8d404eb32fc8157de8b27), [0cfa8dd](https://github.com/Bennyyy27/RapidRAW/commit/0cfa8dd9426fb5f86a3c0781b2f8c6f18083dbab)
- **Notes:** Right-click an empty spot under Albums and choose Import Lightroom Collections. The catalog is only read: a closed catalog is opened as an immutable SQLite file, one with a leftover -wal or -journal file is read from a temporary copy, and a catalog that Lightroom Classic has open (.lock file) is refused. A preview lists collection sets, collections, found and missing photos, skipped smart collections, and each root folder with where it was found, so roots from a Windows or macOS catalog can be relinked before importing. Missing photos stay in their albums. Importing the same catalog again refreshes its group in place. Smart collections, books, slideshows, prints, web galleries and develop settings are not imported. Album saves are now atomic. Adds the rusqlite dependency (MIT; bundled SQLite is public domain). Not yet tested against a real Lightroom catalog in RapidRoom. Harvested with Claude Code.

### Faster raw decoding: highlight recovery and pixel-format conversions run on all CPU cores; the GPU adapter is logged; `rapidraw bench` measures the image pipeline without a window

- **Type:** performance
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@elhigu](https://github.com/elhigu), from CyberTimon/RapidRAW
- **Upstream:** PR [#1790](https://github.com/CyberTimon/RapidRAW/pull/1790) open
- **Commits:** [d2e9351](https://github.com/CyberTimon/RapidRAW/commit/d2e9351d48eadef1662affdc2a21d82e26debe04), [0cd9d87](https://github.com/CyberTimon/RapidRAW/commit/0cd9d875f0dd2a8ac6c29f87ec8ea288812a297b), [f52b75a](https://github.com/CyberTimon/RapidRAW/commit/f52b75a81f993b68b1f303e898bc54dba2f76c75)
- **Notes:** The parallel conversions are per-pixel and tested bit for bit against the serial code. The bench command follows RapidRoom's headless CLI rules: no window, closed-pipe-safe output, exit 1 on failure and 2 on bad arguments. Upstream's speed-up figures are not yet measured on RapidRoom.

### Batch rename treats a photo as one unit: RAW+JPEG/HEIF pairs and every sidecar (.rrdata incl. virtual copies, both XMP styles, .acr, .dop, .pp3) move together, with metadata, burst and group tokens, a preview with collision checks, a rollback-safe two-phase rename and undo

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), [@SandeepSubba](https://github.com/SandeepSubba), from rapidroom; metadata tokens from SandeepSubba/RapidRAW (upstream PR 1309)
- **Upstream:** PR [#1309](https://github.com/CyberTimon/RapidRAW/issues/1309)
- **Commits:** [12b3c5e](https://github.com/SandeepSubba/RapidRAW/commit/12b3c5e52eb1574fa8ef06ab3d32867d415a946f)
- **Notes:** New tokens: {rating}, {stars}, {camera}, {lens}, {iso}, {focal}, {folder}, {label}, {title}, {author}, {copyright}, {comments}, and for rename {group} and {member}. Export and import use the same token engine. {sequence} follows capture time (DateTimeOriginal plus SubSecTimeOriginal), so bursts and brackets number in shooting order. Unknown tokens are an error in the rename preview. Undo covers the last rename and survives a restart. Albums, cached thumbnails and EXIF, the selection and the editor follow the new names. Still refused in Card mode. Written with Claude Code.

### Lightroom XMP import: lens profile turns on lens correction, profile-look tone curves are kept, dead keys are dropped, and an import report lists what was not transferred

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Follow-up to the Lightroom import (RapidRoom #54). Changes what imported sidecars render: crs:LensProfileEnable=1 now switches lens correction to auto with distortion and vignetting on; the tone curve of a nested look (Adobe Color, creative profiles) is composed under the user's luma curve; ToneCurvePV2012 points are copied without the old shadow-dampening rewrite; SharpenEdgeMasking maps to the sharpening threshold. sharpenRadius, sharpenDetail, sharpenMasking, colorNoiseDetail and colorNoiseSmoothness are no longer written (nothing read them). Exposure scaling to RapidRAW's 1.25 EV unit is in the code but off (SCALE_LIGHTROOM_EXPOSURE_TO_RAPIDRAW_UNITS) until the tone calibration lands. Single-photo and folder imports now report custom white balance, profile look, AI Denoise, masks and Point Color when they could not be transferred. Written with Claude Code.

### Editor Reference View: pin a read-only reference photo beside the image you are editing (Shift+R)

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@cgasgarth](https://github.com/cgasgarth), from cgasgarth/RapidRaw
- **Upstream:** not yet offered
- **Commits:** [aa0e235](https://github.com/cgasgarth/RapidRaw/commit/aa0e23573923dfc45c2c5d97b26b4e1e8ee77fdf)
- **Notes:** Reference-view state and toolbar UX ported from cgasgarth's Lightroom reference view and adapted to RapidRoom's editor by Claude Code. Pick the reference from the filmstrip. It is rendered from its saved adjustments through the existing read-only preview command, has its own zoom and pan, and never becomes the selected, edited or exported image. Display only: no change to the image pipeline or exports.

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
- **Notes:** Another denoise cannot start while a command is pending. Cancellation does not interrupt the backend: single processing may continue and batch work may still write files. Superseded by `denoise-backend-cancel` (RapidRoom #68), which adds real interruption and job IDs.

### DxO compressed DNG highlights no longer wrap to black dots when lookup-table dithering exceeds 16 bits

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7), from yojen7/RapidRAW-DngLab
- **Upstream:** [#1119](https://github.com/CyberTimon/RapidRAW/issues/1119) open 156 d
- **Commits:** [5a44585](https://github.com/yojen7/RapidRAW-DngLab/commit/5a44585da25c7ff783c83a4bff25281f0cf28159), [4c96062](https://github.com/yojen7/RapidRAW-DngLab/commit/4c960626978287ab8e4aabe70a6cfa60b4582273)
- **Notes:** Dither values above 65535 now saturate instead of wrapping. Unsaturated values and the random-state update are preserved. The fork retains Kheil-Z's Sony/Canon default-crop fixes; the extra tile-fixture change is test-only.

### Cancelling a denoise stops the backend work, and each job's progress, preview and saved result are tied to its own job ID

- **Type:** fix
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** [#1697](https://github.com/CyberTimon/RapidRAW/issues/1697) open 28 d
- **Notes:** The backend hands out a job ID; progress, preview, error and batch events carry it, and Save only takes the result of the job whose preview is shown. `cancel_denoise` sets a flag checked between BM3D patches, between AI tiles and between batch files, and terminates an ONNX tile already running. Not interruptible: the AI model download, the Apple RAW 9 develop call, image decoding and preview/file encoding. A cancelled batch keeps the files it already finished; the file being written goes to a hidden partial file that is deleted, and existing files are never removed. A new denoise can start right after Cancel. Denoise output is unchanged. Written with Claude Code.

### LinearRaw gamma modes use the sRGB exponent 2.4 when removing gamma

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@pluja](https://github.com/pluja), from pluja/RapidRAW-Fork
- **Upstream:** PR [#1633](https://github.com/CyberTimon/RapidRAW/issues/1633)
- **Commits:** [eb68ee8](https://github.com/pluja/RapidRAW-Fork/commit/eb68ee88ba309a6b4e9770046204bc6292f0f1d7)
- **Notes:** Correct the power from 3.0 to 2.4 in the optional gamma and gamma_skip_calib modes for LinearRaw files. Auto/skip_calib modes, Bayer and X-Trans files do not use this inverse transfer function.

### Saturated red highlights use a continuous magenta correction instead of abrupt green lifts

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@3048mm](https://github.com/3048mm), from CyberTimon/RapidRAW
- **Upstream:** PR [#1824](https://github.com/CyberTimon/RapidRAW/issues/1824)
- **Commits:** [e99082a](https://github.com/CyberTimon/RapidRAW/commit/e99082ad1d3cfea3db1f3e6611340f024f664258)
- **Notes:** Arrives through the normal upstream-main merge, with original author and Claude co-author retained. Weight the existing correction by relative magenta excess; the empirical full-weight threshold is 0.25. Human visual/reference approval is required.

### HSL mixer hue and saturation are evaluated in sRGB perceptual space while luminance stays linear

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), from CyberTimon/RapidRAW
- **Upstream:** [#1775](https://github.com/CyberTimon/RapidRAW/issues/1775); PR [#1777](https://github.com/CyberTimon/RapidRAW/issues/1777)
- **Commits:** [e883421](https://github.com/CyberTimon/RapidRAW/commit/e8834210d9c88793b463f4feca3b6e5ee3f65e55)
- **Notes:** Normal upstream-main merge. Active effective bands convert linear RGB to sRGB before the HSV split and back before the linear-luminance rescale. Inactive bands now return original linear RGB exactly; the accompanying zero-HSL and vibrance-domain correction has its own entry and rendering evidence.

### Sync upstream main: shared RAW embedded previews, Nikon lens metadata fallback and mask Escape/cache fixes

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), [@CyberTimon](https://github.com/CyberTimon), from CyberTimon/RapidRAW
- **Upstream:** PR [#1823](https://github.com/CyberTimon/RapidRAW/issues/1823); PR [#1815](https://github.com/CyberTimon/RapidRAW/issues/1815); PR [#1827](https://github.com/CyberTimon/RapidRAW/issues/1827)
- **Commits:** [0957a1a](https://github.com/CyberTimon/RapidRAW/commit/0957a1ae31248e46196e24c0fe81b92f71f9f69d), [c11c7a5](https://github.com/CyberTimon/RapidRAW/commit/c11c7a5c8d0c78b9f28175a377c53a6569aebe53), [4324809](https://github.com/CyberTimon/RapidRAW/commit/432480973061898dd04fa3d04119bd02d6bd372f), [4e45e62](https://github.com/CyberTimon/RapidRAW/commit/4e45e6206fb63d37dd5e2764db8a134a79d2034c), [9671795](https://github.com/CyberTimon/RapidRAW/commit/9671795e65803df4fc75239d752ddd16b829345b), [cf6813f](https://github.com/CyberTimon/RapidRAW/commit/cf6813f198a252068eaa349eec7598d6761e830a)
- **Notes:** Preserve every RapidRoom change and upstream author. The additive EXIF conflict keeps bounded rating readers and the new lens helper. Existing float upload and ICC work are deduplicated; rawler crop/dither, mask/export protections and cache identity are retained. Render-changing highlight and HSL commits have separate entries. Cache fixes can correct stale rendered masks/exports; the corpus has no interactive cache-reuse scenarios, so those paths still need running-app testing.

### Inactive HSL preserves linear RGB exactly; saturated colours no longer turn grey under positive vibrance

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Check all eight effective per-pixel HSL bands before clamping or conversion, including mask contributions. Keep the positive-vibrance fractional power base nonnegative when HSV saturation rounds above one. Active HSL retains the upstream perceptual conversion and existing negative-channel policy; HDR values above one remain supported. Written with Codex. Rendering evidence and GPU regression tests accompany the upstream-main sync.

### Import old Lightroom `.lrtemplate` presets stored as plain Lua tables, without running any Lua

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@laurensiusadi](https://github.com/laurensiusadi), from laurensiusadi/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [bb60210](https://github.com/laurensiusadi/RapidRAW/commit/bb60210c609c6939d34deba9c1043fe9d01bde92)
- **Notes:** Rewritten from the fork's regex converter as a bounded Lua-table parser that feeds the existing XMP preset mapping, so values match .xmp presets. Settings nested in Looks or local corrections are not read. Batch preset import lists what was not imported (profile look, masks, Point Color, camera or lens profile, custom white balance) using the same identifiers as the XMP sidecar report; malformed files or curves are rejected with a reason. Importing only adds a preset; photos change only when you apply it. Harvested with Claude Code.

### Pick and reject flags, separate from star ratings: P / X / U shortcuts, badges, a flag filter (picked, unflagged, hide rejected, rejected) and Delete rejected

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), [@yojen7](https://github.com/yojen7), from lalibertemarc/RapidRAW (adapted by yojen7)
- **Upstream:** [#1599](https://github.com/CyberTimon/RapidRAW/issues/1599); [#994](https://github.com/CyberTimon/RapidRAW/issues/994); [#1057](https://github.com/CyberTimon/RapidRAW/issues/1057)
- **Commits:** [1b4b9db](https://github.com/lalibertemarc/RapidRAW/commit/1b4b9db115f0c1aa6f12e2b4bc0515ffb7fb0eb6)
- **Notes:** The flag is its own `flag` field in the sidecar, so a reject keeps the stars and older builds ignore it. With XMP sync on, a reject is written as xmp:Rating="-1" (darktable, Bridge and Lightroom use the same value) and read back from it; a flag you set or remove in RapidRoom wins over the .xmp, and a virtual copy never changes the original's reject. A Lightroom XMP import brings rejects in. The Presets panel shortcut moves from P to Shift+P. Upstream has two other designs for this, PRs 1075 (reject as rating -1) and 1272 (flags as tags); the PR for RapidRoom #32 explains the choice. Harvested with Claude Code. Local Codex review propagates sidecar save failures, keeps XMP unchanged when saving fails, and serializes optimistic rating/flag writes so a refused write restores the saved state without discarding later actions.

### Presets move to the left sidebar with compact rows, hover previews, favorites and folder reordering

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@laurensiusadi](https://github.com/laurensiusadi), from laurensiusadi/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [9535884](https://github.com/laurensiusadi/RapidRAW/commit/953588443099def3fd0c5e92a55e1966d994e385), [9e478b6](https://github.com/laurensiusadi/RapidRAW/commit/9e478b66eeee63be3e79238af708916655f52125)
- **Notes:** Hovering a preset previews it on the image without touching the edit, its history or sidecars; only a click applies it. Saved workspaces keep their layout: Presets only moves when it is still in its old default spot on the right. Preset preview thumbnails are gone. Enter/Space apply a focused preset. Scrollbar thumbs across the app are more subtle.

### Color Grading and Color Mixer get their own visibility eye, globally and in masks; the Color panel eye still bypasses every colour tool

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), from lalibertemarc/RapidRAW
- **Upstream:** PR [#1833](https://github.com/CyberTimon/RapidRAW/issues/1833)
- **Commits:** [879b414](https://github.com/lalibertemarc/RapidRAW/commit/879b4146fa4d87ecfa3580261cfc0554ffb1555f)
- **Notes:** New sectionVisibility.colorGrading/colorMixer keys default to on, so existing sidecars and presets render unchanged; turning a tool off intentionally skips it. Resetting or pasting the Color section re-enables both tools.

### Lights Out viewing in Library and Editor: L cycles Normal → Dim → Black, Shift+L goes back, Escape restores

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@zeromeridian](https://github.com/zeromeridian), [@yojen7](https://github.com/yojen7), from zeromeridian/RapidRAW-Enhanced
- **Upstream:** not yet offered
- **Commits:** [69bdb28](https://github.com/zeromeridian/RapidRAW-Enhanced/commit/69bdb283246c7c6746f88c7c0c22d88b7e90ae94)
- **Notes:** Only the app chrome around the photo is dimmed or hidden; the preview and exports are untouched. Toggle folder tree moved from L to N to free the key. Black also switches the window to fullscreen and restores it afterwards. Local Codex review integrates Reference View and pick/reject controls, and serializes native fullscreen entry/restore to preserve the original window state during rapid mode changes; six async tests cover cancellation, restore races and native failures.

### Expanded Color Mixer: Hue, Saturation and Luminance tabs with all eight bands, and a pipette that adjusts the bands under the cursor as you drag up or down on the photo

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@lalibertemarc](https://github.com/lalibertemarc), from lalibertemarc/RapidRAW
- **Upstream:** not yet offered
- **Commits:** [b5475ad](https://github.com/lalibertemarc/RapidRAW/commit/b5475ad8e3176386153d8c3a8fbdc7d21ba7c5a9), [dffc46a](https://github.com/lalibertemarc/RapidRAW/commit/dffc46a161f6a9c35037348d9d62b1a5267a99ac), [17173f2](https://github.com/lalibertemarc/RapidRAW/commit/17173f2873b44f4f7d5f219df1e5e744b3e0dfa9)
- **Notes:** HSL math and defaults are unchanged; the pipette only sets the existing mixer sliders. Each band moves in proportion to how much of it is in the sampled square (dominant band 0.5 per pixel, Alt for fine steps), weighted by the current shader's perceptual sRGB hue and saturation, with its linear neutral gate. A drag is one undo step. Global mixer only, not masks. Adapted with Claude Code. Local Codex review aligns sampling with the post-sync shader and stops the gesture when the parent Color or Color Mixer eye is off; no render math changes.

### Open Terminal Here: open your own terminal in a library folder, with RAPIDROOM_VERSION and RAPIDROOM_FOLDER set, so you can run Claude Code or Codex beside the editor

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** Stopgap for the built-in terminal (RapidRoom #119, design note in rapidroom/TERMINAL.md). Folder context menu. Linux tries $TERMINAL, xdg-terminal-exec, then common terminals with their working-directory flags; macOS opens Terminal.app; Windows uses Windows Terminal or cmd. AppImage library paths are removed from the terminal's environment; Flatpak reports that it can't reach the host. The terminal program never comes from settings or the webview. No new dependencies or network connections. Written with Claude Code. Local review adds the explicit xdg-terminal-exec directory argument and preserves sibling environment paths whose names share the AppImage prefix, with regression tests.

### RapidRoom skill and Claude Code plugin: editing workflow, adjustment reference generated from the code, tutor mode, source lookup and assistant-drafted issues

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), [@sheldonxxxx](https://github.com/sheldonxxxx)
- **Upstream:** not yet offered
- **Commits:** [d332fbd](https://github.com/sheldonxxxx/Lightweft/tree/d332fbd629faf7e47144ba173d7f2032be62199d)
- **Notes:** rapidroom/plugin: a Claude Code plugin (skill, six slash commands, MCP connection) and a Codex copy, for the in-app MCP server (RapidRoom #5). The adjustment and mask reference is generated from INITIAL_ADJUSTMENTS, the editor sliders and createSubMask, and a Vitest test fails when it drifts. Artistic direction, taste learning and film-look guidance are adapted from sheldonxxxx's Lightweft skills (MIT). Planned MCP tools are listed as coming. Issue drafts are shown to the user and posted only after an explicit yes. Docs only: the app and its rendering are unchanged. Written with Claude Code.

### Optional compact adjustment sliders: label, track and value on one row, with aligned columns, in the Develop and mask panels

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@pluja](https://github.com/pluja), [@Bennyyy27](https://github.com/Bennyyy27), from pluja/RapidRAW-Fork
- **Upstream:** not yet offered
- **Commits:** [198d23c](https://github.com/pluja/RapidRAW-Fork/commit/198d23c0b89e073c88a621d43090488c2a8baf9e), [69445ee](https://github.com/Bennyyy27/RapidRAW/commit/69445eefb3598f3461eb3ed28fb82913b2e04d0f)
- **Notes:** Off by default (Settings > General). Layout from pluja's one-row slider; narrow-panel column sizing follows Bennyyy27's final compact density. Drag travel spans the whole row in both densities, so switching density does not change sensitivity. Local integration/review by Codex: accessible React-node and typed-field names, six slider tests, 48 browser layout cases and 24 full-panel cases with screenshots; native desktop checks remain.

### Sony 4:3, 1:1 and 16:9 camera framing opens as an editable crop

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-03
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Commits:** [b84ca29](https://github.com/yojen7/RapidRAW-DngLab/commit/b84ca29b143c62f920361081b058c77e4d73faa5)
- **Notes:** Reads SonyCropTopLeft/SonyCropSize in the rawler fork, retains the full DefaultCrop image for crop expansion/reset, and preserves existing sidecar framing. The M/S crop and saturating highlight dither fixes remain. AI-assisted with Codex; maintainer visual review required.

### Lens correction evaluates Lensfun profiles the way Lensfun does: right radius per model, rescaled terms instead of ×2.5, calibration that fits the sensor

- **Type:** fix (changes rendered output)
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@beneedict](https://github.com/beneedict), [@yojen7](https://github.com/yojen7), from beneedict/RapidRAW (adapted by yojen7 with Claude Code: old edits kept, Lensfun's choice of calibration, vignetting across sensor sizes, Lensfun oracle tests)
- **Upstream:** [#1688](https://github.com/CyberTimon/RapidRAW/issues/1688); [#1689](https://github.com/CyberTimon/RapidRAW/issues/1689); PR [#1705](https://github.com/CyberTimon/RapidRAW/issues/1705)
- **Commits:** [99a142e](https://github.com/beneedict/RapidRAW/commit/99a142e6e7be658c721f5700811199429efb22ee)
- **Notes:** Only images with a Lensfun lens profile change, and only when the lens is chosen or detected again: edits saved by an older version keep their previous lens values and render bit for bit as before. Checked against the Lensfun C++ library on 13 cases (ptlens, poly3, poly5; full frame, APS-C, Micro Four Thirds, compact; 3:2, 4:3, 16:9, portrait). The embedded RAW lens profiles of the source branch (CyberTimon/RapidRAW#1687) are not included.

### Export recipes: export presets from the CLI (`--preset`), opt-in output sharpening for screen or print, and Instagram 4:5, 1:1 and 1.91:1 presets

- **Type:** feature (changes rendered output)
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** RapidRoom #121. Rust can now read export presets (export_recipes.rs), so `rapidroom export --preset NAME` exports with a saved preset or a built-in recipe; MCP support follows once the MCP server (#102) is on main. Three read-only Instagram recipes (1080×1350, 1080×1080, 1080×566; sRGB JPEG q90, padded, GPS stripped, light screen sharpening) appear in the export panel's preset list. Output sharpening is an output-luma unsharp mask after the resize whose radius follows the output size; it is off unless chosen, so default exports are unchanged. Rendering change only for opt-in settings: sharpening when chosen, and pad-plus-resize exports now derive the resized edge from the pad ratio (at most 1 px different, so 4:5 at 1080 wide is always 1350). The donor note (puneetrane1811's Creative Export Studio, weholt's export workflows) is rapidroom/EXPORT-RECIPES.md; no donor code is used. Written with Claude Code. Local integration/review by Codex: sharpened float/alpha precision regression, donor snapshot statistics rechecked, and processing/quality claims narrowed.

### Native Linux UI smoke: exercise the release app, GPU preview, edits, Undo, Compact settings and JPEG export on an isolated compositor

- **Type:** ci
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom; optional WebdriverIO embedded Tauri driver (MIT)
- **Upstream:** not yet offered
- **Notes:** Written with Codex for issue #134. The opt-in native-ui-test feature and explicit port enable loopback automation only in a test GUI; ordinary releases and headless export/benchmark omit registration. Weston captures the real native app and its GPU-processed preview on a private session bus and display with fresh settings and copied CC0 input. Scripts preserve per-step screenshots, error logs and pass/fail evidence. Local Linux gate; hosted CI and other platforms remain untested. An optional latest-release lookup returning 404 is treated as no available release, so a private or new repository does not log a startup error.

### AI raw denoise: Best and Fast presets with optional sharpening, cached sensor output, and basic sliders under Quick noise reduction

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7), [@rymuelle](https://github.com/rymuelle), [@ProGamerGov](https://github.com/ProGamerGov), from rapidroom; RawForge and RawHandler (MIT), blended-tiling-numpy (MIT), colour-demosaicing (BSD-3-Clause)
- **Upstream:** not yet offered
- **Commits:** [79f1770](https://github.com/rymuelle/RawForge/commit/79f17700c29765b28a4cf5bd1b6d90bc7e3eb7f2)
- **Notes:** Opt-in Bayer RAW denoise uses the existing ONNX Runtime on CPU. Best uses TreeNet; Fast uses SuperLight; Sharpen defaults on and adds DeepSharpen. Models download from the author on first use and are SHA-256 verified; weights are not bundled. Sensor results are cached independently of editor adjustments. NIND, BM3D and Apple RAW 9 remain in More methods; existing basic-slider values are preserved. Job IDs and cancellation remain in use. Licence details and human dialog checks: rapidroom/DENOISE.md. Written with Codex.

### Lightroom XMP import brings over linear and radial gradients and brush masks with their local adjustments

- **Type:** feature (changes rendered output)
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@yojen7](https://github.com/yojen7)
- **Upstream:** not yet offered
- **Notes:** RapidRoom #71. Sidecar imports turn crs:MaskGroupBasedCorrections into RapidRAW masks: Mask/Gradient becomes a linear mask, Mask/CircularGradient a radial mask (feather, inversion, Flipped), and Mask/Paint strokes a brush (erase strokes included; strokes with less flow or density use the flow brush). Add, subtract and intersect components keep their mode. Geometry follows tiff:Orientation and the imported straighten angle. Local exposure, contrast, highlights, shadows, whites, blacks, clarity, dehaze, texture, saturation, temperature, tint, sharpness and noise reduction carry over, scaled by the correction's Amount. Corrections with AI components (Mask/Image) or other unsupported components are skipped and listed in the import report, as are local adjustments without a RapidRAW slider. Not calibrated against Lightroom yet: the local exposure factor (LIGHTROOM_LOCAL_EXPOSURE_EV_PER_UNIT), the brush radius reference and the radial angle direction are assumptions to tune with the Lightroom calibration set. Reusable .xmp presets still import without masks. Written with Claude Code. Local Codex review shares crop/mask orientation mapping with the RAW decoder, fixing swapped EXIF 5/7 transforms; a marked-pixel test checks actual imported crop and radial-mask placement against image rotation for all eight orientations. Maintainer-approved integration carries an explicit warning in the user docs and single-image/folder import reports: globals and masks remain uncalibrated and are not ready for real use until full import calibration (RapidRoom #31 and #133) is complete.

### Authenticate local MCP requests and connect Claude Code/Codex through a separate stdio adapter

- **Type:** privacy
- **Landed in RapidRoom:** 2026-10-05
- **By:** [@yojen7](https://github.com/yojen7), from rapidroom (fresh code; design ideas from 1tuz, Irvingouj and ssarangi; AI-assisted)
- **Upstream:** not yet offered
- **Notes:** Only with feature mcp: fresh per-start token, atomic private endpoint file (0600 on Unix), fixed-size constant-time SHA-256 verification, and a separate GUI-free rapidroom-mcp-stdio adapter with automatic per-user discovery. No copied adapter code or additional filesystem tools. Default build remains feature-off. Strict all-feature Clippy, 331 MCP Rust tests, 134 frontend tests and three standalone security tests pass. Real Claude Code 2.1.289 and Codex 0.160.0 discover and edit through stdio in the Linux native harness; missing/wrong tokens return 401 and authenticated Origin requests return 403. Codex forwards XDG_CONFIG_HOME explicitly. Default dependency graph and frontend type diagnostics are unchanged; pixel regression results are recorded in the PR.

### Optional MCP server (cargo feature `mcp`, off by default): AI agents can open images, read and change adjustments, render previews, read the histogram and export through the running editor

- **Type:** feature
- **Landed in RapidRoom:** 2026-10-04
- **By:** [@cgasgarth](https://github.com/cgasgarth), from cgasgarth/RapidRaw (put behind a feature flag in RapidRoom; design ideas from sheldonxxxx/RapidRAW)
- **Upstream:** not yet offered
- **Commits:** [4a5c14e](https://github.com/cgasgarth/RapidRaw/commit/4a5c14eae0715db4d9ae79d4126a1495a0493ff0), [c0f079d](https://github.com/cgasgarth/RapidRaw/commit/c0f079d7fbf017369de660e29f2993f457337115), [b2b9abf](https://github.com/cgasgarth/RapidRaw/commit/b2b9abf2cae9ecae8ad30ccf1ceff28adee6ae24), [9a70a3c](https://github.com/cgasgarth/RapidRaw/commit/9a70a3cd567293c2f25127cf6e018080af91685d), [0a5e71f](https://github.com/cgasgarth/RapidRaw/commit/0a5e71f1fdbaef9f58fa5ca0059319c41f645df8), [c8d6ec6](https://github.com/cgasgarth/RapidRaw/commit/c8d6ec61aa7a27b132d2c93a7a30f4ed3186238a), [a5a01cc](https://github.com/cgasgarth/RapidRaw/commit/a5a01ccfe370a5dfe1042e8ca0e2537780d18409)
- **Notes:** Only in builds with `--features mcp`. Listens on authenticated 127.0.0.1:7790 while the app runs; requests from browsers are refused. The separate stdio adapter discovers the private per-user endpoint. The default build has no MCP code and no listener. Comparison with sheldonxxxx's headless MCP and the security notes are in rapidroom/MCP.md. Local review hardens chunked-body length bounds and terminators, discards consumed framing buffers, and adds frontend regression tests for first-photo opening, pending GUI history, no-op edits, reset undo and slow previews. Local follow-up prevents mirroring a newly selected photo under a pending command’s old path; seven bridge tests cover no-op/reset/history/timeout/photo-switch behavior. Tool discovery supplies the required 2026-07-28 cache hints (zero TTL, private scope) and completed tool calls include resultType=complete so current clients validate the response. The optional Linux native harness can run installed Claude Code and Codex clients against an owned photo and require real editing calls, revision/slider/preview changes, and valid discovery at both 2025-06-18 and 2026-07-28.
