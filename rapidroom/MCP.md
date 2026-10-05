# MCP server

RapidRoom can be built with an [MCP](https://modelcontextprotocol.io) server, so AI agents can drive the editor: load an image, read and change its adjustments, look at a preview and the histogram, and export. Official Linux `.deb` and AppImage packages include the MCP feature and stdio adapter. AI control is **off by default**: there is no listener or endpoint file until enabled. Developer builds without `mcp` still omit its dependencies and server.

This note compares the two forks that have an MCP server, explains which one RapidRoom took and why, and says how to build and connect. Issue #5.

## Building and connecting

In **Settings → General**, enable **Let AI assistants control RapidRoom**. Start Claude/Codex offers the same setting before the first launch when control is off; Cancel leaves it off and starts no client. The explanation is “Control is local only, with a fresh key each time it starts.” Both built-in and external start buttons pass the installed adapter in per-launch client configuration, including its absolute path inside an AppImage. They do not modify global Claude/Codex configuration.

Disabling the setting stops the listener and its accepted connections and removes the private endpoint file. Enabling it again creates a fresh key; restart an existing MCP connection so it discovers the new endpoint. The preference persists for the next GUI start. Clean app exit removes the endpoint. Headless exports never start the server.

Official Linux packaging uses:

```sh
python3 rapidroom/build-linux-packages.py
```

The `.deb` installs `/usr/bin/rapidroom-mcp-stdio` alongside `/usr/bin/rapidroom`. The AppImage contains the same adapter in its `usr/bin`; the app's start buttons resolve that bundled executable directly. For a plugin started outside the AppImage, put the adapter on your PATH or use its absolute path in the client configuration; keep the AppImage mounted while using a path inside its mount. A user-local install should place both executables in `~/.local/bin`.

Developer builds can use the `mcp` cargo feature:

```sh
npm run start:mcp                        # tauri dev -- --features mcp
npm run tauri build -- --features mcp    # release build
```

When control is enabled, its HTTP listener binds to `127.0.0.1:7790` (or the next ten ports). `RAPIDRAW_MCP_PORT` pins a port and fails if it is occupied. Each listener start creates a fresh random 256-bit bearer token. The URL, token and supported protocol hint are atomically written to `mcp-endpoint.json` in the app's per-user config directory, with mode 0600 on Unix. Publishing the endpoint must succeed before the listener accepts requests.

Build the separate adapter alongside the MCP-enabled app:

```sh
cargo build --manifest-path rapidroom/mcp-client/Cargo.toml --release --locked
# Put rapidroom/mcp-client/target/release/rapidroom-mcp-stdio on your PATH,
# or use its absolute path in the client configuration below.
```

The adapter discovers the endpoint under the standard per-user config directory (`$XDG_CONFIG_HOME/io.github.CyberTimon.RapidRAW` or `~/.config/io.github.CyberTimon.RapidRAW` on Linux). It checks ownership and private permissions on Unix, permits only the app's IPv4 loopback URL, and sends the bearer header internally. No token, URL or `RAPIDROOM_MCP_ENDPOINT` environment variable belongs in client configuration. Start the editor first; after restarting it, restart the MCP connection so the proxy reads the new token. The adapter never starts or links the GUI and never adds its own filesystem tools. `--help` is safe; unknown arguments exit with an error.

Claude Code MCP configuration:

```json
{
  "mcpServers": {
    "rapidroom": { "command": "rapidroom-mcp-stdio" }
  }
}
```

Equivalent command:

```sh
claude mcp add --transport stdio rapidroom -- rapidroom-mcp-stdio
```

Codex configuration (`~/.codex/config.toml`):

```toml
[mcp_servers.rapidroom]
command = "rapidroom-mcp-stdio"
env_vars = ["XDG_CONFIG_HOME"]
```

Equivalent command:

```sh
codex mcp add rapidroom -- rapidroom-mcp-stdio
```

Codex filters the environment it passes to stdio children. The `env_vars` entry forwards the standard config-directory variable when set; it contains no credentials and needs no endpoint-specific variable. See the [official MCP environment forwarding documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli). If you register the command with `codex mcp add`, retain that `env_vars` entry for a custom XDG directory.

Both clients use the same tools and running editor as authenticated HTTP clients. The plugin MCP configuration also uses this adapter. The optional thin `rapidroom-agent` command-line interface is not implemented in this change.

## Real-client regression

Tool discovery includes `ttlMs: 0` and `cacheScope: "private"`, required by MCP `2026-07-28`. Tool-call results include `resultType: "complete"`; the SDK removes it for older negotiated versions. The SDK remains pinned at rmcp 3.1.2. Zero TTL disables discovery caching; older supported protocol versions retain the same tools.

The optional native Linux regression runs the installed Claude Code and Codex clients against an isolated CC0 photo. It requires a recorded MCP editing call, a new revision, the exact Exposure value in the native slider, and a changed preview for each client. It also validates discovery at `2025-06-18` and `2026-07-28`. See [`validation/native-ui/README.md`](validation/native-ui/README.md). These opt-in checks use the clients’ existing login and make real model requests; they do not change client configuration or touch the user’s library.

## Shared history, context and generated schema

`history_list`, `undo` and `redo` use the editor's existing undo stack. AI edits have an `AI:` label listing changed fields and numeric deltas, plus a brief toast; history reads leave pending GUI gestures untouched. Undo/redo finish a pending GUI history entry before moving one step. Pass `expectedRevision` to avoid overwriting a newer user edit. `get_editor_context` reads the active photo/virtual copy, dimensions, EXIF summary, crop/masks, panel/mask selection and Card mode without applying an edit.

The MCP resource `rapidroom://schema/adjustments` is generated from `INITIAL_ADJUSTMENTS` and actual slider props. It records defaults, keys, UI ranges and sign conventions, including dynamic slider bindings. Run `npm run mcp:schema` after changing those sources; `npm run mcp:schema:check` and the Vitest drift test refuse a stale artifact. The tool input schema still defines accepted values; a default's type or UI range is not an additional validator.

`get_preview` supports `original`, `side_by_side` and a fractional `region` rectangle. Original means the neutral defaults rendered through the existing tone-mapper path, not the camera JPEG. Comparison places original left/edited right with letterboxing, and bounds the whole image's long edge. Regions crop the rendered preview; native-resolution crops are separate future work. These reads preserve the current edit, revision and history. See [plugin tool reference](plugin/skills/rapidroom/references/tools.md) for limits and combinations.

## Security

- **It listens on a loopback TCP port** (`127.0.0.1` only). Nothing outside the machine can reach it, and it makes no outgoing connections.
- **Per-session authentication.** Missing or incorrect bearer headers receive HTTP 401. The listener retains a SHA-256 token verifier and compares fixed-size hashes in constant time with `subtle`; it does not expose the token through editor state, MCP status or client configuration. A fresh token is generated every listener start; there is no pinned-token setting.
- **Browser requests are refused.** Requests carrying an `Origin` header are refused. rmcp also rejects any `Host` other than `localhost`, `127.0.0.1` or `::1`, which blocks DNS rebinding, and it only accepts `application/json`, so a web page can't send a "simple" cross-site request either.
- **Protocol:** one JSON response per POST to `/mcp` (no event stream; GET gets `405`), `Content-Length` or chunked request bodies, and `Connection: close` on every response. Chunked decoding enforces the body/size-line limits without overflowing the accumulated length, validates each data terminator and discards consumed framing buffers.
- **Reach:** edits only apply to the image open in the editor. `list_images` and `export_images` can read and write any folder the user can.

## The two candidates

|                     | [cgasgarth/RapidRaw](https://github.com/cgasgarth/RapidRaw) (taken)                                                                                            | [sheldonxxxx/RapidRAW](https://github.com/sheldonxxxx/RapidRAW)                                                                                                                                                                                               |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Model               | Drives the running editor. Edits happen in the UI and land in its history and sidecars.                                                                        | Headless engine. Works on copies in a workspace the caller picks, with sessions, undo, recipes and exports. Never touches originals or their sidecars.                                                                                                        |
| Features            | 12 tools: browse, select, read/set/merge/reset/auto adjustments, preview, histogram, guided perspective, export                                                | Far more: sessions, comparison reviews, mask composition and refinement, geometry review, presets, panorama/HDR/focus merge, local AI enhancement, denoise jobs (NIND/BM3D, Nonlocal ONNX), Marigold depth, generative retouch, remote SSH, portable sessions |
| Size                | `src-tauri/src/mcp/`: 6 files, about 2,500 lines of Rust. A 210-line React hook and small hooks in `lib.rs`, `app_state.rs`, `export_processing.rs`.           | `src-tauri/src/mcp_bridge/`: 16 files, about 12,400 lines of Rust. `mcp/`, a Node/TypeScript server: about 28,500 lines with tests and e2e scripts. Plus docs and an agent skill.                                                                             |
| Native changes      | Small: a preview helper with a size limit, an export-and-wait helper, a render counter in the editor store                                                     | About 23,000 lines outside the bridge: `gpu_processing.rs` (+2,500), shaders, export, caches, masks, inpainting, and new modules (`raw_denoise`, `nonlocal_onnx`, `nonlocal_coreml`, `marigold_*`, `ai_runtime`, `linux_nvidia_runtime`, `delivery`)          |
| Dependencies        | [rmcp](https://crates.io/crates/rmcp) 3.1.2 (the official Rust MCP SDK; server plus streamable-HTTP transport), `http`, `bytes`, `http-body`, `http-body-util` | Rust: `moxcms`, `zune-jpegxl`, `flate2`, `tar`, `cc`, ONNX/CoreML runtimes. Node 22+: `@modelcontextprotocol/server` 2.0, `zod`. Optional model downloads and remote providers.                                                                               |
| How it starts       | No flag: always on in every build. Server starts at app startup.                                                                                               | Cargo feature `mcp` adds a `--mcp-bridge` mode. The `rapidraw-mcp` npm package starts that binary as a child process and talks to it over pipes; the MCP client talks to the Node server over stdio.                                                          |
| Transport, security | Loopback HTTP on port 7790, no auth. rmcp checks `Host`.                                                                                                       | stdio only, no socket. Better on this point.                                                                                                                                                                                                                  |
| Upstream sync       | Merged upstream on 2026-09-22 (`cbca858`), close to RapidRoom's base                                                                                           | Merged upstream on 2026-09-20 (`4e1c0cb`), but carries many fork-only engine changes                                                                                                                                                                          |
| Fit with RapidRoom  | Applies with small changes                                                                                                                                     | Can't be taken without most of the fork, including changes to rendered output, new network use (model downloads, remote generative providers) and a second package to publish                                                                                 |

### Recommendation: cgasgarth's

It is small, it uses the official SDK, it fits RapidRoom as it is now, and it can sit behind a feature flag with no effect on the default build. sheldonxxxx's is the more capable design, and its stdio, copy-only model is safer. But it is really a fork of the engine. Harvesting it would mean taking over its rendering changes, which would need RapidRoom's pixel regression check and before/after review, plus its AI runtimes, which would need `network-privacy` review. That's many issues' worth of work, not one.

Ideas from sheldonxxxx's design worth bringing over later, each as its own issue:

- **Thin terminal CLI.** A `rapidroom-agent` wrapper could reuse the same authenticated client for terminal operations.
- **Working on copies:** an option to edit copies in a workspace instead of the library's originals.
- **Comparison reviews** (before/after, A/B crops) for the agent.

## What RapidRoom changed from cgasgarth's version

The harvest is cgasgarth's latest MCP code (`cgasgarth/RapidRaw@a5a01cc`, after `4a5c14e`, `c0f079d`, `b2b9abf`, `9a70a3c`, `0a5e71f` and `c8d6ec6`), with these changes:

- **Feature flag.** Everything is behind the cargo feature `mcp`: the module, the rmcp and HTTP dependencies, the Tauri commands, the state and the server start. The React bridge asks the backend for `mcp_status` once at startup and stays idle if the command doesn't exist.
- **Authenticated stdio adapter.** Fresh per-start bearers, a private per-user endpoint, and a separate GUI-free proxy were added in #116.
- **No tone-mapper override.** cgasgarth's version made the GUI preview and exports use the editor's tone mapper even when the user's "override tone mapper" setting is on. That changes what the editor renders, so it was left out. MCP previews follow the same setting as the GUI.
- **Render wait.** cgasgarth's version stored a JSON copy of the adjustments after every preview render. RapidRoom stores a reference instead and only serialises in the bridge, so normal editing does no extra work.
- **`Origin` check.** Requests with an `Origin` header are refused (see Security).
- **Curve helper.** The parametric-curve function moved from `Curves.tsx` to `utils/adjustments.ts` unchanged, so curves render exactly as before. cgasgarth had rewritten it.
- **Export errors.** `export_images` reports each failed file, using RapidRoom's existing error list.
- cgasgarth's README section is replaced by this note.

### Threat model and implementation provenance

The token blocks unauthenticated local processes, including other users who cannot read the endpoint file, while Origin/Host checks reject browser-origin traffic. The trusted boundary is the operating-system user: processes already running as that user can read the token and drive the editor, and privileged administrators can access it. Authentication does not protect against compromise of that account or guarantee availability against local connection flooding. The proxy accesses only its own endpoint file and forwards the editor's existing tool set; tool access still grants the editor's existing local-image/export capabilities. Unix ownership/0600 behavior is tested on Linux; native macOS and Windows permission and client behavior remain unverified locally.

The adapter and endpoint code are fresh AGPL-3.0 implementations. The stdio-adapter and hashed-bearer ideas came from 1tuz's `rapidraw-mcp`/external-control work; no code was copied, and the adapter was written fresh without relying on an unconfirmed source licence. Irvingouj informed the optional CLI shape, and ssarangi informed the separate-binary pattern. Source ideas are credited in CREDITS.md. AI assistance: implementation and local tests by yojen7 with Codex.
