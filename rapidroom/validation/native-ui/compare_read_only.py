"""Private-native proof that comparison rendering never mutates metadata."""
import hashlib
import json


def _write(path, value):
    path.write_text(json.dumps(value, sort_keys=True) + "\n")
    json.loads(path.read_text())


def _paths(photo):
    return [photo.with_name(photo.name + suffix) for suffix in (".rrdata", ".rrexif")]


def _snapshot(paths):
    result = {}
    for path in paths:
        result[path.name] = None if not path.exists() else {
            "sha256": hashlib.file_digest(path.open("rb"), "sha256").hexdigest(),
            "mtime_ns": path.stat().st_mtime_ns, "bytes": path.read_text()}
    return result


def _seed(photo, kind):
    sidecar, legacy = _paths(photo)
    sidecar.unlink(missing_ok=True)
    legacy.unlink(missing_ok=True)
    if kind == "uncached":
        return
    metadata = {"adjustments": {"exposure": 0}, "unknown": {"mustStay": True}}
    if kind != "legacy":
        metadata["exif"] = {"Make": "SONY", "Model": "ILCE-7CM2"}
    if kind == "auto":
        metadata["exif"]["SyntheticLongExif"] = "X" * 700
    if kind == "auto":
        metadata["adjustments"] = {"lensCorrectionMode": "auto", "cameraLensCorrectionVersion": 1}
    _write(sidecar, metadata)
    if kind == "legacy":
        _write(legacy, {"Make": "SONY", "Model": "ILCE-7CM2"})


def _click(smoke, name, event):
    smoke.execute("""const image=[...document.images].find(e=>e.alt===arguments[0]);
      if(!image)throw Error('Missing image '+arguments[0]);
      image.dispatchEvent(new MouseEvent(arguments[1],{bubbles:true,button:0}));return true;""", [name, event])


def _ready(smoke, wait_for, name):
    wait_for(lambda: smoke.execute("""return !!document.querySelector('[data-testid=editor-compare-wipe-toggle]') &&
      [...document.images].some(e=>e.alt===arguments[0] && e.complete && e.naturalWidth>100);""", [name]), "decoded editor " + name, 90)


def _original(smoke, wait_for, photo, kind):
    _click(smoke, photo.name, "dblclick")
    _ready(smoke, wait_for, photo.name)
    _seed(photo, kind)  # discard ordinary-open side effects before the guard
    before = _snapshot(_paths(photo))
    smoke.key("KeyC", "c")
    wait_for(lambda: smoke.execute("""const pane=document.querySelector('[data-testid=editor-reference-pane]');
      const images=[...pane.querySelectorAll('img')];return pane?.dataset.referenceMode==='wipe' && images.length>=2 && images.every(e=>e.complete&&e.naturalWidth>100);"""), "Original comparison " + photo.name, 90)
    smoke.key("KeyC", "c")
    after = _snapshot(_paths(photo))
    if after != before: raise RuntimeError("Original comparison changed " + photo.name)
    return {"before": before, "after": after}


def _pinned(smoke, wait_for, photo, kind):
    _seed(photo, kind)  # anchor is already open; establish preconditions immediately before compare
    before = _snapshot(_paths(photo))
    smoke.execute("document.querySelector('[data-testid=editor-reference-view-toggle]').click();return true;")
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-testid=editor-reference-pane]');"), "reference chooser")
    smoke.execute("""const pane=document.querySelector('[data-testid=editor-reference-pane]');
      pane.querySelector('button[aria-label=\"Choose another reference\"]')?.click();return true;""")
    wait_for(lambda: smoke.execute("return document.querySelector('[data-testid=editor-reference-pane]')?.dataset.referenceState==='choose-reference';"), "explicit reference chooser")
    _click(smoke, photo.name, "click")
    wait_for(lambda: smoke.execute("""const pane=document.querySelector('[data-testid=editor-reference-pane]'),image=pane?.querySelector('img');
      return pane?.dataset.referenceState==='ready'&&pane.textContent.includes(arguments[0])&&image?.complete&&image.naturalWidth>100&&document.body.innerText.includes('smoke.ARW');""", [photo.name]), "pinned comparison " + photo.name, 90)
    smoke.execute("document.querySelector('[data-testid=editor-reference-pane] button[aria-label=\"Close reference view\"]').click();return true;")
    after = _snapshot(_paths(photo))
    if after != before: raise RuntimeError("Pinned comparison changed " + photo.name)
    return {"before": before, "after": after}


def run_compare_read_only(case, smoke, wait_for):
    folder = case / "input"
    cases = [(folder / "compare-uncached.ARW", "uncached"), (folder / "compare-legacy.ARW", "legacy"), (folder / "compare-auto.ARW", "auto")]
    _click(smoke, "smoke.ARW", "dblclick")
    _ready(smoke, wait_for, "smoke.ARW")
    pinned = {photo.name: _pinned(smoke, wait_for, photo, kind) for photo, kind in cases}
    originals = {photo.name: _original(smoke, wait_for, photo, kind) for photo, kind in cases}
    smoke.step("comparison render preserves sidecars and legacy EXIF", {"pinned": pinned, "originals": originals})
    healing = folder / "compare-healing.ARW"
    _seed(healing, "auto")
    sidecar, _ = _paths(healing)
    data = json.loads(sidecar.read_text()); data.pop("exif", None); _write(sidecar, data)
    _click(smoke, healing.name, "dblclick")
    wait_for(lambda: (json.loads(sidecar.read_text()).get("exif") or {}).get("Make") == "SONY", "ordinary editor EXIF healing", 45)
    smoke.step("ordinary editor still heals saved Auto recipe", {"after": json.loads(sidecar.read_text())})
