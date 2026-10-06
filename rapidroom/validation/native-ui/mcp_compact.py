"""Measure real three-mask MCP payloads and exercise both installed clients."""

import copy
import json
import math
import time
from pathlib import Path

from mcp_clients import client_command, execute_client, image_state, objects, rpc, run_client_checks
from mcp_history import read_tool
from mcp_measure import images, check_statistics
from mcp_revisions import calls


def byte_size(value):
    return len(json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode())


def client_reply(events, client, identifier):
    if client == "claude":
        value = next(v for v in objects(events) if v.get("type") == "tool_result" and v.get("tool_use_id") == identifier)
        if value.get("is_error"):
            raise RuntimeError("Actual Claude mutation failed")
        content = value["content"]
        if isinstance(content, list):
            content = next(v["text"] for v in content if v.get("type") == "text")
        return json.loads(content)
    value = {v["id"]: v for v in objects(events) if v.get("type") == "mcp_tool_call"}[identifier]
    if value["status"] != "completed":
        raise RuntimeError("Actual Codex mutation failed")
    result = value["result"]
    return result.get("structured_content") or json.loads(next(v["text"] for v in result["content"] if v["type"] == "text"))


def run_compact_checks(case, smoke):
    from smoke import save, sha, wait_for

    baseline = json.loads((case / "mcp-compact-test.json").read_text())["baseline"]
    smoke.step("authenticated endpoint and MCP discovery", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    path = str(case / "input/smoke.ARW")
    folder = case / "compact-tests"
    folder.mkdir(mode=0o700)
    result = {"baseline": baseline, "byte_count": "UTF-8 compact JSON serialization of the actual tools/call result (text and structured content), excluding JSON-RPC framing and image tokens", "replies": {}, "clients": {}}

    def state():
        return image_state(url, path, token=token)

    def mutate(tool, extra, verbose=None):
        before = state()
        arguments = {"imagePath": path, "expectedRevision": before["editRevision"], **extra}
        if verbose is not None:
            arguments["verbose"] = verbose
        response = rpc(url, "tools/call", {"name": tool, "arguments": arguments}, token=token)
        if response.get("isError"):
            raise RuntimeError(tool + " failed")
        after = state()
        value = response["structuredContent"]
        actual_keys = sorted(key for key in before["adjustments"].keys() | after["adjustments"].keys()
                             if before["adjustments"].get(key) != after["adjustments"].get(key))
        if value["editRevision"] != after["editRevision"]:
            raise RuntimeError("Mutation reply has wrong revision")
        full = baseline or verbose is True
        if full:
            if value["adjustments"] != after["adjustments"]:
                raise RuntimeError("Full mutation result differs from authoritative state")
        elif "adjustments" in value or value["changedKeys"] != actual_keys:
            raise RuntimeError("Compact result echoed adjustments or reported inaccurate changed keys")
        return {"result_json_bytes": byte_size(response), "text_bytes": sum(len(v.get("text", "").encode()) for v in response["content"]),
                "changed_keys": actual_keys, "mask_count_before": len(before["adjustments"]["masks"]),
                "mask_count_after": len(after["adjustments"]["masks"]), "full_adjustments_returned": full}, after

    initial = state()
    environment = json.loads((case / "environment.json").read_text())
    build = json.loads((Path(environment["engine_source"]).parent / "build.json").read_text())
    context = read_tool(url, token, "get_editor_context", {})
    if build["source"]["status"] or build["engine_sha256"] != environment["engine_sha256"] or context.get("sourceCommit") != build["source"]["head"] or context.get("sourceDirty") is not False or context.get("editRevision") != initial["editRevision"]:
        raise RuntimeError("Compact scenario requires the exact clean pinned source identity")
    result["source"] = build["source"]
    result["engine_sha256"] = build["engine_sha256"]
    result["stdio_adapter_sha256"] = build["stdio_adapter_sha256"]
    width, height = 4608, 3072
    # Use complete neutral mask recipes so byte comparisons isolate response size.
    mask_keys = ("blacks brightness clarity colorGrading colorNoiseReduction contrast curves pointCurves parametricCurve curveMode "
                 "dehaze exposure flareAmount glowAmount halationAmount highlights hsl hue lumaNoiseReduction saturation shadows "
                 "sharpness sharpnessThreshold structure temperature tint vibrance whites").split()
    mask_defaults = {key: copy.deepcopy(initial["adjustments"][key]) for key in mask_keys}
    mask_defaults["sectionVisibility"] = {key: True for key in ("basic", "curves", "color", "colorGrading", "colorMixer", "details", "effects")}
    masks = [{"id": f"compact-radial-{index}", "name": f"Public fixture radial {index}", "visible": True, "invert": False, "opacity": 100,
              "adjustments": {**copy.deepcopy(mask_defaults), "exposure": index / 20}, "subMasks": [{"id": f"compact-sub-{index}", "name": "Radial", "type": "radial",
              "mode": "additive", "visible": True, "invert": False, "opacity": 100,
              "parameters": {"centerX": width * index / 4, "centerY": height / 2, "radiusX": width / 6,
                             "radiusY": height / 4, "rotation": 0, "feather": 0.5}}]} for index in (1, 2, 3)]
    _, seeded = mutate("update_adjustments", {"changes": {"masks": masks}})
    if len(seeded["adjustments"]["masks"]) != 3 or any(len(mask["adjustments"]) != len(mask_defaults) for mask in seeded["adjustments"]["masks"]):
        raise RuntimeError("Three full radial fixture recipes did not round-trip")
    three_mask = copy.deepcopy(seeded["adjustments"])
    result["replies"]["update_adjustments_default"], updated = mutate("update_adjustments", {"changes": {"exposure": 0.2}})
    recipe = copy.deepcopy(updated["adjustments"])
    recipe["exposure"] = 0.3
    result["replies"]["set_adjustments_default"], _ = mutate("set_adjustments", {"adjustments": recipe})
    result["replies"]["reset_adjustments_default"], reset = mutate("reset_adjustments", {})
    if reset["adjustments"]["masks"] or reset["adjustments"]["exposure"] != 0:
        raise RuntimeError("Compact reset did not reset the actual editor")
    result["replies"]["set_adjustments_verbose"], _ = mutate("set_adjustments", {"adjustments": three_mask}, True)
    result["replies"]["update_adjustments_verbose"], _ = mutate("update_adjustments", {"changes": {"exposure": 0.4}}, True)
    result["replies"]["reset_adjustments_verbose"], _ = mutate("reset_adjustments", {}, True)
    _, _ = mutate("set_adjustments", {"adjustments": three_mask})
    listed = rpc(url, "tools/list", {}, token=token)
    result["discovery"] = {"tools_list_json_bytes": byte_size(listed),
                           "input_schema_json_bytes": {tool["name"]: byte_size(tool["inputSchema"]) for tool in listed["tools"]}}
    if not baseline:
        for tool in listed["tools"]:
            if tool["name"] in ("set_adjustments", "update_adjustments", "get_preview") and not tool["inputSchema"].get("$defs"):
                raise RuntimeError("Discovery schema lacks local definitions")
    smoke.step("three real radial masks: measured default/verbose mutation replies and discovery", {"baseline": baseline, "replies": result["replies"], "discovery": result["discovery"]})

    stable = state()
    stable_history = read_tool(url, token, "history_list", {"imagePath": path})
    time.sleep(0.8)
    def sidecars():
        return {f.name: {"sha256": sha(f), "mtime_ns": f.stat().st_mtime_ns} for f in (case / "input").glob("*.rrdata")}
    stable_sidecars = sidecars()
    def unchanged():
        if state() != stable or read_tool(url, token, "history_list", {"imagePath": path}) != stable_history or sidecars() != stable_sidecars:
            raise RuntimeError("Read-only measurements changed state/history/sidecar bytes or mtime")
    for tool, extra in (("analyze", {}), ("sample_region", {"region": {"x": 0.25, "y": 0.25, "width": 0.5, "height": 0.5}})):
        arguments = {"imagePath": path, "maxDimension": 128, **extra}
        response = rpc(url, "tools/call", {"name": tool, "arguments": arguments}, token=token)
        full = rpc(url, "tools/call", {"name": tool, "arguments": {**arguments, "histogram": True}}, token=token)
        if response.get("isError") or full.get("isError"):
            raise RuntimeError("Summary/histogram measurement failed")
        summary, detailed = response["structuredContent"], full["structuredContent"]
        if not baseline and ("histogram" in summary or {k: v for k, v in detailed.items() if k != "histogram"} != summary):
            raise RuntimeError("Summary dropped exact statistics or kept default histograms")
        if "histogram" not in detailed:
            raise RuntimeError("Explicit histogram opt-in did not return bins")
        result["replies"][tool + "_default"] = {"result_json_bytes": byte_size(response), "histogram_returned": "histogram" in summary}
        result["replies"][tool + "_histogram"] = {"result_json_bytes": byte_size(full), "histogram_returned": True}
        unchanged()
    variants = [{"name": name} for name in ("Current", "Twin", "Neutral", "Same")]
    arguments = {"imagePath": path, "maxDimension": 1000, "variants": variants}
    response, payloads, sheets = images(url, token, "render_compare", arguments)
    _, _, separate = images(url, token, "render_compare", {**arguments, "separateImages": True})
    if len(separate) != 4 or any(frame.tobytes() != separate[0].tobytes() for frame in separate[1:]):
        raise RuntimeError("Presentation comparison changed variant pixels")
    source_width, source_height = separate[0].size
    scale = min(500 / source_width, 476 / source_height)
    expected_sheet = (2 * max(1, math.floor(source_width * scale + 0.5)),
                      2 * (max(1, math.floor(source_height * scale + 0.5)) + 24))
    if not baseline and sheets[0].size != expected_sheet:
        raise RuntimeError("Four landscape variants still have square padding")
    (case / "captures/mcp-tight-comparison.png").write_bytes(payloads[0])
    result["comparison"] = {"sheet_dimensions": list(sheets[0].size), "variant_dimensions": [list(v.size) for v in separate],
                            "sheet_png_sha256": sha(case / "captures/mcp-tight-comparison.png"), "labels": response["structuredContent"]["labels"],
                            "separate_variant_pixels_identical": True}
    unchanged()
    # Independently verify the opt-in histograms against exactly decoded pre-JPEG pixels.
    stats = read_tool(url, token, "analyze", {"imagePath": path, "maxDimension": 1000, "histogram": True})
    check_statistics(stats, separate[0])
    unchanged()
    smoke.step("summary/histogram bytes, tight labels and unchanged read-only geometry", {"baseline": baseline, "replies": {k: v for k, v in result["replies"].items() if k.startswith(("analyze", "sample_region"))}, "comparison": result["comparison"], "independent_opt_in_pixel_statistics": True, "state_history_sidecars_unchanged": True})
    save(folder / "result.json", result)
    if baseline:
        return

    for client, exposure in (("claude", 0.5), ("codex", 1.0)):
        current = state()
        current_history = read_tool(url, token, "history_list", {"imagePath": path})
        pixels = smoke.crop(smoke.stable_preview(client + "-compact-before"))
        changes = {"exposure": exposure, "hsl": {"greens": {"saturation": exposure * 10}},
                   "colorGrading": {"midtones": {"hue": 90, "saturation": exposure * 4}},
                   "parametricCurve": {"luma": {"lights": exposure * 6}}}
        prompt = (f"Run only these rapidroom MCP calls for imagePath {json.dumps(path)}: first get_editor_context; "
                  "then update_adjustments exactly once with expectedRevision from that context and changes " + json.dumps(changes) +
                  ". Do not set verbose: use the compact default response. Then analyze maxDimension 128 with default summary (no histogram argument). "
                  "Then render_compare maxDimension 512 with variants [{name:'Current'},{name:'Twin'}]. "
                  "Do not change masks or use shell/files/other tools. Finish after all calls.")
        client_folder = folder / client
        client_folder.mkdir(mode=0o700)
        command, version = client_command(case, client_folder, client, exposure, prompt_override=prompt,
                                          tools=["get_editor_context", "update_adjustments", "analyze", "render_compare"])
        events = execute_client(command, client_folder, client, case / "config")
        actual = calls(events, client)
        if sorted(name for name, _, _ in actual) != sorted(["get_editor_context", "update_adjustments", "analyze", "render_compare"]):
            raise RuntimeError("Actual compact client calls missing or repeated")
        mutation = next(v for v in actual if v[0] == "update_adjustments")
        if mutation[1].get("changes") != changes or mutation[1].get("expectedRevision") != current["editRevision"] or mutation[1].get("verbose", False) is not False:
            raise RuntimeError("Actual client changed the requested nested patch or compact revision arguments")
        reply = client_reply(events, client, mutation[2])
        after = state()
        after_history = wait_for(lambda: (value if (value := read_tool(url, token, "history_list", {"imagePath": path}))["historyIndex"] == current_history["historyIndex"] + 1 else None), "one assistant history step")
        entry = after_history["entries"][after_history["historyIndex"]]
        after_pixels = smoke.crop(smoke.stable_preview(client + "-compact-after"))
        if after["adjustments"]["exposure"] != exposure or after["adjustments"]["masks"] != current["adjustments"]["masks"] or after_pixels.tobytes() == pixels.tobytes():
            raise RuntimeError("Actual nested edit failed or changed unrelated masks")
        if "adjustments" in reply or reply["editRevision"] != after["editRevision"] or reply["changedKeys"] != entry["changedKeys"] or entry["actor"] != "assistant":
            raise RuntimeError("Actual client did not receive a compact accurate reply/shared assistant history")
        summary = client_reply(events, client, next(v[2] for v in actual if v[0] == "analyze"))
        if "histogram" in summary:
            raise RuntimeError("Actual summary response contains unwanted histograms")
        result["clients"][client] = {"version": version, "calls": [v[0] for v in actual], "nested_local_schema_patch_accepted": True,
                                     "compact_revision_changed_keys_reply": True, "changed_keys": reply["changedKeys"],
                                     "three_masks_preserved": True, "one_shared_assistant_history_entry": True,
                                     "native_preview_changed": True, "summary_histogram_omitted": True}
        save(folder / "result.json", result)
        smoke.step("actual " + client + " nested schemas, compact edit reply and summary/compare", result["clients"][client])
