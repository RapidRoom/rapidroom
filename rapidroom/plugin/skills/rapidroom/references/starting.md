> Official Linux packages include MCP and the stdio adapter. Enable **Settings → General → Let AI assistants control RapidRoom** before connecting; Start Claude/Codex offers to enable it. Control starts off, listens locally only, and uses a fresh key each time it starts. Disabling removes the endpoint and stops the listener. Reconnect the assistant after re-enabling. AppImage start buttons use its bundled adapter; outside plugins need the adapter on PATH or an absolute path.

# Starting an assistant with RapidRoom

In builds with the `terminal` feature, the movable terminal panel can start Claude Code or Codex in the open photo folder. External-terminal mode uses the same folder and client registration. Use this page to help the user set it up, or when the MCP tools are missing.

## 1. RapidRoom with the MCP server

The MCP server is only in builds made with the `mcp` cargo feature, from a source checkout that includes the implementation in PR #102. Check that the `mcp` feature and `start:mcp` script exist before using these commands; a checkout without them cannot enable the server:

```sh
npm run start:mcp                        # development: tauri dev -- --features mcp
npm run tauri build -- --features mcp    # a release build with MCP
```

Release packages don't include it yet. Build the separate `rapidroom-mcp-stdio` adapter and put it on `PATH` (or configure its absolute path):

```sh
cargo build --manifest-path rapidroom/mcp-client/Cargo.toml --release --locked
```

The adapter reads the private per-user endpoint file and authenticates to the running editor. It discovers the actual port automatically; no endpoint or bearer token belongs in the assistant configuration. See [MCP.md](../../../../MCP.md) for the full configuration and security notes. If tools are missing, check that the app is running with `mcp` enabled and that the adapter is installed. Restart the assistant's MCP connection after restarting the editor.

## 2. The assistant

**Claude Code**, with this plugin (skill, slash commands and the MCP connection):

```sh
claude plugin marketplace add RapidRoom/rapidroom
claude plugin install rapidroom@rapidroom
```

Or, from a RapidRoom checkout, for one session: `claude --plugin-dir rapidroom/plugin`. The commands are `/rapidroom:edit`, `/rapidroom:tutor`, `/rapidroom:looks`, `/rapidroom:instagram`, `/rapidroom:explain` and `/rapidroom:report`.

**Codex**: connect the server and give Codex the skill.

```sh
codex mcp add rapidroom -- rapidroom-mcp-stdio
mkdir -p ~/.agents/skills
ln -s "$PWD/rapidroom/plugin/skills/rapidroom" ~/.agents/skills/rapidroom
```

The [official Codex skills documentation](https://learn.chatgpt.com/docs/build-skills#where-codex-loads-local-skills) documents user skills in `~/.agents/skills` and supports symlinked folders. Older installations may also use `~/.codex/skills`; use the location supported by your installed version. The [official MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) covers server registration; the stdio configuration was checked with real Claude Code 2.1.289 and Codex CLI 0.160.0 editing a CC0 photo in the native editor. Add `env_vars = ["XDG_CONFIG_HOME"]` under `[mcp_servers.rapidroom]` when using a custom XDG config directory, so Codex forwards that standard directory variable to the adapter. No bearer or endpoint environment variable is needed.

If your Codex doesn't load skills, merge the guidance in `rapidroom/plugin/codex/AGENTS.md` into the photo folder's `AGENTS.md` (or `~/.codex/AGENTS.md`) and fix its checkout path. Preserve any existing instructions in that file.

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
