# MCP server

RapidRoom can be built with an [MCP](https://modelcontextprotocol.io) server, so AI agents can drive the editor: load an image, read and change its adjustments, look at a preview and the histogram, and export. It is **off by default**. A normal build has no MCP code, no extra dependencies and no listening socket.

This note compares the two forks that have an MCP server, explains which one RapidRoom took and why, and says how to build and connect. Issue #5.

## Building and connecting

Build or run with the `mcp` cargo feature:

```sh
npm run start:mcp                        # tauri dev -- --features mcp
npm run tauri build -- --features mcp    # release build
```

While the app is running, the server listens on `http://127.0.0.1:7790/mcp`. If that port is taken it tries the next ten ports. Set `RAPIDRAW_MCP_PORT` to pin a port; then it fails instead of moving. The URL in use is written to `mcp-endpoint.json` in the app's config directory.

```sh
claude mcp add --transport http rapidroom http://127.0.0.1:7790/mcp   # Claude Code
codex mcp add rapidroom --url http://127.0.0.1:7790/mcp               # Codex
```

An agent can start from the library: `open_image` opens a photo in the editor. The editing tools work on the image open in the editor. Every change goes through the editor, so you see it happen, each MCP edit is one step you can undo with Ctrl+Z, and autosave writes it like a click. An edit that changes nothing returns at once. If the preview takes longer than 40 seconds, the edit is still applied and the result says `renderPending: true`. If the editor's current state doesn't fit the MCP schema (for example an out-of-range value from an imported sidecar), the state is still returned, with the problem in `validationError`.

| Tool                                          | What it does                                                                                                         |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `list_images`                                 | List supported images in a folder (paged, optionally recursive)                                                      |
| `open_image` (alias `select_image`)           | Open an image in the editor, also when none is open yet                                                              |
| `get_image_state`, `get_active_image_state`   | Current adjustments and an `editRevision`                                                                            |
| `set_adjustments`, `update_adjustments`       | Replace or merge adjustments; validated against a strict schema. Pass `expectedRevision` to avoid overwriting edits. |
| `reset_adjustments`, `apply_auto_adjustments` | Reset everything (one undoable step) or apply auto adjustments                                                       |
| `get_preview`                                 | JPEG render of the current or a proposed edit (128–4096 px)                                                          |
| `get_histogram_data`                          | The editor's 256-bin RGB and luma histogram                                                                          |
| `calculate_guided_perspective`                | Perspective transform and crop for guide lines, without changing the edit                                            |
| `export_images`                               | Export with the export panel's settings and wait until it finishes; no image needs to be open                        |

## Real-client regression

Tool discovery includes `ttlMs: 0` and `cacheScope: "private"`, required by MCP `2026-07-28`. Tool-call results include `resultType: "complete"`; the SDK removes it for older negotiated versions. The SDK remains pinned at rmcp 3.1.2. Zero TTL disables discovery caching; older supported protocol versions retain the same tools.

The optional native Linux regression runs the installed Claude Code and Codex clients against an isolated CC0 photo. It requires a recorded MCP editing call, a new revision, the exact Exposure value in the native slider, and a changed preview for each client. It also validates discovery at `2025-06-18` and `2026-07-28`. See [`validation/native-ui/README.md`](validation/native-ui/README.md). These opt-in checks use the clients’ existing login and make real model requests; they do not change client configuration or touch the user’s library.

## Security

- **It listens on a loopback TCP port** (`127.0.0.1` only). Nothing outside the machine can reach it, and it makes no outgoing connections.
- **There is no authentication.** Any program running on the same machine can connect while the app is running, including programs run by other users. Only build with `mcp` on a machine you trust. A token and a stdio proxy are tracked in #116.
- **Browsers can't drive it.** Requests carrying an `Origin` header are refused. rmcp also rejects any `Host` other than `localhost`, `127.0.0.1` or `::1`, which blocks DNS rebinding, and it only accepts `application/json`, so a web page can't send a "simple" cross-site request either.
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

- **stdio transport.** For example, a small `--mcp-stdio` proxy that forwards to the running app, so MCP clients can use stdio and no TCP port is needed.
- **Authentication:** a token in `mcp-endpoint.json` (readable only by the user) that clients send as a header.
- **Working on copies:** an option to edit copies in a workspace instead of the library's originals.
- **Comparison reviews** (before/after, A/B crops) for the agent.

## What RapidRoom changed from cgasgarth's version

The harvest is cgasgarth's latest MCP code (`cgasgarth/RapidRaw@a5a01cc`, after `4a5c14e`, `c0f079d`, `b2b9abf`, `9a70a3c`, `0a5e71f` and `c8d6ec6`), with these changes:

- **Feature flag.** Everything is behind the cargo feature `mcp`: the module, the rmcp and HTTP dependencies, the Tauri commands, the state and the server start. The React bridge asks the backend for `mcp_status` once at startup and stays idle if the command doesn't exist.
- **No tone-mapper override.** cgasgarth's version made the GUI preview and exports use the editor's tone mapper even when the user's "override tone mapper" setting is on. That changes what the editor renders, so it was left out. MCP previews follow the same setting as the GUI.
- **Render wait.** cgasgarth's version stored a JSON copy of the adjustments after every preview render. RapidRoom stores a reference instead and only serialises in the bridge, so normal editing does no extra work.
- **`Origin` check.** Requests with an `Origin` header are refused (see Security).
- **Curve helper.** The parametric-curve function moved from `Curves.tsx` to `utils/adjustments.ts` unchanged, so curves render exactly as before. cgasgarth had rewritten it.
- **Export errors.** `export_images` reports each failed file, using RapidRoom's existing error list.
- cgasgarth's README section is replaced by this note.
