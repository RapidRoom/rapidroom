"""Real MCP clients reached through the built-in and external launch buttons."""

import json
import os
from pathlib import Path
import sys

from PIL import ImageChops

from mcp_clients import client_command, execute_client, image_state, objects, run_client_checks


def run_launched_client(case, name):
    request = json.loads((case / "tools" / (name + "-request.json")).read_text())
    folder = Path(request["folder"])
    result = {"folder": os.getcwd(), "app_folder": os.environ.get("RAPIDROOM_FOLDER"),
              "config_home": os.environ.get("XDG_CONFIG_HOME"),
              "endpoint_or_token_env": any(key in os.environ for key in (
                  "RAPIDROOM_MCP_ENDPOINT", "RAPIDROOM_MCP_URL", "RAPIDROOM_MCP_TOKEN")),
              "stdin_is_pty": os.isatty(0)}
    try:
        if result["folder"] != str(case / "input") or result["app_folder"] != result["folder"]:
            raise RuntimeError("Launched client folder differs from the photo folder")
        if result["config_home"] != str(case / "config") or result["endpoint_or_token_env"]:
            raise RuntimeError("Launched client endpoint discovery environment differs")
        command, version = client_command(case, folder, name, request["exposure"])
        events = execute_client(command, folder, name, case / "config", cwd=case / "input")
        if name == "claude":
            called = any(value.get("type") == "tool_use" and
                         value.get("name") == "mcp__rapidroom__update_adjustments"
                         for value in objects(events))
        else:
            called = any(value.get("type") == "mcp_tool_call" and value.get("server") == "rapidroom"
                         and value.get("tool") == "update_adjustments" and value.get("status") == "completed"
                         for value in objects(events))
        if not called:
            raise RuntimeError("Launched client did not record a real editing call")
        result.update({"passed": True, "version": version, "actual_mcp_edit_call": True})
    except Exception as error:
        result.update({"passed": False, "error": str(error)})
    (folder / "launch-result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("RR_REAL_CLIENT_" + ("PASS" if result["passed"] else "FAIL"), flush=True)
    return 0 if result["passed"] else 1


def run_launch_checks(case, smoke):
    from smoke import wait_for

    smoke.step("authenticated endpoint and both MCP wire versions", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    image = str(case / "input/smoke.ARW")
    results = []
    for mode, clients in (("built-in", (("claude", 0.5), ("codex", 1.0))),
                          ("external", (("claude", 1.5), ("codex", 2.0)))):
        if mode == "external":
            smoke.execute("""document.querySelector('[aria-label="Collapse bottom panel"]').click();return true;""")
            wait_for(lambda: smoke.execute("return !document.querySelector('[data-terminal-panel]');"),
                     "built-in panel closed before external launch")
            smoke.execute("""const e=document.querySelector('[data-agent-launcher] select');
              e.value='external';e.dispatchEvent(new Event('change',{bubbles:true}));return true;""")
            wait_for(lambda: smoke.execute("return document.querySelector('[data-agent-launcher] select').value;") == "external",
                     "external launch setting")
        for name, exposure in clients:
            label = mode + "-" + name
            folder = case / "terminal-client-tests" / label
            folder.mkdir(parents=True, mode=0o700)
            (case / "tools" / (name + "-request.json")).write_text(
                json.dumps({"folder": str(folder), "exposure": exposure}) + "\n")
            before = image_state(url, image, token=token)
            before_pixels = smoke.crop(smoke.stable_preview(label + "-before"))
            smoke.execute("""document.querySelector('[data-start-agent="'+arguments[0]+'"]').click();return true;""", [name])
            path = folder / "launch-result.json"
            wait_for(path.exists, "real launched " + label, 330)
            result = json.loads(path.read_text())
            if not result.get("passed"):
                raise RuntimeError(label + " failed: " + str(result))
            if mode == "built-in" and not result["stdin_is_pty"]:
                raise RuntimeError("Built-in real client did not inherit a PTY")
            if mode == "external" and smoke.execute("return !!document.querySelector('[data-terminal-panel]');"):
                raise RuntimeError("External real-client launch reopened the built-in panel")
            after = image_state(url, image, token=token)
            after_pixels = smoke.crop(smoke.stable_preview(label + "-after"))
            if after["adjustments"]["exposure"] != exposure or before["editRevision"] == after["editRevision"]:
                raise RuntimeError(label + " did not make the requested edit")
            if smoke.slider("Exposure")["value"] != exposure or ImageChops.difference(before_pixels, after_pixels).getbbox() is None:
                raise RuntimeError(label + " did not update native slider and preview")
            result.update({"mode": mode, "client": name, "exposure": exposure,
                           "revision_slider_preview_changed": True,
                           "external_panel_stayed_closed": mode == "external"})
            results.append(result)
            smoke.step("real " + label + " authenticated stdio edit", result)
    return results


if __name__ == "__main__":
    sys.exit(run_launched_client(Path(sys.argv[1]), sys.argv[2]))
