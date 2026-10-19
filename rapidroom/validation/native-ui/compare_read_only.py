"""Private-native proof that comparison rendering never mutates metadata."""

import hashlib
from pathlib import Path


def _snapshot(paths):
    return {
        path.name: None if not path.exists() else {
            "sha256": hashlib.file_digest(path.open("rb"), "sha256").hexdigest(),
            "mtime_ns": path.stat().st_mtime_ns,
            "bytes": path.read_text(),
        }
        for path in paths
    }


def _click_image(smoke, name, event):
    smoke.execute("""const image=[...document.images].find(e=>e.alt===arguments[0]);
      if(!image)throw Error('Missing image '+arguments[0]);
      image.dispatchEvent(new MouseEvent(arguments[1],{bubbles:true,button:0}));return true;""", [name, event])


def _original(smoke, wait_for, name):
    _click_image(smoke, name, "dblclick")
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-testid=editor-compare-wipe-toggle]');"), "editor for " + name)
    smoke.key("KeyC", "c")
    wait_for(lambda: smoke.execute("""const pane=document.querySelector('[data-testid=editor-reference-pane]');
      return pane?.dataset.referenceMode==='wipe' && pane.querySelectorAll('img').length>=2;"""), "Original comparison " + name)
    smoke.key("KeyC", "c")


def _pinned(smoke, wait_for, name):
    smoke.execute("document.querySelector('[data-testid=editor-reference-view-toggle]').click();return true;")
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-testid=editor-reference-pane]');"), "reference chooser")
    _click_image(smoke, name, "click")
    wait_for(lambda: smoke.execute("""const pane=document.querySelector('[data-testid=editor-reference-pane]');
      return pane?.dataset.referenceState==='ready' && pane.querySelector('img')?.naturalWidth>100;"""), "pinned comparison " + name, 90)
    smoke.execute("document.querySelector('[data-testid=editor-reference-pane] button[aria-label=\"Close reference view\"]').click();return true;")


def run_compare_read_only(case, smoke, wait_for):
    folder = case / "input"
    uncached, legacy, saved_auto = (folder / "compare-uncached.ARW", folder / "compare-legacy.ARW", folder / "compare-auto.ARW")
    guarded = [path.with_name(path.name + suffix) for path in (uncached, legacy, saved_auto) for suffix in (".rrdata", ".rrexif")]
    before = _snapshot(guarded)
    for path in (uncached, legacy, saved_auto):
        _original(smoke, wait_for, path.name)
    _click_image(smoke, uncached.name, "dblclick")
    for path in (legacy, saved_auto):
        _pinned(smoke, wait_for, path.name)
    after = _snapshot(guarded)
    if after != before:
        raise RuntimeError("Comparison changed a sidecar or legacy EXIF cache")
    smoke.step("comparison render preserves sidecars and legacy EXIF", {"before": before, "after": after})
    _click_image(smoke, saved_auto.name, "dblclick")
    target = saved_auto.with_name(saved_auto.name + ".rrdata")
    wait_for(lambda: _snapshot([target])[target.name] != before[target.name], "ordinary editor EXIF healing", 45)
    smoke.step("ordinary editor still heals saved Auto recipe", {"before": before[target.name], "after": _snapshot([target])[target.name]})
