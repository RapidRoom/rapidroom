"""Native no-op crop guards and one real crop/Undo on the owned CC0 photo."""

import json
import time

from PIL import ImageChops

from mcp_clients import image_state, run_client_checks
from mcp_history import read_tool


def run_crop_checks(case, smoke, baseline):
    from smoke import save, sha, wait_for

    wire = run_client_checks(case, smoke, wire_only=True)
    wire["transport"] = "authenticated loopback HTTP for native guard reads; no model requests"
    smoke.step("private authenticated MCP reads for crop guards", wire)
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    image = str(case / "input/smoke.ARW")

    def state():
        return image_state(url, image, token=token)

    def history():
        return read_tool(url, token, "history_list", {"imagePath": image})

    def sidecars():
        return {str(f.relative_to(case)): {"sha256": sha(f), "mtime_ns": f.stat().st_mtime_ns}
                for f in (case / "input").rglob("*.rrdata")}

    def select(panel):
        smoke.execute("const e=document.querySelector('[data-tab-id=layout-tab-'+arguments[0]+']');if(!e)throw Error('Crop panel switcher missing');e.click();return true;", [panel])

    def open_crop():
        select("crop")
        wait_for(lambda: smoke.execute("const e=document.querySelector('.ReactCrop [data-ord=se]');return !!e && e.getBoundingClientRect().width>0;"), "real crop overlay")
        time.sleep(0.8)

    initial = state()
    if initial["adjustments"]["crop"] is not None:
        raise RuntimeError("Expected an uncropped native fixture")
    initial_history = history()
    time.sleep(0.8)
    initial_sidecars = sidecars()
    save(case / "crop-guards-initial.json", {"state": initial, "history": initial_history, "sidecars": initial_sidecars})

    def assert_unchanged(phase):
        now, past, files = state(), history(), sidecars()
        evidence = {"phase": phase, "state": now, "history": past, "sidecars": files,
                    "state_unchanged": now == initial, "history_unchanged": past == initial_history,
                    "sidecars_unchanged": files == initial_sidecars}
        save(case / ("crop-guard-" + phase.replace(" ", "-") + ".json"), evidence)
        if not all(evidence[key] for key in ("state_unchanged", "history_unchanged", "sidecars_unchanged")):
            raise RuntimeError(phase + " changed state/revision/history or sidecar bytes/mtime")

    open_crop()
    smoke.capture("crop-full-frame-open")
    assert_unchanged("opening the no-op full-frame crop")
    select("adjustments")
    time.sleep(0.8)
    assert_unchanged("closing the no-op full-frame crop")
    smoke.step("opening/closing crop preserves revision, history and sidecars", {"editRevision": initial["editRevision"], "historyIndex": initial_history["historyIndex"], "sidecars": initial_sidecars})

    open_crop()
    point = smoke.execute("const r=document.querySelector('.ReactCrop [data-ord=se]').getBoundingClientRect();return [r.left+r.width/2,r.top+r.height/2];")
    smoke.execute("const e=document.querySelector('.ReactCrop [data-ord=se]');e.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,cancelable:true,pointerId:1,pointerType:'mouse',isPrimary:true,button:0,buttons:1,clientX:arguments[0],clientY:arguments[1]}));return true;", point)
    time.sleep(0.2)
    smoke.execute("document.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerId:1,pointerType:'mouse',isPrimary:true,button:0,clientX:arguments[0],clientY:arguments[1]}));return true;", point)
    time.sleep(0.8)
    assert_unchanged("completing an unchanged full-frame crop")
    smoke.step("no-movement crop completion records no edit", {"editRevision": initial["editRevision"], "history_entries": len(initial_history["entries"])})

    # The library's actual keyboard handler commits this corner resize once.
    smoke.execute("const e=document.querySelector('.ReactCrop [data-ord=se]');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,cancelable:true,key:'ArrowLeft',ctrlKey:true}));return true;")
    changed = wait_for(lambda: (value if (value := state())["editRevision"] != initial["editRevision"] else None), "real crop changes revision")
    crop = changed["adjustments"]["crop"]
    if not crop or not (0 < crop["width"] < 4608 and 0 < crop["height"] <= 3072):
        raise RuntimeError("Actual corner resize did not crop the photo")
    expected_index = initial_history["historyIndex"] + 1
    changed_history = wait_for(lambda: (value if (value := history())["historyIndex"] == expected_index else None), "debounced real crop history step")
    if changed_history["historyIndex"] != expected_index or len(changed_history["entries"]) != expected_index + 1:
        raise RuntimeError("Real crop did not record exactly one history step")
    entry = changed_history["entries"][expected_index]
    if entry["actor"] != "user" or entry["changedKeys"] != ["crop"]:
        raise RuntimeError("Real crop history actor/keys are wrong")
    wait_for(lambda: sidecars() != initial_sidecars, "real crop sidecar saved")
    smoke.capture("crop-real-corner-edit")
    smoke.step("one real crop records one user history step", {"crop": crop, "editRevision": changed["editRevision"], "history_entry": entry})

    smoke.execute("const e=document.querySelector('[data-bench-id=undo]');if(!e||e.disabled)throw Error('Crop Undo unavailable');e.click();return true;")
    wait_for(lambda: state()["adjustments"] == initial["adjustments"], "crop Undo restores adjustments")
    time.sleep(0.8)
    undone = history()
    if undone["historyIndex"] != initial_history["historyIndex"]:
        raise RuntimeError("Crop Undo did not restore the history index")
    select("adjustments")
    restored = smoke.stable_preview("crop-undo-restored")
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError("Crop Undo did not restore exact native preview pixels")
    smoke.step("crop Undo restores original preview and stays uncropped", {"historyIndex": undone["historyIndex"], "crop": state()["adjustments"]["crop"], "native_preview_exact": True, "real_assistants_started": False})
