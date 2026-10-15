"""Single-mask native regression on the owned public CC0 fixture (#150)."""

import copy
import json
from pathlib import Path

from PIL import ImageChops

from mcp_clients import client_command, execute_client, image_state, rpc, run_client_checks
from mcp_compact import client_reply
from mcp_history import read_tool
from mcp_revisions import calls


def run_mask_checks(case, smoke):
    from smoke import save, wait_for

    smoke.step("authenticated mask-tool discovery", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    path = str(case / "input/smoke.ARW")
    folder = case / "mask-tests"
    folder.mkdir(mode=0o700)

    def state():
        return image_state(url, path, token=token)

    def mutate(name, **extra):
        return read_tool(url, token, name, {"imagePath": path, "expectedRevision": state()["editRevision"], **extra})

    def history():
        return read_tool(url, token, "history_list", {"imagePath": path})

    environment = json.loads((case / "environment.json").read_text())
    build = json.loads((Path(environment["engine_source"]).parent / "build.json").read_text())
    context = read_tool(url, token, "get_editor_context", {})
    if build["source"]["status"] or context["sourceCommit"] != build["source"]["head"] or context["sourceDirty"]:
        raise RuntimeError("Mask regression requires exact clean build identity")
    def fresh_editor():
        # Drop the frontend cache/history before reading the actual sidecar again.
        smoke.execute("setTimeout(() => location.reload(), 0);return true;")
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.innerText.trim()==='Continue Session' && !e.disabled);"), "fresh frontend session")
        smoke.execute("const e=[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Continue Session');e.click();return true;")
        wait_for(lambda: smoke.execute("return [...document.images].some(e=>e.src.startsWith('asset:') && e.complete && e.naturalWidth>100);"), "reloaded library")
        # Export adds another library image; select the exact RAW, not the first thumbnail.
        read_tool(url, token, "select_image", {"imagePath": path})
        wait_for(lambda: smoke.execute("return document.body.innerText.includes('4608 × 3072') && document.body.innerText.includes('Exposure');"), "fresh editor loads sidecar")

    initial = state()
    if initial["adjustments"]["masks"]:
        raise RuntimeError("Mask fixture was not initially empty")
    first = {"id": "first", "name": "First local edit", "adjustments": {"exposure": 0.2}, "subMasks": [
        {"id": "first-radial", "type": "radial", "parameters": {"centerX": 1500, "centerY": 1500,
         "radiusX": 900, "radiusY": 900, "rotation": 0, "feather": 0.5}}]}
    mutate("upsert_mask", mask=first)
    seeded = state()
    first = copy.deepcopy(seeded["adjustments"]["masks"][0])
    seeded_pixels = smoke.crop(smoke.stable_preview("first-mask"))
    second = {"id": "second", "name": "Second local edit", "adjustments": {"exposure": 1,
        "hsl": {"yellows": {"hue": 25, "saturation": -20}}}, "subMasks": [
        {"id": "second-radial", "type": "radial", "parameters": {"centerX": 3250, "centerY": 1550,
         "radiusX": 1200, "radiusY": 1500, "rotation": -20, "feather": 0.8}}]}
    prompt = (f"Using only rapidroom MCP tools on imagePath {json.dumps(path)}, first call get_image_state. "
        f"Then call upsert_mask exactly once with mask {json.dumps(second)} and expectedRevision from that state. "
        "Then call get_image_state again. Do not change or resend the first mask, call any other tool, or use files/shell. Finish.")
    command, version = client_command(case, folder, "claude", 0, prompt_override=prompt,
                                      tools=["get_image_state", "upsert_mask"])
    events = execute_client(command, folder, "claude", case / "config")
    recorded = calls(events, "claude")
    mutations = [call for call in recorded if call[0] == "upsert_mask"]
    if len(mutations) != 1 or mutations[0][1]["mask"] != second:
        raise RuntimeError("Actual Claude did not add exactly the requested second mask")
    reply = client_reply(events, "claude", mutations[0][2])
    added = state()
    if added["adjustments"]["masks"][0] != first or len(added["adjustments"]["masks"]) != 2:
        raise RuntimeError("Single-mask add changed its sibling")
    yellows = added["adjustments"]["masks"][1]["adjustments"]["hsl"]["yellows"]
    if yellows != {"hue": 25, "saturation": -20, "luminance": 0}:
        raise RuntimeError("Partial local HSL defaults did not fill on real creation")
    if "adjustments" in reply or reply["changedKeys"] != ["masks"] or reply["editRevision"] != added["editRevision"]:
        raise RuntimeError("Single-mask reply is not compact or accurate")
    added_pixels = smoke.crop(smoke.stable_preview("two-masks"))
    if ImageChops.difference(seeded_pixels, added_pixels).getbbox() is None:
        raise RuntimeError("Actual second mask did not render")
    smoke.execute("const e=document.querySelector('[data-bench-id=undo]');if(!e||e.disabled)throw Error('Undo missing');e.click();return true;")
    wait_for(lambda: state()["adjustments"] == seeded["adjustments"], "GUI Undo of actual-client single mask")
    undone_pixels = smoke.crop(smoke.stable_preview("mask-undo"))
    if ImageChops.difference(seeded_pixels, undone_pixels).getbbox() is not None:
        raise RuntimeError("Mask Undo did not restore exact native preview")
    mutate("redo")
    if state()["adjustments"] != added["adjustments"]:
        raise RuntimeError("Shared history redo lost mask fields")
    mutate("upsert_mask", mask={"id": "second", "adjustments": {"hsl": {"yellows": {"hue": -10}}}})
    patched = state()
    if patched["adjustments"]["masks"][0] != first or patched["adjustments"]["masks"][1]["adjustments"]["hsl"]["yellows"] != {"hue": -10, "saturation": -20, "luminance": 0}:
        raise RuntimeError("Nested patch replaced sibling mask or omitted local values")

    def saved_masks():
        for sidecar in (case / "input").rglob("*.rrdata"):
            value = json.loads(sidecar.read_text())
            if value.get("adjustments", {}).get("masks") == patched["adjustments"]["masks"]:
                return str(sidecar.relative_to(case))
        return None

    sidecar = wait_for(saved_masks, "normal autosave of both masks")
    fresh_editor()
    reloaded = state()
    if len(history()["entries"]) != 1:
        raise RuntimeError("Sidecar reload retained the previous frontend history/cache")
    if reloaded["adjustments"]["masks"] != patched["adjustments"]["masks"]:
        raise RuntimeError("Sidecar reload did not preserve masks/defaults")
    mutate("remove_mask", maskId="second")
    remaining = state()
    if remaining["adjustments"]["masks"] != [first]:
        raise RuntimeError("Single-mask remove lost sibling")
    before_history = history()
    mutate("remove_mask", maskId="missing")
    if state() != remaining or history() != before_history:
        raise RuntimeError("Missing-mask removal changed revision/history")
    error = rpc(url, "tools/call", {"name": "upsert_mask", "arguments": {"imagePath": path,
        "expectedRevision": remaining["editRevision"], "mask": {"id": "first", "opacity": -1}}}, token=token)
    if not error.get("isError") or state() != remaining:
        raise RuntimeError("Invalid mask opacity was accepted or mutated state")
    # Test #211/#212 together without sending hostile fixture text to a model.
    saved = case / sidecar
    wait_for(lambda: json.loads(saved.read_text()).get("adjustments", {}).get("masks") == remaining["adjustments"]["masks"], "single sibling autosave")
    stored = json.loads(saved.read_text())
    native_name = "z" * 200
    stored["adjustments"]["masks"][0]["name"] = native_name
    saved.write_text(json.dumps(stored))
    fresh_editor()
    displayed = read_tool(url, token, "get_editor_context", {})["masks"][0]["name"]
    if displayed != native_name[:128]:
        raise RuntimeError("Context did not produce the expected bounded native name")
    mutate("upsert_mask", mask={"id": "first", "name": displayed, "opacity": 80})
    echoed = state()
    if echoed["adjustments"]["masks"][0]["name"] != native_name:
        raise RuntimeError("Top-level single-mask echo renamed the stored native mask")
    wait_for(lambda: json.loads(saved.read_text())["adjustments"]["masks"][0]["name"] == native_name, "native name echo preserved on disk")
    mutate("upsert_mask", mask={"id": "first", "name": "New\0\u202e password=planted"})
    if state()["adjustments"]["masks"][0]["name"] != "New [REDACTED]":
        raise RuntimeError("Assistant single-mask name was not projected before UI storage")
    long_id = "q" * 128
    mutate("upsert_mask", mask={"id": long_id, "subMasks": []})
    if state()["adjustments"]["masks"][-1]["id"] != long_id:
        raise RuntimeError("Opaque mask ID was changed in the output projection")
    mutate("remove_mask", maskId=long_id)
    before_invalid_id = state()
    for name, args in (("upsert_mask", {"mask": {"id": "q" * 129, "subMasks": []}}), ("remove_mask", {"maskId": "bad id"})):
        error = rpc(url, "tools/call", {"name": name, "arguments": {"imagePath": path, **args}}, token=token)
        if not error.get("isError") or state() != before_invalid_id:
            raise RuntimeError("Invalid opaque identifier was accepted or changed the editor")
    smoke.capture("single-mask-policy")
    result = {"sourceCommit": context["sourceCommit"], "appVersion": context["appVersion"],
        "client": {"name": "Claude", "version": version, "calls": [call[0] for call in recorded]},
        "first_mask_unchanged": True, "partial_hsl_defaults_filled": True, "native_preview_changed": True,
        "gui_undo_restores_exact_preview": True, "shared_redo_exact": True, "nested_patch_preserves_omitted_values": True,
        "sidecar": sidecar, "sidecar_reload_exact": True, "remove_preserves_sibling": True,
        "missing_remove_is_noop": True, "invalid_mutation_refused": True, "compact_reply": reply, "top_level_context_name_echo_preserved": True,
        "assistant_rename_projected_before_storage": True, "opaque_128_byte_id_roundtrips": True,
        "invalid_opaque_ids_refused": True, "hostile_name_phase_model_requests": False}
    save(folder / "result.json", result)
    smoke.step("actual-client single-mask add, local defaults, shared history and sidecar round-trip", result)
