# Starting an assistant with RapidRoom

Until RapidRoom has a built-in terminal ([coming](tools.md#coming), #119), the assistant runs in a terminal next to the app. Use this page to help the user set it up, or when the MCP tools are missing.

## 1. RapidRoom with the MCP server

The MCP server is only in builds made with the `mcp` cargo feature, from a source checkout:

```sh
npm run start:mcp                        # development: tauri dev -- --features mcp
npm run tauri build -- --features mcp    # a release build with MCP
```

Release packages don't include it yet. While the app runs, it listens on `http://127.0.0.1:7790/mcp` (see [tools.md](tools.md)). If the tools don't show up in the assistant: is RapidRoom running, is it an `mcp` build, and is the port 7790 (otherwise set `RAPIDRAW_MCP_PORT` for both)?

## 2. The assistant

**Claude Code**, with this plugin (skill, slash commands and the MCP connection):

```sh
claude plugin marketplace add RapidRoom/rapidroom
claude plugin install rapidroom@rapidroom
```

Or, from a RapidRoom checkout, for one session: `claude --plugin-dir rapidroom/plugin`. The commands are `/rapidroom:edit`, `/rapidroom:tutor`, `/rapidroom:looks`, `/rapidroom:instagram`, `/rapidroom:explain` and `/rapidroom:report`.

**Codex**: connect the server and give Codex the skill.

```sh
codex mcp add rapidroom --url http://127.0.0.1:7790/mcp
mkdir -p ~/.codex/skills && ln -s "$PWD/rapidroom/plugin/skills/rapidroom" ~/.codex/skills/rapidroom
```

If your Codex doesn't load skills, copy `rapidroom/plugin/codex/AGENTS.md` into the photo folder (or `~/.codex/AGENTS.md`) and fix the path in it.

## 3. Open the assistant where your photos are

`rapidroom/plugin/scripts/rapidroom-assistant` starts Claude Code (or Codex) in the folder RapidRoom has open in its library. It reads RapidRoom's settings file and changes nothing. Put it on the `PATH`:

```sh
ln -s "$PWD/rapidroom/plugin/scripts/rapidroom-assistant" ~/.local/bin/
rapidroom-assistant            # Claude Code in RapidRoom's current folder
rapidroom-assistant codex      # Codex
rapidroom-assistant claude ~/Pictures/2026/iceland
```

**Omarchy** (Hyprland): add a key to `~/.config/hypr/bindings.conf`, for example Super+Shift+R. Omarchy's own bindings use `$terminal`; any terminal that takes `-e` works the same way.

```ini
bindd = SUPER SHIFT, R, RapidRoom assistant, exec, $terminal -e rapidroom-assistant
bindd = SUPER SHIFT ALT, R, RapidRoom assistant (Codex), exec, $terminal -e rapidroom-assistant codex
```

**Other desktops**: a launcher entry in `~/.local/share/applications/rapidroom-assistant.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=RapidRoom assistant
Comment=Claude Code in the folder RapidRoom has open
Exec=rapidroom-assistant
Terminal=true
Categories=Graphics;Photography;
Actions=codex;

[Desktop Action codex]
Name=Codex
Exec=rapidroom-assistant codex
```

Put the terminal beside RapidRoom so both are visible: the user watches the photo while talking to the assistant.
