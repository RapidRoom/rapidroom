"""Fresh Claude + shipped plugin acceptance on the owned CC0 native fixture."""

import hashlib
import json
import os
from pathlib import Path
import subprocess

from PIL import Image

from mcp_clients import execute_client, image_state, objects, run_client_checks
from mcp_history import read_tool


def run_skill_checks(case, smoke):
    smoke.step("authenticated plugin acceptance endpoint", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    source = Path(json.loads((case / "mcp-skill-test.json").read_text())["source_checkout"])
    context = read_tool(url, token, "get_editor_context", {})
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=source, text=True).strip()
    if context.get("sourceCommit") != head or context.get("sourceDirty") is not False or dirty:
        raise RuntimeError("Skill source lookup needs the pinned app's matching clean checkout")
    root = Path(__file__).resolve().parents[3]
    plugin = root / "rapidroom/plugin"
    folder = case / "skill-client-test"
    folder.mkdir(mode=0o700)
    path = str(case / "input/smoke.ARW")
    initial = image_state(url, path, token=token)
    prompt = (
        "Use the RapidRoom plugin's skill to edit the open CC0 sample. Destination: a 4:5 feed JPEG, "
        "natural with gentle warmth. I approve this original-pixel crop: x500, y100, width1800, height2250. "
        "Crop first. Show three named tone/colour looks using temporary comparisons; do not create virtual "
        "copies if those tools are missing. For this acceptance run I choose the warm look, so apply it, "
        "inspect the result and one small native detail region, and export width1080 with GPS stripped "
        "to originalFolder subfolder plugin-acceptance. Explain how exposure is scaled before the shader "
        "by reading the running version's source. The clean checkout matching the app sourceCommit is "
        f"{json.dumps(str(source))}; verify that identity against get_editor_context. "
        "Use only the plugin instructions, read-only file tools and RapidRoom MCP. Do not use shell, "
        "network tools, write files directly or post anything. Missing virtual-copy tools should be "
        "explained as coming. Finish with the export location and source evidence."
    )
    binary = os.environ["RAPIDROOM_TEST_CLAUDE_BIN"]
    command = [binary, "-p", "--plugin-dir", str(plugin), "--strict-mcp-config",
               "--mcp-config", str(plugin / ".mcp.json"), "--tools", "Read,Skill",
               "--allowedTools", "Read,Skill,mcp__rapidroom__*", "--setting-sources", "",
               "--settings", '{"disableAllHooks":true}', "--no-session-persistence",
               "--max-turns", "40", "--output-format", "stream-json", "--verbose",
               "--add-dir", str(plugin), "--add-dir", str(source), prompt]
    version = subprocess.check_output([binary, "--version"], text=True).strip()
    events = execute_client(command, folder, "claude-skill", case / "config", cwd=folder,
                            env_overrides={"PATH": str(case / "engine") + os.pathsep + os.environ["PATH"]})
    values = list(objects(events))
    calls = [v for v in values if v.get("type") == "tool_use"]
    names = {v.get("name", "").removeprefix("mcp__rapidroom__") for v in calls}
    required = {"get_editor_context", "update_adjustments", "get_preview", "render_compare", "render_region", "export_images"}
    if not required <= names:
        raise RuntimeError("Fresh plugin session missed workflow calls: " + str(sorted(required - names)))
    if any(v.get("is_error") for v in values if v.get("type") == "tool_result"):
        raise RuntimeError("Fresh plugin session returned a failed tool call")
    if any(v.get("name") not in {"Read", "Skill"} and not v.get("name", "").startswith("mcp__rapidroom__") for v in calls):
        raise RuntimeError("Plugin session used tools outside the acceptance scope")
    skill_used = any(v.get("name") == "Skill" or (v.get("name") == "Read" and "SKILL.md" in v.get("input", {}).get("file_path", "")) for v in calls)
    source_reads = [v.get("input", {}).get("file_path", "") for v in calls if v.get("name") == "Read"]
    if not skill_used or not any(str(source / "src-tauri/src/image_processing.rs") == path for path in source_reads):
        raise RuntimeError("Plugin instructions or verified exposure source were not read")
    for call in calls:
        if call.get("name") == "mcp__rapidroom__update_adjustments" and not call.get("input", {}).get("expectedRevision"):
            raise RuntimeError("Plugin mutation lacked an expected revision")
    comparisons = [v for v in calls if v.get("name") == "mcp__rapidroom__render_compare"]
    if not any(len(v.get("input", {}).get("variants", [])) == 3 for v in comparisons):
        raise RuntimeError("Plugin did not compare three temporary looks")
    exports = [v for v in calls if v.get("name") == "mcp__rapidroom__export_images"]
    if not exports or any(v.get("input", {}).get("exportSettings", {}).get("stripGps") is not True for v in exports):
        raise RuntimeError("Plugin export did not explicitly strip GPS")
    final = image_state(url, path, token=token)
    crop = final["adjustments"].get("crop", {})
    if any(crop.get(k) != v for k, v in {"x":500,"y":100,"width":1800,"height":2250}.items()) or final["editRevision"] == initial["editRevision"]:
        raise RuntimeError("Plugin did not apply the approved feed crop/edit")
    files = list((case / "input/plugin-acceptance").glob("*.jpg"))
    if len(files) != 1:
        raise RuntimeError("Plugin export missing or ambiguous")
    with Image.open(files[0]) as image:
        if image.size != (1080, 1350):
            raise RuntimeError("Plugin export dimensions differ from the requested feed output")
        dimensions = list(image.size)
    smoke.step("fresh plugin session grades, compares, exports and reads verified source", {
        "client_version": version, "called": sorted(names), "source_commit": head,
        "source_checkout_clean_and_matches_app": True, "plugin_skill_used": skill_used,
        "export_dimensions": dimensions, "export_sha256": hashlib.sha256(files[0].read_bytes()).hexdigest(),
        "mutations_revision_guarded": True, "virtual_copy_tools_deferred": True,
        "source_explanation_read": "src-tauri/src/image_processing.rs", "real_model_requests": True})
