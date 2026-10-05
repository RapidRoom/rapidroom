"""Actual-client history and read-only preview/resource regression for issue #115."""

import base64
import io
import json

from PIL import Image

from mcp_clients import client_command, execute_client, image_state, objects, rpc, run_client_checks


def read_tool(url, token, name, arguments):
    value = rpc(url, "tools/call", {"name": name, "arguments": arguments}, token=token)
    if value.get("isError"):
        raise RuntimeError(name + " failed: " + str(value))
    return value.get("structuredContent") or json.loads(next(item["text"] for item in value["content"] if item["type"] == "text"))


def run_history_checks(case, smoke):
    from smoke import wait_for

    smoke.step("authenticated endpoint and MCP discovery", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    image = str(case / "input/smoke.ARW")
    folder = case / "history-client-tests"
    folder.mkdir(mode=0o700)
    prompt = (f"Run the following MCP regression using only rapidroom tools on imagePath {json.dumps(image)}. "
        "Await each result before making the next call; do not parallelize. First get_image_state. "
        "Make exactly three separate update_adjustments calls that set only exposure to 0.2, then 0.4, then 0.6. "
        "Pass the latest editRevision as expectedRevision each time. Call history_list. "
        "Undo exactly twice, one undo per call, with the latest editRevision. Call history_list again. "
        "Get three previews with maxDimension 256: one original=true, one sideBySide=true, and one "
        "region={x:0.25,y:0.25,width:0.5,height:0.5}. Then get_editor_context and get_image_state. "
        "Make no other changes; use no shell/files/unrelated tools. Finish with a short confirmation.")
    tools = ["get_image_state", "update_adjustments", "history_list", "undo", "get_preview", "get_editor_context"]
    initial = image_state(url, image, token=token)
    command, version = client_command(case, folder, "claude", 0.2, prompt_override=prompt, tools=tools)
    events = execute_client(command, folder, "claude", case / "config")
    calls = list({value["id"]: value for value in objects(events) if value.get("type") == "tool_use" and value.get("name", "").startswith("mcp__rapidroom__")}.values())
    names = [value["name"].removeprefix("mcp__rapidroom__") for value in calls]
    if names.count("update_adjustments") != 3 or names.count("undo") != 2 or names.count("history_list") < 2 or names.count("get_preview") < 3:
        raise RuntimeError("Actual Claude history/preview calls missing: " + str(names))
    state = image_state(url, image, token=token)
    history = read_tool(url, token, "history_list", {"imagePath": image})
    if abs(state["adjustments"]["exposure"] - 0.2) > 1e-6 or smoke.slider("Exposure")["value"] != 0.2:
        raise RuntimeError("Undo twice did not restore the first real AI edit in the UI")
    expected = dict(initial["adjustments"], exposure=0.2)
    if state["adjustments"] != expected:
        raise RuntimeError("Actual client changed a field outside Exposure")
    ai_entries = [entry for entry in history["entries"] if entry["actor"] == "assistant"]
    if len(ai_entries) != 3 or sum(entry["undone"] for entry in ai_entries) != 2 or not all("Exposure" in entry["label"] and entry["changedKeys"] == ["exposure"] for entry in ai_entries):
        raise RuntimeError("AI-labelled shared history does not match the client edits")
    smoke.execute("""document.querySelector('[data-bench-id="undo"]').dispatchEvent(new MouseEvent('contextmenu',
      {bubbles:true,cancelable:true}));return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button[data-active]')].filter(e=>e.innerText.includes('AI: Exposure')).length;") == 3,
             "real GUI history labels")
    smoke.capture("mcp-ai-labelled-history")
    smoke.step("actual Claude three edits, labelled history and undo twice", {"version": version, "calls": names,
        "exposure": 0.2, "ai_entries": ai_entries, "native_history_labels": True})
    # Close the real history menu before captures for read-only calls.
    smoke.execute("""document.querySelector('[data-bench-id="undo"]').dispatchEvent(new MouseEvent('contextmenu',
      {bubbles:true,cancelable:true}));return true;""")

    resources = rpc(url, "resources/list", {}, token=token)
    if resources.get("ttlMs") != 0 or resources.get("cacheScope") != "private":
        raise RuntimeError("Resource discovery lacks modern protocol cache hints")
    content = rpc(url, "resources/read", {"uri": "rapidroom://schema/adjustments"}, token=token)
    if content.get("resultType") != "complete" or content.get("ttlMs") != 0 or content.get("cacheScope") != "private":
        raise RuntimeError("Resource read lacks required modern protocol fields")
    schema = json.loads(content["contents"][0]["text"])
    if schema["parameters"]["exposure"]["default"] != 0 or not schema["parameters"]["exposure"]["uiRanges"]:
        raise RuntimeError("Generated adjustment schema was not served")
    previews = []
    for options in ({"original": True}, {"sideBySide": True}, {"region": {"x":0.25,"y":0.25,"width":0.5,"height":0.5}}):
        value = rpc(url, "tools/call", {"name": "get_preview", "arguments": {"imagePath":image,"maxDimension":256,**options}}, token=token)
        if value.get("isError"):
            raise RuntimeError("Preview option failed")
        payload = base64.b64decode(next(item["data"] for item in value["content"] if item["type"] == "image"))
        with Image.open(io.BytesIO(payload)) as rendered:
            rendered.load()
            if max(rendered.size) > 256:
                raise RuntimeError("Preview long-edge bound exceeded")
            previews.append({"options": options, "dimensions": list(rendered.size), "jpeg_bytes": len(payload)})
    if image_state(url, image, token=token) != state or read_tool(url, token, "history_list", {"imagePath":image}) != history:
        raise RuntimeError("Resource/preview reads changed state, revision or history")
    smoke.step("generated schema and bounded read-only original/comparison/region previews", {"keys": len(schema["parameters"]),
        "controls": len(schema["controls"]), "previews": previews, "state_revision_history_unchanged": True})

    codex_folder = folder / "codex"
    codex_folder.mkdir(mode=0o700)
    codex_prompt = f"Use only rapidroom MCP tools: get_editor_context, then history_list with imagePath {json.dumps(image)}, then get_preview with that path and maxDimension 256 and sideBySide=true. Do not edit or use any other tool. Finish."
    command, version = client_command(case, codex_folder, "codex", 0.2, prompt_override=codex_prompt,
                                      tools=["get_editor_context", "history_list", "get_preview"])
    events = execute_client(command, codex_folder, "codex", case / "config")
    called = {value.get("tool") for value in objects(events) if value.get("type") == "mcp_tool_call" and value.get("status") == "completed"}
    if not {"get_editor_context", "history_list", "get_preview"}.issubset(called):
        raise RuntimeError("Actual Codex new-tool reads missing")
    if image_state(url, image, token=token) != state or read_tool(url, token, "history_list", {"imagePath":image}) != history:
        raise RuntimeError("Codex read-only calls changed the editor")
    smoke.step("actual Codex context/history/comparison reads preserve state", {"version":version,"called":sorted(called),
        "state_revision_history_unchanged":True})
