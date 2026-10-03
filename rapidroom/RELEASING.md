# Releasing RapidRoom

Linux only for now: a `.deb` and an AppImage, built on Ubuntu 22.04 (x86_64). Windows and macOS builds come later.

## Versions

There are two version numbers:

| What                     | Where                                           | Example          |
| ------------------------ | ----------------------------------------------- | ---------------- |
| RapidRoom version        | `RAPIDROOM_VERSION` in `src/utils/rapidroom.ts` | `0.1.0`          |
| RapidRAW base (upstream) | `version` in `src-tauri/tauri.conf.json`        | `1.6.4`          |
| Release tag              | `v` + RapidRoom version                         | `v0.1.0`         |
| Package version          | base + `+rr.` + RapidRoom version               | `1.6.4+rr.0.1.0` |

- `RAPIDROOM_VERSION` is plain `MAJOR.MINOR.PATCH`. The update check strips the `v` from the latest release's tag and compares the numbers with `RAPIDROOM_VERSION`, so suffixes like `-rc.1` don't compare. The release workflow refuses a tag that isn't exactly `v<RAPIDROOM_VERSION>`.
- Don't change `version` in `tauri.conf.json` for a RapidRoom release; it follows upstream. The workflow sets the package version to `<base>+rr.<RapidRoom version>` at build time, so the `.deb` version goes up with every RapidRoom release (and with every upstream sync), and `apt`/`dpkg` upgrade in place. The start screen shows the RapidRAW base without the `+rr.` part.
- The update check only sees published, non-prerelease releases (`/releases/latest`). Drafts are invisible to users.

## Steps

1. On `main`, bump `RAPIDROOM_VERSION` in `src/utils/rapidroom.ts` in a PR, and merge it.
2. Tag the merge commit and push the tag:

   ```sh
   git checkout main && git pull
   git tag -a v0.1.0 -m "RapidRoom 0.1.0"
   git push origin v0.1.0
   ```

3. The **RapidRoom release: Linux** workflow (`.github/workflows/rapidroom-release-linux.yml`) checks the tag, builds, and attaches these to a **draft** release named after the tag:
   - `RapidRoom_v0.1.0_amd64.deb`
   - `RapidRoom_v0.1.0_amd64.AppImage`
   - `SHA256SUMS`

   It never publishes. If the draft already exists, it replaces the assets; if the release is already published, it fails and leaves it alone.

4. Download both packages and test them on a machine that also has stock RapidRAW installed (see the checklist below).
5. Edit the draft's notes (the new entries in `CHANGES.md` since the last release), then publish it. From then on the update check offers it.

If the build fails, fix it on `main`, then move the tag (`git tag -f -a v0.1.0 …` and `git push -f origin v0.1.0`) or bump to the next version. Delete a broken draft by hand.

Push tags only. Don't create the release in the GitHub UI: that triggers upstream's `release.yml` (`release: created`), which builds every platform with upstream's naming.

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

Tauri's resource directory on Linux is `<exe dir>/../lib/<productName>`, falling back to `$APPDIR/usr/lib/<productName>` and then `/usr/lib/<productName>`. With `productName` set to `RapidRoom` none of these can be RapidRAW's. The workflow fails if the `.deb` contains any path with `RapidRAW` in it.

**Still shared:** the app identifier stays `io.github.CyberTimon.RapidRAW`, so settings, presets, the library database, thumbnails, logs and the WebView storage live in the same folders as RapidRAW's (`~/.config/io.github.CyberTimon.RapidRAW`, `~/.local/share/io.github.CyberTimon.RapidRAW`, `~/.cache/io.github.CyberTimon.RapidRAW`). That is on purpose for now, so existing users keep their settings. Moving to a RapidRoom identifier needs a settings migration and is a separate issue.

## Test checklist (before publishing)

With stock RapidRAW installed:

- [ ] `sudo apt install ./RapidRoom_v0.1.0_amd64.deb` installs without conflicts and without removing RapidRAW.
- [ ] `dpkg -L rapid-room` lists `/usr/bin/rapidroom`, `/usr/lib/RapidRoom/…` and `/usr/share/applications/RapidRoom.desktop`, and nothing named RapidRAW.
- [ ] Both apps can run at the same time; starting one doesn't just focus the other.
- [ ] The taskbar/dock shows the RapidRoom icon for RapidRoom's window, on X11 and on Wayland (`xprop WM_CLASS` shows `"RapidRoom", "RapidRoom"`).
- [ ] AI masks work (ONNX Runtime loads from `/usr/lib/RapidRoom/resources`), film LUTs list, and lens correction finds lenses.
- [ ] The AppImage runs on its own (`chmod +x`, run it) with the same checks.
- [ ] The start screen shows `RapidRoom 0.1.0` and `RapidRAW 1.6.4`.
- [ ] Upgrading from the previous RapidRoom `.deb` replaces it in place.
