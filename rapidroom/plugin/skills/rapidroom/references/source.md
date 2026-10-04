# Understanding RapidRoom from its source

When these docs don't answer a question ("what does Centré actually do?", "why does Dehaze shift the colour?"), read RapidRoom's source **for the version the user is running**, and explain what you found in plain words with file and line references.

## 1. Find the version

- The start screen shows it: `RapidRoom 2.2.0 - community build of RapidRAW 1.6.4`. Ask the user if you can't see it.
- Coming: `get_editor_context` (#115) will report it. Until then, don't use the MCP server's own version number: it isn't RapidRoom's.
- If the user runs RapidRoom from a source checkout (for example `npm run start:mcp`), that checkout is the source. Use it directly, with `git log -1` for the commit.

## 2. Get the source once

Release builds may be matched to a published `v<version>` tag. A version string alone does not identify a development build: several commits can share it. Prefer the installed build’s known commit or source checkout, and explicitly state uncertainty when neither is known.

Releases are tagged `v<version>` (for example `v2.2.0`). Keep one copy per version in a cache:

```sh
cache="${XDG_CACHE_HOME:-$HOME/.cache}/rapidroom-source/v2.2.0"
[ -d "$cache" ] || git clone --depth 1 --branch v2.2.0 https://github.com/RapidRoom/rapidroom.git "$cache"
```

- Say what you're about to download before the first clone (a shallow clone of a public repository, a few hundred MB with the lens database).
- If the tag doesn't exist (no release yet, or a dev build), use `main` and **say that it may differ** from what the user runs. Don't present `main` as their version.
- Without git or network, browse it on GitHub instead: `https://github.com/RapidRoom/rapidroom/tree/v2.2.0`.
- Never modify the cache. It's for reading.

## 3. Where things are

| Question                                       | Look in                                                                                                                                                                                                 |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| What a slider does to the pixels               | `src-tauri/src/shaders/shader.wgsl`: `main` at the end calls the steps in order; for example `apply_white_balance`, `apply_tonal_adjustments`, `apply_hsl_panel`, `apply_color_grading`, `apply_dehaze` |
| How a slider value is scaled before the shader | `src-tauri/src/image_processing.rs`: `SCALES` and `get_global_adjustments_from_json`                                                                                                                    |
| Slider ranges, labels, which panel             | `src/components/adjustments/*.tsx`, `src/components/panel/right/CropPanel.tsx`; keys and defaults in `src/utils/adjustments.ts`                                                                         |
| Masks                                          | `src/components/panel/right/MasksPanel.tsx`, `src/utils/maskUtils.ts`, `src-tauri/src/mask_generation.rs`                                                                                               |
| Lens correction, perspective, lens blur, flare | `src-tauri/src/lens_correction.rs`, `guided_perspective.rs`, `lens_blur.rs`, `shaders/flare.wgsl`                                                                                                       |
| LUTs and film looks                            | `src-tauri/src/lut_processing.rs`, `src-tauri/resources/film_luts/`                                                                                                                                     |
| Export                                         | `src-tauri/src/export_processing.rs`                                                                                                                                                                    |
| The MCP tools themselves                       | `src-tauri/src/mcp/` (`tools.rs` for the tool list) and `src/hooks/useMcpBridge.ts`                                                                                                                     |
| What RapidRoom changed compared with RapidRAW  | `CHANGES.md` and `rapidroom/changes.json`                                                                                                                                                               |

Search by the adjustment key (`grep -rn "dehaze" src-tauri/src`), then follow it from the JSON into the shader.

## 4. Explain it

- Answer the user's question first, in photo terms: "Dehaze estimates the haze colour from a blurred copy of the image and subtracts it, which is why it can shift colour in a foggy scene."
- Then the evidence, linked at the version: `shader.wgsl:1125` → `https://github.com/RapidRoom/rapidroom/blob/v2.2.0/src-tauri/src/shaders/shader.wgsl#L1125`.
- Say what you're sure of and what you're inferring. Shader code is exact; your description of how it looks is a reading of it.
- If the code contradicts these docs, the code wins. Tell the user, and offer to report the docs problem ([report-issue.md](report-issue.md)).
