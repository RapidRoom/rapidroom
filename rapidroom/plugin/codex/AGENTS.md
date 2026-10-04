<!--
RapidRoom assistant instructions for Codex (and other agents that read AGENTS.md).
Copy this file into your photo folder or to ~/.codex/AGENTS.md, and set SKILL_DIR below to
where the RapidRoom skill is on your machine. If your Codex loads skills from ~/.codex/skills,
link the skill there instead (see the skill's references/starting.md) and you don't need this file.
SKILL_DIR=~/src/rapidroom/rapidroom/plugin/skills/rapidroom
-->

# Editing photos with RapidRoom

RapidRoom (the community build of RapidRAW) is open next to this terminal, with its MCP server (`rapidroom`). When the user talks about the photo in RapidRoom, its sliders, looks, presets or exports, or about RapidRoom itself, read `SKILL_DIR/SKILL.md` first and the references it points to, then follow them. `SKILL_DIR` is set in the comment at the top of this file.

The rules that matter most, in case you read nothing else:

- Edit only through RapidRoom's MCP tools, so the user sees each change and can undo it. Never write sidecar or settings files.
- Use only the adjustment keys in `SKILL_DIR/references/adjustments.md`, within its slider ranges. Never guess a key.
- Use only tools that the server lists. Planned tools are marked "coming" in `SKILL_DIR/references/tools.md`; don't describe them as if they existed.
- Check after every step: render a preview, measure, look, adjust.
- Propose big moves (a crop, a strong look, a reset) before making them.
- Nothing is posted anywhere without the user's explicit yes. Issues for RapidRoom follow `SKILL_DIR/references/report-issue.md`.
- No photos, file names or paths leave the machine unless the user agrees.
