# What RapidRoom adds to RapidRAW

Generated from [rapidroom/changes.json](rapidroom/changes.json) by `node rapidroom/status.mjs`; don't edit by hand.

**7 changes on top of RapidRAW.** 2 fix upstream issues that had been open a median of 130 days when RapidRoom shipped the fix; 2 of them still open upstream. 4 offered upstream as PRs, 1 merged so far.

| Change                                                                                                             | Type     | By                                     | Upstream                                                                                                                                                |
| ------------------------------------------------------------------------------------------------------------------ | -------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)                                       | platform | [@yojen7](https://github.com/yojen7)   | PR [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813) merged                                                                                     |
| No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … \| head`)                                   | fix      | [@yojen7](https://github.com/yojen7)   | PR [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819) open                                                                                       |
| Exports embed an sRGB ICC profile (JPEG, PNG, TIFF), so colour-managed apps and print services read them correctly | feature  | [@yojen7](https://github.com/yojen7)   | [#1489](https://github.com/CyberTimon/RapidRAW/issues/1489) open 57 d; PR [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) open                |
| Sony lossless M/S raws (A7C II, A7CR) and Canon mRAW/sRAW no longer get green borders ⚑                            | fix      | [@Kheil-Z](https://github.com/Kheil-Z) | [#850](https://github.com/CyberTimon/RapidRAW/issues/850) open 205 d; PR [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) open |
| The Poppins UI font is bundled: no Google Fonts request at startup, and the UI works offline                       | privacy  | [@yojen7](https://github.com/yojen7)   | not yet offered                                                                                                                                         |
| The Clerk sign-in SDK loads only when cloud features are turned on, with telemetry off                             | privacy  | [@yojen7](https://github.com/yojen7)   | not yet offered                                                                                                                                         |
| The update check looks at RapidRoom releases                                                                       | platform | [@yojen7](https://github.com/yojen7)   | RapidRoom only                                                                                                                                          |
| CI builds Android unsigned when no signing key is configured, so forks get green builds                            | ci       | [@yojen7](https://github.com/yojen7)   | not yet offered                                                                                                                                         |

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
