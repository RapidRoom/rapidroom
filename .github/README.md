# RapidRoom

**The community build of [RapidRAW](https://github.com/CyberTimon/RapidRAW).**

RapidRAW is a fast, GPU-accelerated, non-destructive raw photo editor by [Timon Käch (CyberTimon)](https://github.com/CyberTimon), licensed AGPL-3.0. RapidRoom is the same app, kept close to upstream, plus fixes and features gathered from the many people building on it. Every change is credited to the person who wrote it.

> RapidRoom is an independent community project. It is not affiliated with or endorsed by CyberTimon or the RapidRAW project. Please report RapidRoom bugs here, not upstream.

## Why this exists

RapidRAW has hundreds of forks. Many are active, many are AI-assisted, and a lot of them fix the same problems separately. RapidRoom is a shared place for that work:

- **One repo for the best of the forks.** Good fixes and features are collected in one tested build instead of being spread across dozens of repos.
- **AI-assisted work is welcome.** It's held to the same gates as any other change, and you say how it was made. See [CONTRIBUTING](CONTRIBUTING.md).
- **We stay friendly with upstream.** We merge upstream regularly and offer our fixes back as pull requests. If the projects drift apart over time, that's fine too. The point is good software for photographers.
- **Every harvest is credited.** Original authorship is kept on commits, and every source is listed in [CREDITS.md](../CREDITS.md).

## What RapidRoom adds

Everything RapidRoom has on top of upstream RapidRAW, kept up to date from [rapidroom/changes.json](../rapidroom/changes.json):

<!-- rapidroom-changes:start -->

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

⚑ changes rendered output on purpose. Upstream status as of 2026-10-03. Full list with sources: [CHANGES.md](../CHANGES.md).
<!-- rapidroom-changes:end -->

**Privacy:** at startup, RapidRoom contacts only `api.github.com` for its update check. AI models, community presets and the map are still fetched only when you use those features, as in upstream.

**Rendering:** v0 renders a 60-image regression set (20 raws × 3 edits) pixel-identical to upstream with the #850 fix applied.

## Install

There are no RapidRoom release builds yet. Build from source on any platform RapidRAW supports. You need [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org/):

```bash
git clone https://github.com/RapidRoom/rapidroom.git
cd rapidroom
npm install
npm run tauri build
```

On Linux, Tauri also needs the WebKitGTK development packages; see the [Tauri prerequisites](https://tauri.app/start/prerequisites/). For everything else (features, tethering, CLI, system requirements), see the upstream [RapidRAW README](../README.md).

## Contributing

Bug reports, fixes and harvests from other forks are all welcome. Read [CONTRIBUTING](CONTRIBUTING.md) first. If you maintain a RapidRAW fork and would like your work in here, or would like to help maintain RapidRoom, open an issue. See [GOVERNANCE](GOVERNANCE.md) for how decisions are made.

## Support upstream

RapidRoom only exists because of RapidRAW. If it's useful to you, consider supporting CyberTimon on [Ko-fi](https://ko-fi.com/cybertimon).

## License

[AGPL-3.0](../LICENSE), the same as RapidRAW. Every RapidRoom change is AGPL-3.0, and the source is always public.
