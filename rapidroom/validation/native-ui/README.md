# Native Linux UI smoke test

Issue #134: run the real release-profile RapidRoom app, with its native WebKitGTK
WebView and GPU-processed preview, on a private Weston headless GL compositor. No React
store fixtures, mocked Tauri calls, MCP server, or user desktop are involved.

The optional `native-ui-test` Cargo feature adds the MIT-licensed WebdriverIO
embedded driver, pinned at `tauri-plugin-wdio-webdriver` 1.4.0. Registration also
requires `RAPIDROOM_NATIVE_UI_TEST_PORT` and a GUI launch. It binds to loopback on
Linux. Default builds omit the dependency and listener; headless export and
benchmark runs never register it. The harness uses the WebDriver HTTP protocol
directly, so neither npm WebdriverIO dependencies nor WebKitWebDriver are needed.

## Prerequisites and run

Tested on Arch Linux with Weston 15.0.1, WebKitGTK 2.52.6, GTK 3.24.52, Mesa 26.2.2
and an Intel Lunar Lake GPU. Install Weston (including `weston-screenshooter`),
Bubblewrap, D-Bus, FUSE 3, and Python 3.11+ with Pillow. The normal app build
dependencies and a working GL/Vulkan driver are also required. Weston must
support `--fake-seat`; older compositor releases are not covered by this test.
The machine used for the first run already had every dependency except Weston.

Fetch the existing CC0 regression corpus if it is not present. The smoke uses
only `sony-a7c2-15mp-uncompressed.ARW`, verifies its published SHA-256, and makes
an independent copy before the app touches it.

```sh
python3 rapidroom/regression/baseline.py fetch --raw-dir /path/to/samples/corpus

# Both commands acquire /tmp/rapidroom-build.lock and use nice 10.
# Set CARGO_TARGET_DIR if this machine uses a shared build cache.
python3 rapidroom/validation/native-ui/build.py \
  --out /path/to/samples/native-ui/engine

python3 rapidroom/validation/native-ui/smoke.py \
  --engine /path/to/samples/native-ui/engine/rapidroom \
  --raw-dir /path/to/samples/corpus \
  --out /path/to/samples/native-ui/run-1
```

Output directories must be new and under `samples/`. A failure returns nonzero
and retains the logs, completed steps, DOM evidence and a failure screenshot
when possible. `result.json` is the pass/fail record. Engine, harness and input
hashes are recorded in `environment.json`; source guards confirm the original
RAW hash/mtime and supplied engine remain unchanged. The build helper records
the commit, working diff hash, release profile and feature set inside the lock.

## Minimum scenario

1. Continue the isolated library session and wait for a real native thumbnail.
2. Open that photo and verify a nonblank, settled native preview at 1680×1050.
3. Move Exposure and Contrast through their real mouse handlers. Check the
   displayed values and that compositor preview pixels change.
4. Click Undo until both edits are restored (the app may group quick edits), and
   require zero Exposure/Contrast and exact restoration of
   the preview rectangle's pixels.
5. Open General settings, enable Compact, check its real accessible slider
   layout, and verify the setting was persisted to the isolated settings file.
6. Open Export and choose Original image folder. Export one full-size JPEG into
   the copied fixture's directory and decode it to verify format and dimensions.
7. Close the native window through WebDriver and require exit 0 with no recorded
   frontend exception, rejected promise, console error, native error or panic.

Screenshots capture the entire Weston output. Linux currently disables the
separate WGPU display surface and presents GPU-processed JPEGs in WebKitGTK;
the test follows that normal release path. Screenshot
blankness checks apply to the photo rectangle, not merely a nonempty window.
Actions are DOM mouse/keyboard events sent through the embedded driver; this
does not test physical input devices or every compositor focus behavior.

## Recorded native result

[`evidence.json`](evidence.json) records the successful local release-profile run,
including exact Undo restoration, Compact persistence, full-size JPEG export,
clean exit and zero frontend/native errors. A prototype before the release-info
404 fix completes all interactions but is correctly rejected by the error gate.
Photos and screenshots remain under `samples/`; only measurements and hashes
are committed.

## Isolation, cleanup and extension

Each run has fresh data/config/cache/state directories, a short private
`/run/user/<uid>/rr-smoke-*` runtime directory, its own Wayland socket and session
bus. `DISPLAY`, Xauthority and the user's session-bus address are removed before
launch. The compositor reads no user configuration. Only process groups created
by this run are stopped; its private portal/VFS mounts are detached before its
runtime directory is removed. Photos, exports and captures stay under the owned
case. Bubblewrap makes every other screenshot path read-only.

The test engine is copied while holding the same build/render lock. Bundled
resource directories are symlinked from that engine's resource location; the
minimum scenario does not load an AI model. Extensions that exercise models
must also pin and record those assets. Add a real interaction, explicit state
and screenshot assertions, and a `step()` result entry for new PR scenarios.

This is a local gate. CI has not been enabled: the existing Ubuntu runner's
Weston version and software native presentation need a separate measured run.
macOS, Windows, Android, physical touch, user desktop focus and file-picker
dialogs remain untested. The opt-in driver build supplements the default release
checks and the pixel-exact full60 regression; it does not replace either.

## Optional terminal scenarios

Pass `--terminal` to both helpers for the existing shell/CLI launcher probes and real PTY, docking, resize, collapse and cleanup checks. To test the installed clients through the actual start buttons, build with `--terminal --mcp-clients` and run `smoke.py --terminal-clients` (which implies both options). This opt-in mode makes real model requests. Each installed client reads state and changes only Exposure, once from the built-in PTY and once through the external terminal launcher. It requires the same photo folder and standard XDG config, automatic private endpoint discovery without endpoint/token variables, a real editing-tool event, changed revision/slider/preview, and a closed built-in panel during external launches.

The owned `claude`/`codex` wrapper scripts invoke the real installed binaries in noninteractive regression mode with the documented stdio configuration. User hooks and unrelated tools are disabled per invocation; global settings and logins are preserved. Only the external terminal executable is a launcher stub, so physical desktop terminal focus is untested. Built-in clients must inherit a real PTY. App quit must remove a separate owned background shell job.

[`terminal-evidence.json`](terminal-evidence.json) records all 15 successful Linux steps, including strict response-header CSP with a blocked data-script control, both authenticated wire versions, four real-client edits and clean process exit. Claude Code 2.1.289 changed Exposure to 0.5/1.5 and Codex 0.160.0 to 1.0/2.0 in built-in/external mode. The engine and adapter were built from clean application source `f32324c4`; the launch harness source and hashes are recorded separately. Application and adapter code are unchanged through the evidence commit.

## Optional real MCP clients

Pass `--mcp-clients` to both helpers to build `native-ui-test,mcp` and run the installed `claude` and `codex` CLIs after the minimum editor/export scenario. Python 3.11+ and working existing client logins are required. This opt-in scenario makes real model requests. Each client discovers RapidRoom’s tools and changes only Exposure on the owned CC0 photo; the harness requires an actual editing-tool event, changed revision, exact native slider value and changed native preview. It also checks wire discovery for MCP `2025-06-18` and `2026-07-28`, including required cache hints.

Client configuration is supplied per invocation, with unrelated servers and user hooks disabled. No global client configuration changes. Logs, client versions, results and screenshots remain in the isolated case. This supplements the raw HTTP and unit checks with the real clients’ response validation and tool-call path.

[`mcp-client-evidence.json`](mcp-client-evidence.json) records the successful real-client run: Claude Code 2.1.289 changed Exposure to 0.5 and Codex 0.160.0 changed it to 1.0, each with a recorded MCP editing call and verified revision/slider/preview changes. The app exited with zero frontend/native errors and unchanged fixture/engine guards. The application and adapter were built from `3c187d19`; the client-check harness is hashed separately. Codex grants per-invocation approval only for the two explicitly requested state/edit tools.
