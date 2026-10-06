"""Real Claude/Codex reads across a GUI edit and strict stale-write refusal."""

import json
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from mcp_clients import client_command, execute_client, image_state, objects, rpc, run_client_checks
from mcp_history import read_tool
from mcp_measure import images


def calls(events, client):
    if client == "claude":
        found = {v["id"]: v for v in objects(events) if v.get("type") == "tool_use" and v.get("name", "").startswith("mcp__rapidroom__")}
        return [(v["name"].removeprefix("mcp__rapidroom__"), v["input"], v["id"]) for v in found.values()]
    found = {v["id"]: v for v in objects(events) if v.get("type") == "mcp_tool_call" and v.get("server") == "rapidroom"}
    return [(v["tool"], v["arguments"], v["id"]) for v in found.values()]


def run_revision_checks(case, smoke):
    from smoke import save, sha, wait_for

    smoke.step("authenticated endpoint and MCP discovery", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    path = str(case / "input/smoke.ARW")
    environment = json.loads((case / "environment.json").read_text())
    build_file = Path(environment["engine_source"]).parent / "build.json"
    build = json.loads(build_file.read_text())
    if build["engine_sha256"] != environment["engine_sha256"] or build["source"]["status"]:
        raise RuntimeError("Revision scenario requires a clean pinned build")

    def state():
        return image_state(url, path, token=token)

    def history():
        return read_tool(url, token, "history_list", {"imagePath": path})

    def sidecars():
        return {f.name: {"sha256": sha(f), "mtime_ns": f.stat().st_mtime_ns}
                for f in (case / "input").glob("*.rrdata")}

    options = {
        "get_preview": {"maxDimension": 128},
        "analyze": {"maxDimension": 128},
        "sample_region": {"maxDimension": 128, "region": {"x": 0.25, "y": 0.25, "width": 0.5, "height": 0.5}},
        "render_region": {"region": {"x": 100, "y": 100, "width": 8, "height": 8}},
        "render_compare": {"maxDimension": 128, "separateImages": True, "variants": [{"name": "Current"}, {"name": "Twin"}]},
    }
    # UI edits re-render App and change its navigation callback identity.
    # Context requests must keep reaching the bridge during those renders.
    deliveries = []
    with ThreadPoolExecutor(max_workers=1) as pool:
        for index in range(8):
            pending = pool.submit(read_tool, url, token, "get_editor_context", {})
            drag = smoke.drag("Exposure", 0.4 if index % 2 == 0 else 0.8)
            context = pending.result()
            if not context.get("editRevision") or context.get("sourceCommit") != build["source"]["head"]:
                raise RuntimeError("Concurrent GUI context read lost revision or source identity")
            deliveries.append({"slider": drag, "editRevision": context["editRevision"]})
    smoke.step("context delivery survives eight concurrent GUI rerenders", {"read_context_requests": len(deliveries), "all_acknowledged": True, "deliveries": deliveries})

    root = case / "revision-client-tests"
    root.mkdir(mode=0o700)
    for client, target in (("claude", 0.4), ("codex", 0.8)):
        folder = root / client
        folder.mkdir(mode=0o700)
        before = state()
        before_history = history()
        context = read_tool(url, token, "get_editor_context", {})
        if context.get("editRevision") != before["editRevision"] or context.get("sourceCommit") != build["source"]["head"] or context.get("sourceDirty") is not False or context.get("appVersion") != "2.2.0":
            raise RuntimeError("Context revision/version/compiled source identity mismatch")
        prompt = "Use only rapidroom MCP: call get_editor_context exactly once. Do not edit or use other tools. Finish after the result."
        command, version = client_command(case, folder, client, 0, prompt_override=prompt, tools=["get_editor_context"])
        events = execute_client(command, folder, client, case / "config")
        first = calls(events, client)
        if [name for name, _, _ in first] != ["get_editor_context"]:
            raise RuntimeError(client + " first actual context call missing")
        if state() != before:
            raise RuntimeError("Actual context call changed edits")
        _, old_preview, _ = images(url, token, "get_preview", {"imagePath": path, "maxDimension": 128})
        smoke.step("actual " + client + " context carries revision/version/source", {"version": version, "editRevision": context["editRevision"], "appVersion": context["appVersion"], "sourceCommit": context["sourceCommit"], "sourceDirty": False})

        drag = smoke.drag("Exposure", target)
        latest = wait_for(lambda: (value if (value := state())["editRevision"] != before["editRevision"] else None), "GUI edit mirrored to MCP")
        committed = wait_for(lambda: (value if (value := history())["historyIndex"] == before_history["historyIndex"] + 1 else None), "one GUI history step committed")
        entry = committed["entries"][committed["historyIndex"]]
        if entry["changedKeys"] != ["exposure"] or entry["actor"] != "user":
            raise RuntimeError("GUI slider did not create one user Exposure step")
        time.sleep(0.8)
        latest_history, latest_sidecars = history(), sidecars()
        capture = smoke.stable_preview("revision-" + client + "-gui-edit")
        save(folder / "gui-edit.json", {"before_revision": before["editRevision"], "latest": latest, "history": latest_history, "sidecars": latest_sidecars})

        def unchanged():
            if state() != latest or history() != latest_history or sidecars() != latest_sidecars:
                raise RuntimeError("Read-only call or refused write changed state/revision/history/sidecar bytes/mtime")

        reads = []
        for tool, extra in options.items():
            args = {"imagePath": path, "expectedRevision": before["editRevision"], **extra}
            stale = read_tool(url, token, tool, args)
            fresh = read_tool(url, token, tool, {**args, "expectedRevision": latest["editRevision"]})
            if stale != fresh or stale["editRevision"] != latest["editRevision"]:
                raise RuntimeError(tool + " did not return the latest read-only state")
            if tool in ("get_preview", "render_compare", "render_region"):
                _, a, _ = images(url, token, tool, args)
                _, b, _ = images(url, token, tool, {**args, "expectedRevision": latest["editRevision"]})
                if a != b or (tool == "get_preview" and a == old_preview):
                    raise RuntimeError(tool + " did not render exact fresh-state pixels")
            reads.append(tool)
            unchanged()
        for extra in ({"original": True}, {"adjustments": {"exposure": 0}}):
            preview = read_tool(url, token, "get_preview", {"imagePath": path, "expectedRevision": before["editRevision"], "maxDimension": 128, **extra})
            if preview["editRevision"] != latest["editRevision"] or preview["renderRevision"] == latest["editRevision"]:
                raise RuntimeError("Original/custom preview confused the edit and render-recipe revision")
            unchanged()
        refused = []
        for tool, extra in (("update_adjustments", {"changes": {"exposure": 1.3}}),
                            ("set_adjustments", {"adjustments": latest["adjustments"]}),
                            ("reset_adjustments", {}), ("undo", {}), ("redo", {}), ("apply_auto_adjustments", {})):
            error = rpc(url, "tools/call", {"name": tool, "arguments": {"imagePath": path, "expectedRevision": before["editRevision"], **extra}}, token=token)
            details = error.get("structuredContent")
            if not error.get("isError") or not details or details["error"] != "revision_conflict" or details["currentRevision"] != latest["editRevision"] or details["changedKeys"] != ["exposure"] or details["actor"] != "user" or details["changesKnown"] is not True:
                raise RuntimeError(tool + " lacks strict structured current-revision/change details")
            refused.append(tool)
            unchanged()
        save(folder / "wire-proof.json", {"reads": reads, "current_revision": latest["editRevision"], "stale_write": details, "all_stale_mutations_refused": refused, "original_custom_edit_revision_correct": True, "all_guards_unchanged": True})
        smoke.step(client + " GUI edit: advisory reads latest and strict stale mutation refused", {"slider": drag, "reads": reads, "stale_write": details, "all_stale_mutations_refused": refused, "original_custom_edit_revision_correct": True, "capture": str(capture.relative_to(case)), "state_revision_history_sidecars_unchanged": True})

        folder = folder / "after-gui"
        folder.mkdir(mode=0o700)
        requests = [{"tool": tool, "arguments": {"imagePath": path, "expectedRevision": before["editRevision"], **extra}} for tool, extra in options.items()]
        requests.append({"tool": "update_adjustments", "arguments": {"imagePath": path, "expectedRevision": before["editRevision"], "changes": {"exposure": 1.3}}})
        prompt = ("Run each of these rapidroom MCP calls exactly once, sequentially, with these exact arguments: " + json.dumps(requests) +
                  ". The expectedRevision is deliberately stale after a user GUI edit. All five reads must succeed; the update must be refused. "
                  "Do not recover, omit/change the revision, make another mutation, use a shell/files, or call another tool. Finish after the refused update.")
        command, _ = client_command(case, folder, client, 0, prompt_override=prompt, tools=[*options, "update_adjustments"])
        events = execute_client(command, folder, client, case / "config")
        actual = calls(events, client)
        if sorted(name for name, _, _ in actual) != sorted([*options, "update_adjustments"]):
            raise RuntimeError(client + " actual advisory-read/stale-write calls missing or repeated")
        if any(arguments.get("expectedRevision") != before["editRevision"] or arguments.get("imagePath") != path for _, arguments, _ in actual):
            raise RuntimeError("Actual client changed or omitted the deliberately stale revision")
        mutation_id = next(identifier for name, _, identifier in actual if name == "update_adjustments")
        results = [v for v in objects(events) if v.get("tool_use_id") == mutation_id or (v.get("id") == mutation_id and v.get("type") == "mcp_tool_call")]
        text = json.dumps(results)
        if "revision_conflict" not in text or latest["editRevision"] not in text:
            raise RuntimeError("Actual client did not receive the structured stale mutation refusal")
        unchanged()
        smoke.step("actual " + client + " latest advisory reads and structured stale-write error", {"version": version, "calls": [name for name, _, _ in actual], "literal_stale_revision_preserved": True, "latest_revision": latest["editRevision"], "state_revision_history_sidecars_unchanged": True})
