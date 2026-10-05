# RapidRoom plugin for Claude Code (and a skill for Codex)

Lets an assistant edit the photo open in RapidRoom while you watch: grade it step by step and check each step, compare looks, export for Instagram, teach as it goes, explain a slider by reading RapidRoom's source for your version, and draft issues for RapidRoom that are posted only after you say yes. Part of the AI-assisted editing epic (#113); this is issue #118.

It needs a RapidRoom build with the MCP server (the `mcp` cargo feature, issue #5). Without it the plugin has nothing to connect to.

## Install

```sh
claude plugin marketplace add RapidRoom/rapidroom
claude plugin install rapidroom@rapidroom
```

Or for one session from a checkout: `claude --plugin-dir rapidroom/plugin`. Codex, the launcher script and an Omarchy keybinding: [skills/rapidroom/references/starting.md](skills/rapidroom/references/starting.md).

## What's in it

| Path                                         | What                                                                                                 |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `skills/rapidroom/SKILL.md`                  | The skill: principles, how to start, gotchas, and which reference to read when                       |
| `skills/rapidroom/references/`               | Workflow, tools (now and coming), adjustment reference, direction, looks, tutor mode, source, issues |
| `skills/rapidroom/references/adjustments.md` | **Generated** from the code. Don't edit it by hand (see below)                                       |
| `commands/`                                  | `/rapidroom:edit`, `:tutor`, `:looks`, `:instagram`, `:explain`, `:report`                           |
| `.mcp.json`                                  | Connects to `http://127.0.0.1:7790/mcp` (or `RAPIDRAW_MCP_PORT`)                                     |
| `codex/AGENTS.md`                            | For Codex without skill support                                                                      |
| `scripts/rapidroom-assistant`                | Starts Claude Code or Codex in the folder RapidRoom has open                                         |

## The adjustment reference

`references/adjustments.md` is built by `src/utils/adjustmentReference.ts` from `INITIAL_ADJUSTMENTS`, the editor's sliders, `createSubMask` and the mask panel, plus the hand-written notes in `src/utils/adjustmentReferenceNotes.ts`. The Vitest test `src/utils/adjustmentReference.test.ts` fails when the committed file differs, when a slider can't be mapped to a key, or when a key has no note. After changing adjustments or sliders:

```sh
npx vitest run src/utils/adjustmentReference.test.ts -u
```

Then check the diff, and update the note if the meaning changed. The notes were checked against the shader and `image_processing.rs`.

When a planned MCP tool lands, move it from "Coming" to "Now" in `references/tools.md` and update the workarounds that mention it.

## Licence

AGPL-3.0, like the rest of RapidRoom, except `references/direction.md`, `references/taste.md` and `references/film-looks.md`. Those are adapted from the `photo-edit-master` and `photo-style-builder` skills in [Lightweft](https://github.com/sheldonxxxx/Lightweft/tree/d332fbd629faf7e47144ba173d7f2032be62199d) by sheldonxxxx and the Lightweft contributors, and stay under the MIT licence ([LICENSES/Lightweft-MIT.txt](LICENSES/Lightweft-MIT.txt)). Don't copy AGPL text into those three files.
