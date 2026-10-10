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

## Optional shared MCP history scenario

Build with `--mcp-clients`, then run `smoke.py --mcp-history` with that pinned engine. It uses the same private fixture and existing client logins. Claude Code makes exactly three Exposure edits, reads labelled history, undoes twice and requests original/comparison/region previews. The harness checks the actual GUI history menu and slider, generated schema resource, bounded JPEG dimensions and unchanged state/revision/history after read-only calls. Codex independently reads context, history and a comparison preview. This opt-in scenario makes real model requests; it does not change global client configuration. It is automated native evidence, and does not satisfy issue #115's separate human live check.

[`mcp-history-evidence.json`](mcp-history-evidence.json) records the 11-step Linux pass. The native app/adapter were built from clean `fedd5e28`, with harness source/hashes recorded separately. Actual Claude Code 2.1.289 made three Exposure edits and undid twice to 0.2; the painted GUI menu shows three AI labels, with the first active and two undone. Codex 0.160.0 read context/history/comparison without changing state. Application code is unchanged through the evidence commit. The required human live check remains pending.

## Linux MCP packaging control

Build with `build.py --terminal --mcp-clients`, then run `smoke.py --mcp-packaging` with a new owned samples directory. This scenario omits `mcpEnabled` from old-style preferences and requires no initial listener/file. It tests Cancel and both launch-mode enable prompts, four real Claude/Codex edits using the app-generated adapter registration, persisted off/on, closure of an accepted connection, fresh keys, unchanged edits/revision/history across a settings-only restart, and endpoint cleanup on app exit. Client login is reused; CLI configuration and photo files remain isolated. These opt-in checks make real model requests.

## Optional read-only measurement scenario

Build with `--mcp-clients`, then run `smoke.py --mcp-measure` using that pinned engine. The scenario creates one real virtual copy through the normal native command before guarding its sidecar. It checks byte-identical repeated variants, exact independent statistics from decoded pre-JPEG PNG pixels, fractional sampling and consistent nested 1:1 native crops. It verifies the copy's exact GUI label in a bounded sheet and refuses invalid variant/geometry/size requests. Actual Claude Code calls all four new read-only tools. Every measurement phase checks unchanged adjustments, revision, history and sidecar bytes/mtime. Logs and the labelled PNG stay under the owned case. This makes real model requests with the client's existing login and restricted per-invocation tools; it does not change persistent client configuration.

[`mcp-measure-evidence.json`](mcp-measure-evidence.json) records the 12-step Linux pass from clean application/harness source `b9ea429e`. Actual Claude Code 2.1.289 called every new tool through authenticated stdio. The bounded sheet shows `A: Current` and `B: smoke.ARW (VC)` and was visually inspected. Pixel statistics, repeated variants, native crop coordinates, unchanged sidecar bytes/mtime, history/revision and clean application exit were all checked. Photos and client logs remain in the owned case.

[`mcp-packaging-evidence.json`](mcp-packaging-evidence.json) records the 18-step Linux pass: four real clients used app-generated registration, both consent modes and the Settings control were inspected in compositor captures, default-off/Cancel/toggle/fresh-key/history guards passed, and clean exit removed the endpoint. The app was built from `cedd0a63`; subsequent changes only affect packaging, notices, harness capture timing and evidence. Production package GUI behavior and physical terminal focus remain unverified.

[`mcp-packages-evidence.json`](mcp-packages-evidence.json) records actual deb/AppImage contents and an additional 8-step native pass with the adapter extracted from the real AppImage. Debian adapter bytes match exactly. AppImage verification permits only its expected local loader path while preserving allocated payloads, dynamic-symbol semantics and library requirements; a modified code-byte control is refused. Both Claude and Codex made real edits through the packaged adapter. The common app was run with the optional native driver; the production package GUI remains unverified.

## Optional dock layout and compact terminal header

Build with --terminal, then run smoke.py --dock-layout. It uses 24 independent
copies of the existing CC0 Sony photo at a logical 1800×1048 viewport. In both
Library and Editor, accessible resize controls step the bottom dock from 120
to 620px and back, and each side dock from 240 to 560px and back. Each step
records viewport/toolbar/dock bounds, the effective painted thumbnail bounds
after ancestor clipping, compact-header height/controls, and a compositor
screenshot. Virtualized overscan rows may extend beyond the scrolling viewport;
their clipped painted bounds must never cover neighbouring chrome.

The scenario checks one 28–32px terminal header and labelled controls contained
inside it, with icon-only assistant buttons below 480px. It exercises new/close
tab, persistent built-in/external selection in the gear menu, both assistant
launchers via owned CLI stubs, and collapse/reopen through public controls.
The normal preview/edit/Undo/Compact/export/PTY smoke follows. No real model
requests or user desktop interaction occur. These DOM keyboard resizes exercise
the native app's actual accessible handlers; physical pointer drags and the
user's fractional desktop scale are not covered.

[dock-layout-evidence.json](dock-layout-evidence.json) records the successful
39-step Linux run from clean application/harness source `8855d8ae`: all 26
resize states have a 32px header, no chrome/viewport overlap, and contained
labelled controls. Tab, gear launch mode, both assistant stubs and PTY
collapse/reopen pass, followed by the complete existing smoke and clean exit.
The negative baseline catches an 11px toolbar/dock overlap and an 86px header;
intermediate controls/scrollbar failures are retained. Four bottom/narrow/wide
compositor captures were visually inspected. Subsequent changes only record
measurements and documentation; photos and captures remain under samples/.

## Optional no-op crop scenario

Build with `--mcp-clients`, then run `smoke.py --crop-noop`. This mode uses
private authenticated MCP reads to guard revision and labelled history; it
makes no real model requests. After the normal preview/edit/Undo steps, it
opens and closes the real Crop view, completes a corner click without movement,
and requires unchanged adjustment state/revision, history and sidecar bytes
and mtime. A real keyboard corner resize must record exactly one user Crop
step; Undo must restore the uncropped state and exact native preview pixels.
Compact, full-size JPEG export and clean exit then complete the normal smoke.
Photos and captures remain under the owned case. Physical input devices,
other compositors, Windows and macOS are untested.

[crop-noop-evidence.json](crop-noop-evidence.json) records the successful
12-step Linux pass from clean application source `5efade09` and harness
`87ce6ad9` (the harness-only change accepts the free corner resize). All three
no-op phases preserve state/revision/history and sidecar bytes/mtime. A real
4150×2614 crop records one user Crop entry; Undo restores the original preview
exactly. Three compositor captures were visually inspected. The retained
negative baseline reproduces the full-frame write and an intermediate run
catches the duplicated Editor callback's empty Undo step. One initial driver
startup timed out before any scenario step; a retry reached the intermediate
negative, and the revised application passed on its first run. No photos or
private session logs are committed.

## Optional advisory read and stale mutation scenario

Build with `--mcp-clients`, then run `smoke.py --mcp-revisions`. After the normal
preview/edit/Undo/Compact/JPEG checks, real Claude and Codex each read editor
context, then a native GUI Exposure change advances revision/history. A second
real client turn sends the literal old revision to all five read-only rendering
tools and a single mutation. Reads must return the latest state; the stale
mutation must fail with current revision, changed keys and the user actor.
Independent authenticated reads compare stale/fresh results and rendered
payloads, check compiled version/source identity, and guard state/revision,
history and sidecar bytes/mtime across reads and refusal. Real calls and exact
stale arguments are verified in client event logs. This mode makes real model
requests. Only the owned CC0 Sony fixture is used; physical input and other
platforms remain untested.

An evicted/unknown expected revision returns `changedKeys: null` with
`changesKnown: false`; it does not claim that the latest history row describes
every change since an arbitrary revision. A change during rendering retains
the read guard with structured conflict details. Source commit/dirty fields
are compiled for MCP builds; archives without Git report unknown identity.

[revisions-evidence.json](revisions-evidence.json) records the 15-step Linux pass
from clean application source `849ccd34`, with harness-only settling commit
`bcf03869`. Eight context requests overlapped actual GUI Exposure edits and all
arrived. Claude Code 2.1.289 and Codex CLI 0.160.0 then each read context in
one turn and sent the literal stale revision to all five reads and one update
after GUI Exposure edits (1.2→0.6 and 0.6→1.2). Reads matched fresh results and
rendered payloads; all six stale mutations were independently refused with
the latest revision, `exposure` and `user`. State, history and sidecar bytes/mtime
were unchanged across reads/refusal. Original/custom previews separated edit
and recipe identity. Both GUI captures were visually inspected; normal smoke,
clean exit and fixture/application guards passed.

An earlier 14-step run passed, but a later payload-baseline run exposed one
lost editor-context request while navigation callbacks changed. The bridge
now keeps its command listener stable and calls the latest navigation handler
through a ref. A deferred-registration regression fails on the old bridge;
all 163 frontend tests pass with the fix. The earlier timeout and regression
logs are retained. The revised native run above needed no startup retry.

Tracked-file build watches keep compiled source identity current after
incremental Rust/frontend edits. An isolated Cargo/Git probe reproduces the
old stale dirty flag and checks clean builds, edits, reversion and a new HEAD.
The revised application includes this correction. Other platforms, physical
input, native evicted-history/mid-render concurrency and archive identity
remain untested.

## Optional compact payload scenario

Build with `--mcp-clients`, then run `smoke.py --mcp-compact`. It prepares three
actual radial masks on the public CC0 Sony fixture, measures UTF-8 JSON result
bytes for default/verbose set, update and reset, summary/opt-in histograms, and
tools/list schemas. Four identical landscape variants exercise the tight
labelled contact sheet and unchanged separate-image pixels. Opt-in statistics
are independently checked against decoded pre-JPEG pixels, with state/history
and sidecar bytes/mtime guarded across reads. Both real clients apply a nested
HSL/grading/parametric patch through the local schemas, receive compact replies,
preserve all three masks, and record one shared assistant history step.

Use `--mcp-compact-baseline` with the prior pinned application to measure the
old replies and square sheet using the same owned fixture and wire requests.
This baseline mode makes no real model requests. Neither mode uses the private
human editing-session photos/log; captures and client logs stay under samples/.
Other platforms and physical input devices remain untested.

[compact-evidence.json](compact-evidence.json) records a 10-step old-payload
baseline at clean application `849ccd34` and the 12-step revised pass at clean
application `96d9f6d5`, with harness-only corrections at `3f0463aa`. Real Claude
Code 2.1.289 and Codex CLI 0.160.0 each applied the exact nested patch, received
a compact reply, kept all three masks, changed the native preview and recorded
one assistant history row. Sorted compact keys and GUI control-order history
keys both match the actual state diff. Both GUI captures and the sheet were
visually inspected; normal smoke, clean exit and source guards passed.

| Actual tools/call result | Before bytes | After bytes |
| ------------------------ | -----------: | ----------: |
| Set/update, three masks  |       37,572 |         610 |
| Reset from three masks   |       10,447 |         670 |
| Analyze default          |       16,164 |       2,851 |
| Sample-region default    |       15,602 |       2,967 |
| tools/list discovery     |       82,710 |      57,200 |

Counts are UTF-8 compact JSON of the entire result, including text and
structured content, excluding JSON-RPC framing and image-token billing.
Separate owned case paths differ by one character, affecting a few bytes.
Verbose replies retain the full authoritative state. Histograms opt in and
keep their exact bins; statistics and percentiles match decoded pixels. The
four-image sheet is 1000×716 instead of 1000×1000; all separate variants remain
1000×667 with identical pixels and complete labels. The contact-sheet layout
changes intentionally.

Failed attempts remain available: an incorrect 30-versus-29 fixture assertion,
the old context delivery gap fixed in #148, driver welcome-screen timeouts
before any step, and a harness assertion that conflated sorted reply keys
with GUI history order. Application code did not change for the last two
harness corrections. Startup now permits one script-timeout retry only for
its first side-effect-free readiness probe, before Continue Session. The
successful final run needed no retry; GUI actions and MCP calls are never
silently retried.

Point Color: pass `--point-color` to pick a swatch on the real canvas, verify
zero-shift pixels are identical, edit its range and Hue Shift, and verify one
Undo restores the shift and exact preview. The usual edit/export/exit checks
follow. Uses an owned sample copy and native GPU processing.
