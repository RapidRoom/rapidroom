# Releasing RapidRoom

Linux only for now: a `.deb` and an AppImage, built on Ubuntu 22.04 (x86_64). Windows and macOS builds come later.

## Versions

RapidRoom has its own version, `MAJOR.MINOR.PATCH`, independent of upstream. It started at **2.2.0**.

| What                                    | Where                                                                                                                             | Example  |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | -------- |
| RapidRoom version                       | `RAPIDROOM_VERSION` in `src/utils/rapidroom.ts` **and** `version` in `src-tauri/tauri.conf.json` (a Vitest test keeps them equal) | `2.2.0`  |
| Release tag                             | `v` + RapidRoom version                                                                                                           | `v2.2.0` |
| Package version (.deb, AppImage, About) | the RapidRoom version                                                                                                             | `2.2.0`  |
| RapidRAW base (upstream)                | `RAPIDRAW_BASE_VERSION` in `src/utils/rapidroom.ts`                                                                               | `1.6.4`  |

- The start screen shows `RapidRoom 2.2.0 - community build of RapidRAW 1.6.4`.
- The update check strips the `v` from the latest release's tag and compares the numbers with `RAPIDROOM_VERSION`, so suffixes like `-rc.1` don't compare. The release workflow refuses a tag that isn't exactly `v<RAPIDROOM_VERSION>`, or a `tauri.conf.json` version that differs.
- **Upstream syncs:** upstream bumps `version` in `tauri.conf.json` on its releases. Keep RapidRoom's value there, and set `RAPIDRAW_BASE_VERSION` to the upstream release the sync brings in.
- The update check only sees published, non-prerelease releases (`/releases/latest`). Drafts are invisible to users.

### Which number to bump

Look at the entries in `rapidroom/changes.json` (and merged PRs) since the last tag:

| Bump                      | When                                                                                                                                                                                                                             | Example                                                  |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| **PATCH** (2.2.0 → 2.2.1) | Only fixes: crashes, wrong behaviour, performance, packaging. No new features or settings, and **no deliberate change to how existing photos render**. Safe to take without reading the notes.                                   | a crash fix, a broken export option                      |
| **MINOR** (2.2.x → 2.3.0) | Anything else that keeps compatibility: new features or settings, harvests, upstream syncs, and deliberate rendering changes (entries with `rendering_change: true`, marked ⚑; the regression reference is re-blessed for them). | Reference View, an upstream sync that changes highlights |
| **MAJOR** (2.x → 3.0.0)   | A break: edits or sidecars that older RapidRoom/RapidRAW can't read, settings or catalog data that must be migrated or reset, a removed feature or platform. Rare, and announced ahead.                                          | a new sidecar format                                     |

When in doubt between two, take the bigger one. Release notes list rendering changes first, so nobody is surprised by an edit that looks different.

### When to release

- **Minor:** when `main` has a verified batch worth shipping, roughly every one to two weeks while development is busy. Everything on `main` has already passed CI and the 60-image regression check, so a release is a tag, not a freeze.
- **Patch:** any time, for a regression or a bad bug in the latest release. Branch from the release tag if `main` already has minor-level changes, fix there, tag `vX.Y.Z+1`, and merge the fix into `main` too.
- Between releases, `main` keeps the last released number; the bump PR is the first step of the next release.

## Steps

1. On `main`, bump `RAPIDROOM_VERSION` in `src/utils/rapidroom.ts` in a PR, and merge it.
2. Tag the merge commit and push the tag:

   ```sh
   git checkout main && git pull
   git tag -a v2.2.0 -m "RapidRoom 2.2.0"
   git push origin v2.2.0
   ```

3. The **RapidRoom release: Linux** workflow (`.github/workflows/rapidroom-release-linux.yml`) checks the tag, builds, and attaches these to a **draft** release named after the tag:
   - `RapidRoom_v2.2.0_amd64.deb`
   - `RapidRoom_v2.2.0_amd64.AppImage`
   - `SHA256SUMS`

   It never publishes. If the draft already exists, it replaces the assets; if the release is already published, it fails and leaves it alone.

4. Download both packages and test them on a machine that also has stock RapidRAW installed (see the checklist below).
5. Edit the draft's notes (the new entries in `CHANGES.md` since the last release), then publish it. From then on the update check offers it.

If the build fails, fix it on `main`, then move the tag (`git tag -f -a v2.2.0 …` and `git push -f origin v2.2.0`) or bump to the next version. Delete a broken draft by hand.

Push the tag; don't create the release by hand. Upstream's `release.yml` (all platforms, triggered by `release: created`) only runs in `CyberTimon/RapidRAW`.

## What makes it install next to RapidRAW

Set in `src-tauri/tauri.linux.conf.json` and `src-tauri/src/lib.rs`:

| Thing                                           | RapidRAW                                       | RapidRoom                                         |
| ----------------------------------------------- | ---------------------------------------------- | ------------------------------------------------- |
| Debian package                                  | `rapid-raw`                                    | `rapid-room`                                      |
| Binary                                          | `/usr/bin/RapidRAW`                            | `/usr/bin/rapidroom`                              |
| Resources (ONNX Runtime, film LUTs, lensfun DB) | `/usr/lib/RapidRAW`                            | `/usr/lib/RapidRoom` (AppImage: inside the image) |
| Desktop entry                                   | `RapidRAW.desktop`                             | `RapidRoom.desktop`, `StartupWMClass=RapidRoom`   |
| Icon                                            | `RapidRAW`                                     | `rapidroom`                                       |
| Window class / Wayland app ID                   | `RapidRAW`                                     | `RapidRoom`                                       |
| Single-instance D-Bus name                      | `io.github.CyberTimon.RapidRAW.SingleInstance` | `io.github.RapidRoom.RapidRoom.SingleInstance`    |

Tauri's resource directory on Linux is `<exe dir>/../lib/<productName>`, falling back to `$APPDIR/usr/lib/<productName>` and then `/usr/lib/<productName>`. With `productName` set to `RapidRoom` none of these can be RapidRAW's. The workflow fails if the `.deb` is missing `/usr/bin/rapidroom`, `RapidRoom.desktop` or the resources under `/usr/lib/RapidRoom`, or contains any path with `RapidRAW` in it.

**Still shared:** the app identifier stays `io.github.CyberTimon.RapidRAW`, so settings, presets, the library database, thumbnails, logs and the WebView storage live in the same folders as RapidRAW's (`~/.config/io.github.CyberTimon.RapidRAW`, `~/.local/share/io.github.CyberTimon.RapidRAW`, `~/.cache/io.github.CyberTimon.RapidRAW`). That is on purpose for now, so existing users keep their settings. Moving to a RapidRoom identifier needs a settings migration and is a separate issue.

## MCP in Linux packages

`python3 rapidroom/build-linux-packages.py` builds the existing stdio adapter, stages the host-suffixed sidecar, and builds both bundles with `terminal,mcp` under the shared build lock and four Cargo jobs. The official workflow uses this helper. `tauri.mcp-linux.conf.json` adds only the adapter; package names, desktop identity and resource paths remain as above. No dependency or licence is added by this packaging change.

Both packages contain `usr/bin/rapidroom-mcp-stdio`. AI control is off by default, with no listener or endpoint file. Settings **Let AI assistants control RapidRoom** and the first assistant launch offer enable control; disabling stops it and removes the endpoint. Each enable/start uses a fresh private key. See [MCP.md](MCP.md) for per-launch registration and external-plugin setup. Keep the adapter with the app when making a user-local install.

## Test checklist (before publishing)

With stock RapidRAW installed:

- [ ] `sudo apt install ./RapidRoom_v2.2.0_amd64.deb` installs without conflicts and without removing RapidRAW.
- [ ] `dpkg -L rapid-room` lists `/usr/bin/rapidroom`, `/usr/lib/RapidRoom/…` and `/usr/share/applications/RapidRoom.desktop`, and nothing named RapidRAW.
- [ ] Both apps can run at the same time; starting one doesn't just focus the other.
- [ ] The taskbar/dock shows the RapidRoom icon for RapidRoom's window, on X11 and on Wayland (`xprop WM_CLASS` shows `"RapidRoom", "RapidRoom"`).
- [ ] AI masks work (ONNX Runtime loads from `/usr/lib/RapidRoom/resources`), film LUTs list, and lens correction finds lenses.
- [ ] Both packages contain an executable stdio adapter; AI control initially has no endpoint/listener, and enable/cancel/disable work. Start Claude/Codex uses the installed adapter and a real client edit reaches the slider and preview.
- [ ] The AppImage runs on its own (`chmod +x`, run it) with the same checks.
- [ ] The start screen shows `RapidRoom 2.2.0` and `RapidRAW 1.6.4`.
- [ ] Upgrading from the previous RapidRoom `.deb` replaces it in place.
