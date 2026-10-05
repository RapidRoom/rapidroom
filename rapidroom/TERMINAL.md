# A terminal in RapidRoom: design note

Issue #119: a real shell inside RapidRoom, so you can run `claude` or `codex` beside the photo you are editing. The implemented panel follows Josh's decisions below. The original design measurements and alternatives remain as historical context.

The panel requires Cargo feature `terminal`. It uses the existing left/right layout and a resizable bottom dock; tabs and font/shell preferences persist, while shells and transcripts restart fresh. Start buttons run the ordinary installed clients. Choose built-in panel or my terminal in the launcher. **Open Terminal Here** remains available from the folder menu.

For AI editing, use a build with `mcp` as well and install/configure the separate `rapidroom-mcp-stdio` adapter or the [RapidRoom plugin](plugin/README.md). Follow [MCP.md](MCP.md) for client registration. The adapter discovers the private per-user endpoint in either terminal; no token or endpoint variable is needed. Independent terminal sessions use the same registration. Codex must forward standard `XDG_CONFIG_HOME` if it is customized. Terminal-only builds provide the shell without MCP tools. Official packaging is handled separately from this implementation.

## Approved decisions (2026-10-04)

Josh chose option **A**: the terminal belongs in the existing movable and resizable panel layout inside the main React webview. Clerk sign-in is disabled and scripts are local only. This replaces the earlier recommendation for a separate window. The private per-user endpoint discovery, shell-typed start buttons, persistent layout and tabs, and font and shell settings are required. Official release builds must enable the terminal.

Built-in and external terminals must use the same folder, environment, MCP tools and per-user endpoint discovery. A setting chooses where Start Claude/Codex opens. The external path must work without inherited environment variables. MCP/auth integration from #102/#116 is verified in the 15-step Linux native scenario: real Claude Code and Codex each edit from both launch paths, external mode keeps the panel closed, and app quit removes the owned PTY background process. A closed terminal tab does not restore a running process after restart; saved tabs reopen as fresh shells, with no saved transcript or automatic assistant command.

The strict CSP and Clerk-off prerequisite is implemented. The current implementation adds optional portable-pty 0.9.0 and xterm 6.0.0 with addon-fit 0.11.0, persistent tab metadata, the existing sidebar docking plus a full-width bottom dock, font/shell preferences and built-in/external start controls. Linux PTY cleanup covers owned session processes, including ordinary background jobs. macOS cleanup covers the shell and its foreground process group; descendant cleanup on macOS/Windows remains unverified. The optional Linux native extension passes all 11 editor/PTY/launcher/cleanup steps with unchanged fixture and engine guards and zero frontend/native errors; see [`terminal-evidence.json`](validation/native-ui/terminal-evidence.json). It uses assistant and external-terminal stubs to verify commands, folder and environment. Authenticated endpoint/stdio parity and real client interoperability remain pending; do not merge #119 before #102/#116 integration and those checks. Earlier recommendations below are historical design notes.

## 1. Stopgap: Open Terminal Here (in this PR)

Right-click a folder in the folder tree → **Open Terminal Here**. Rust command `open_terminal_here` (`src-tauri/src/terminal.rs`):

- **Linux:** `$TERMINAL` first, then [`xdg-terminal-exec`](https://github.com/Vladimir-csp/xdg-terminal-exec) (the XDG default-terminal launcher), then Ghostty, foot, kitty, Alacritty, WezTerm, Ptyxis, GNOME Terminal, GNOME Console, Konsole, Xfce Terminal, Tilix, `x-terminal-emulator`, xterm. The first one installed wins. `xdg-terminal-exec` gets its documented `--dir=<folder>` option, so an already running terminal is given the folder explicitly. Each gets its own working-directory flag, because single-instance terminals (Ghostty, kitty, GNOME's) can open the window in an existing process that has a different working directory.
- **macOS:** `open -a Terminal <folder>`.
- **Windows:** Windows Terminal (`wt -d <folder>`), otherwise a new `cmd` window in the folder.
- **Environment:** `RAPIDROOM_VERSION` and `RAPIDROOM_FOLDER`. The MCP URL and the endpoint from #116 are added here when #116 lands, so an assistant started in that terminal finds the running app.
- **AppImage:** the AppImage's `LD_LIBRARY_PATH`, GTK/GIO module paths, and its directory descendants in `PATH`, `XDG_DATA_DIRS` and `XDG_CONFIG_DIRS` (sibling paths with a shared name prefix are retained) are removed, so neither the terminal nor the tools you run in it load RapidRoom's bundled libraries.
- **Flatpak:** returns a clear error. A sandboxed app can't start a host terminal without `flatpak-spawn --host`, which is a full sandbox escape. RapidRoom doesn't ship a Flatpak today.
- **Security:** the webview sends only a folder path. It must be an existing absolute directory. The terminal program comes from a fixed list or from `$TERMINAL` in the process environment, never from settings, so script in the webview can't choose what runs.

Limits: environment variables don't reach a single-instance terminal that is already running (Ghostty, kitty `--single-instance`, GNOME Terminal), or Terminal.app on macOS. The folder still does. `$TERMINAL` must be a program name or path, not a command line.

Tested: unit tests for argument building, AppImage environment cleanup, the fallthrough past missing terminals, and a real spawn that checks the working directory and both variables (`cargo test --lib terminal::`). **Untested:** an actual terminal window on any desktop, macOS and Windows.

## 2. The CSP has to come first

### What it is now

`tauri.conf.json` has `"csp": null`, which means no Content-Security-Policy. Any script that gets into the webview, from any origin, runs with the full Tauri command surface: delete, move and write files, change settings, open URLs. A terminal would add "run any command", with nothing in the way.

### What the frontend actually loads (measured)

Checked on the production build (`npm run build`), statically and in Chromium under a CSP. A local integration recheck at `d364a964` reproduces the four start-screen results with no page errors; editor and native-terminal behavior are not covered by this harness. The harness is in [`validation/csp/`](validation/csp/README.md).

| Directive     | Needed for                                                                                                                                                                                                      | Source                                                             |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `script-src`  | the app bundle only. No `eval` or `new Function` at runtime (lodash's `Function("return this")` is short-circuited by `self`). Tauri hashes every JS asset into `script-src` itself.                            | `dist/assets/index-*.js`                                           |
| `style-src`   | the CSS bundle, **plus runtime `<style>` elements**: react-toastify v11 injects its whole stylesheet, framer-motion's `popLayout` (Masks and AI panels) injects one per animation, and xterm.js would add more. | measured: `style-src 'self'` blocks react-toastify's style         |
| `img-src`     | thumbnails over the asset protocol (`asset://localhost`, `http://asset.localhost` on Windows), `blob:` previews everywhere, `data:` SVG cursors in the canvas                                                   | `useTauriListeners.ts`, `useImageProcessing.ts`, `ImageCanvas.tsx` |
| `font-src`    | bundled Poppins (`@fontsource`), no `data:` fonts in the CSS                                                                                                                                                    | `dist/assets/*.woff2`                                              |
| `connect-src` | Tauri IPC (`ipc://localhost`, `http://ipc.localhost`); the update check `https://api.github.com`; the community-presets sample image `https://raw.githubusercontent.com`                                        | `MainLibrary.tsx`, `CommunityPage.tsx`                             |
| `frame-src`   | the GPS map in the metadata panel, `https://www.openstreetmap.org`                                                                                                                                              | `MetadataPanel.tsx`                                                |
| none          | workers, WebAssembly, `<object>`, forms                                                                                                                                                                         | not used                                                           |

**Cloud AI (upstream's, off by default)** is the exception. With the AI provider set to "cloud", `ClerkProvider` loads Clerk's JavaScript **from Clerk's servers into the app's own webview**: `https://brief-seasnail-12.clerk.accounts.dev/npm/@clerk/clerk-js@6/dist/clerk.browser.js` (measured). Clerk also needs `img.clerk.com`, Cloudflare Turnstile (`challenges.cloudflare.com`) for bot protection, and `blob:` workers. In the current code the publishable key is commented as a "local dev key" and the usage API is `http://127.0.0.1:5000`, so it looks unfinished upstream.

### Proposed policy

```json
"security": {
  "csp": {
    "default-src": "'self'",
    "script-src": "'self'",
    "style-src": "'self' 'unsafe-inline'",
    "img-src": "'self' asset: http://asset.localhost blob: data:",
    "font-src": "'self'",
    "connect-src": "'self' ipc: http://ipc.localhost https://api.github.com https://raw.githubusercontent.com",
    "frame-src": "https://www.openstreetmap.org",
    "worker-src": "'none'",
    "object-src": "'none'",
    "base-uri": "'none'",
    "form-action": "'none'"
  },
  "devCsp": "… the same, plus connect-src ws://localhost:1420 for Vite's hot reload"
}
```

- **`'unsafe-inline'` for styles only.** Three libraries inject `<style>` at runtime, and xterm.js can't take a nonce. Inline style can restyle the page but can't run code. Tauri adds a nonce to `style-src` only when `index.html` contains a `<style>` element, which would make browsers ignore `'unsafe-inline'`. Ours has none. To keep it that way, add `"dangerousDisableAssetCspModification": ["style-src"]`.
- **`script-src 'self'` with no remote hosts.** That's the line that matters for a terminal.

### What breaks

| Breaks                   | Why                                                                          | Options                                                                                                                                                                                            |
| ------------------------ | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cloud AI sign-in (Clerk) | remote script blocked (measured)                                             | (a) leave it broken in RapidRoom; (b) allow Clerk's hosts, and the terminal must then live in a separate webview (§4); (c) move sign-in to a separate window that has no terminal or file commands |
| Nothing else found       | start screen renders with no violations under the proposed policy (measured) | the editor, masks, GPS map and community page are covered by the static review above, not by the harness                                                                                           |

Not affected: links that open in the browser (navigation isn't governed by CSP), and Rust-side network calls (AI connector, community presets list, model downloads), which the CSP doesn't cover.

The CSP change is its own small PR, before any terminal code, with the harness as its test: `proposed` must show no violations, and the build must not contain a remote `script-src`.

## 3. The panel

### Pieces

| Piece              | Choice                                | Size (measured)                                                                                                                                                                                       | Licence |
| ------------------ | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| Terminal emulator  | `@xterm/xterm` 6 + `@xterm/addon-fit` | 345 KB minified, 88 KB gzip; 7 KB CSS                                                                                                                                                                 | MIT     |
| GPU renderer (opt) | `@xterm/addon-webgl`                  | +126 KB minified (471 KB with it, 123 KB gzip)                                                                                                                                                        | MIT     |
| PTY                | `portable-pty` 0.9 (WezTerm's)        | about 200 KB stripped on Linux x86_64; 5 new crates (`portable-pty`, `filedescriptor`, `serial2`, `shell-words`, `downcast-rs`, plus `shared_library` on Windows); a second `nix` (0.28, beside 0.31) | MIT     |

For scale, today's bundle is 3.0 MB of JS. Checked here: `portable-pty` starts a shell in a PTY with a given working directory and environment, and reads its output (Linux).

Not chosen:

- **`tauri-plugin-pty`** exposes spawn and write to JavaScript as plugin permissions. Our own ~200-line module keeps control of environment, working directory and session ownership.
- **`alacritty_terminal`** is an emulator in Rust, so we'd still need a renderer in the webview.

### Process model

- Rust owns the sessions: `pty_open { cols, rows } → id`, `pty_write { id, data }`, `pty_resize { id, cols, rows }`, `pty_close { id }`. Output streams to the webview over a Tauri `Channel`, in batches.
- The shell is the user's own (`CommandBuilder::new_default_prog()`): `$SHELL` started as a login shell on Unix, so `claude` and `codex` are on `PATH`; `%ComSpec%` (cmd.exe) on Windows, where PowerShell would be an explicit choice. A login shell matters on macOS, where apps started from Finder get a minimal `PATH`.
- **Start Claude Code** / **Start Codex** buttons type `claude⏎` or `codex⏎` into the current tab. They don't spawn anything themselves, so there is no "run this program" command, and you see exactly what runs.
- Closing a tab or quitting the app sends SIGHUP to the process group (Windows: closes the ConPTY).
- Global shortcuts already ignore a focused `TEXTAREA` (`useKeyboardShortcuts.ts`), and xterm.js types into a hidden textarea, so editor shortcuts don't fire while you type in the terminal. Escape and Ctrl+Z need checking, though.

### Working directory follows the library

- New tabs start in the library's current folder (`currentFolderPath`), or in the home directory when no folder is open. That's VS Code's behaviour.
- A running shell can't have its directory changed from outside without typing into it. The panel shows a small **cd here** button when the shell's directory (reported by OSC 7, which most shells' prompts send) differs from the library folder. It types `cd -- '<folder>'` with the folder shell-quoted.
- Rust tracks the current folder itself (set when the library loads one), so the terminal webview never trusts a path sent by the main webview.
- The other direction, where the library follows a `cd` in the terminal, is possible with OSC 7 but isn't proposed.

### Environment

`RAPIDROOM_VERSION`, `RAPIDROOM_FOLDER`, and, with #116, `RAPIDROOM_MCP_URL` plus either:

- **`RAPIDROOM_MCP_ENDPOINT`**, the path of the 0600 endpoint file that holds the token (recommended). The secret stays out of every child's environment, crash reports and `/proc/<pid>/environ`, and the stdio proxy from #116 already reads that file; or
- **`RAPIDROOM_MCP_TOKEN`**, the token itself, as the issue first said. Simpler for `curl`.

Same AppImage cleanup as the stopgap.

## 4. Security

**Threat:** whoever can call `pty_write` can run any command as you. The terminal adds no rights you don't have, but it turns "script in the webview" from "can damage the library" into "owns the account".

Ways foreign script could reach the webview, and the answer to each:

1. **Remote scripts.** Today nothing blocks them (`csp: null`); Clerk is the only one loaded. → `script-src 'self'` (§2).
2. **XSS through rendered data**: file names, EXIF and XMP text, preset names from the community repo, AI and MCP responses. React escapes text; the only raw-HTML sink is a static Simple Icons SVG (`CommunityPage.tsx`). → Keep it that way: lint rule against `dangerouslySetInnerHTML`, and the CSP as a second wall.
3. **Frames and navigation.** The OSM iframe is cross-origin and has no IPC. Tauri gives remote origins no commands unless a capability lists them under `remote`. → Keep it so. Deny top-level navigation away from the app with `on_navigation`.
4. **Terminal output.** xterm.js renders escape sequences, never HTML. → Off by default: OSC 52 clipboard writes. Ask before opening: OSC 8 hyperlinks, which only accept `http(s)`.
5. **MCP.** → The MCP server from #5/#116 gets **no** terminal tools. An agent driving the terminal that runs the agent is a loop with no human in it.

**The boundary: a separate webview with its own capability.** Today any app command is callable from any local webview. Declaring the PTY commands in `build.rs` (`tauri_build::AppManifest::new().commands(&["pty_open", …])`) puts them under Tauri's ACL. Only a `terminal` capability for a `terminal` webview grants them, and each command also checks the caller's label. The terminal webview loads its own small page (`terminal.html`) with an extra meta CSP (`connect-src ipc: http://ipc.localhost`, no frames, no remote anything), so it stays tight even if the main webview's policy is loosened for Clerk.

Where that webview lives:

| Option                              | Docked                  | Isolation                            | Cost                                                                                                      |
| ----------------------------------- | ----------------------- | ------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| A. Panel inside the main webview    | yes                     | none: the whole UI can reach the PTY | simplest; acceptable only with the strict CSP and Clerk off                                               |
| B. Child webview in the main window | yes (we position it)    | own capability                       | needs Tauri's `unstable` feature (`Window::add_child`); we lay it out by hand and handle resize and focus |
| C. Separate window                  | no (tiling WMs dock it) | own capability                       | stable Tauri; on Hyprland or Sway it tiles beside the editor by itself                                    |

**Recommendation:** C behind the cargo feature first. It's the smallest step with real isolation, and it works on stable Tauri. B later, if a docked panel inside the window matters enough to take `unstable`. A only if Josh accepts that the terminal shares the UI's trust.

## 5. Platforms and packaging

| Platform           | PTY                                          | Notes                                                                                                                                                                                                                                      |
| ------------------ | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Linux, Wayland/X11 | `openpty`, independent of the display server | WebKitGTK renders xterm.js with the DOM renderer; make WebGL opt-in, because the app already works around NVIDIA/Wayland GPU issues (`tauri-plugin-wayland-nvidia-quirk`). IME and dead keys go through xterm's hidden textarea; untested. |
| AppImage           | as Linux                                     | sanitize the environment (as the stopgap does); shells die with the app, because the PTY master closes                                                                                                                                     |
| .deb               | as Linux                                     | nothing special                                                                                                                                                                                                                            |
| Flatpak            | inside the sandbox only                      | the shell would see no host tools (`claude` and `codex` missing), and `flatpak-spawn --host` is a sandbox escape. Build without the feature. `packaging/` holds upstream's manifest; RapidRoom doesn't ship a Flatpak.                     |
| macOS              | `openpty`                                    | login shell for `PATH`; not sandboxed, because it isn't an App Store build; RapidRoom doesn't ship macOS builds yet                                                                                                                        |
| Windows            | ConPTY (Windows 10 1809+)                    | cmd.exe by default (PowerShell is a choice to make); RapidRoom doesn't ship Windows builds yet                                                                                                                                             |

## 6. Step 2, after the decisions

1. **CSP PR**: §2 policy and `devCsp`, plus the harness as its test (no violations; no remote `script-src`), run in CI next to the build.
2. **Terminal PR**, behind a cargo feature `terminal`, off by default, with the npm side loaded dynamically so the default bundle doesn't grow:
   - `src-tauri/src/terminal_pty.rs` (sessions, env, cleanup) with unit tests: spawn in a folder, environment, resize, close kills the group;
   - the PTY commands in the ACL, a `terminal` capability and webview, `terminal.html` with its meta CSP;
   - the panel: tabs, **Start Claude Code** / **Start Codex**, **cd here**, fit on resize;
   - done when the default build is byte-for-byte unchanged, it works on Linux (Wayland and X11, .deb and AppImage), and a test shows the main webview can't call `pty_write`.

## Decisions for Josh

1. **Cloud AI sign-in vs. a strict CSP:** (a) let Clerk break in RapidRoom (recommended for now; it's upstream's, off by default and looks unfinished), (b) allow Clerk's hosts, or (c) move sign-in to its own window.
2. **Where the terminal lives:** A, B or C from §4 (recommended: C first).
3. **Token in the environment:** `RAPIDROOM_MCP_ENDPOINT` (file path, recommended) or `RAPIDROOM_MCP_TOKEN`.
4. **Start buttons type the command** into the shell (recommended), or start the agent as the tab's process directly?
5. Is the stopgap's terminal order right, and should it get a setting? A setting would need a fixed list rather than a free command line (§1, Security).
