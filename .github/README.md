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

## What's in v0

v0 is upstream `main` plus:

| Change                                                                                                                                                                     | Upstream status                                                                                      |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Tauri 2.12: Native Titlebar works with tiling Wayland compositors (Hyprland)                                                                                               | merged upstream as [#1813](https://github.com/CyberTimon/RapidRAW/pull/1813)                         |
| No abort or hang when stdout/stderr is a closed pipe (e.g. `rapidraw … \| head`)                                                                                           | [#1819](https://github.com/CyberTimon/RapidRAW/pull/1819)                                            |
| Exports embed an sRGB ICC profile (JPEG, PNG and TIFF), so colour-managed apps and print services read them correctly                                                      | [#1820](https://github.com/CyberTimon/RapidRAW/pull/1820) (JPEG/PNG)                                 |
| Sony lossless M/S raws (tested: A7C II, A7CR) no longer get green borders; the same fix covers Canon mRAW/sRAW ([#850](https://github.com/CyberTimon/RapidRAW/issues/850)) | rawler fix from [RapidRAW-DngLab#7](https://github.com/CyberTimon/RapidRAW-DngLab/pull/7) by Kheil-Z |
| The Poppins UI font is bundled, so startup doesn't contact Google Fonts and the UI works offline                                                                           | not yet proposed                                                                                     |
| The Clerk sign-in SDK loads only if you turn on cloud features, with its telemetry off                                                                                     | not yet proposed                                                                                     |
| The update check looks at RapidRoom releases                                                                                                                               | RapidRoom only                                                                                       |

**Privacy:** at startup, v0 contacts only `api.github.com` for its update check. AI models, community presets and the map are still fetched only when you use those features, as in upstream.

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
