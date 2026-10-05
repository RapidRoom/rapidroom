#!/usr/bin/env python3
"""Exercise the real Linux release on a private Weston compositor (issue #134)."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request

from PIL import Image, ImageChops, ImageStat


ROOT = Path(__file__).resolve().parents[3]
FIXTURE = "sony-a7c2-15mp-uncompressed.ARW"
WIDTH, HEIGHT = 1680, 1050


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def wait_for(check, description, timeout=45):
    deadline = time.monotonic() + timeout
    while True:
        value = check()
        if value:
            return value
        if time.monotonic() >= deadline:
            raise RuntimeError("Timed out: " + description)
        time.sleep(0.25)


def request(url, body=None, method=None):
    payload = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=payload, method=method,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            result = json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error
    if isinstance(result.get("value"), dict) and "error" in result["value"]:
        raise RuntimeError(str(result["value"]))
    return result


class Smoke:
    def __init__(self, case):
        self.case = case
        self.processes = []
        self.result = {"passed": False, "steps": []}
        self.url = None

    def start(self, command, name, env=None):
        with (self.case / (name + ".log")).open("w") as log:
            process = subprocess.Popen(command, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
        self.processes.append(process)
        return process

    def execute(self, script, args=None):
        return request(self.url + "/execute/sync", {"script": script, "args": args or []})["value"]

    def step(self, name, evidence=None):
        self.result["steps"].append({"name": name, "evidence": evidence})
        save(self.case / "result.json", self.result)
        print("PASS " + name, flush=True)

    def capture(self, name):
        folder = self.case / "captures" / name
        folder.mkdir(parents=True)
        (self.case / "config/user-dirs.dirs").write_text(
            "XDG_PICTURES_DIR=" + json.dumps(str(folder)) + "\n")
        env = os.environ.copy()
        env["XDG_PICTURES_DIR"] = str(folder)
        # Constrain even a screenshooter fallback path to this owned case.
        with (folder / "capture.log").open("w") as log:
            subprocess.run(["bwrap", "--ro-bind", "/", "/", "--bind",
                            str(self.case), str(self.case), "--", "weston-screenshooter"],
                           env=env, cwd=folder, stdout=log, stderr=subprocess.STDOUT,
                           timeout=20, check=True)
        files = list(folder.glob("*.png"))
        if len(files) != 1:
            raise RuntimeError("Expected one compositor screenshot: " + name)
        with Image.open(files[0]) as image:
            if image.size != (WIDTH, HEIGHT):
                raise RuntimeError("Unexpected compositor dimensions")
        return files[0]

    def snapshot(self, name):
        data = self.execute("""return {
          text: document.body.innerText,
          buttons: [...document.querySelectorAll('button')].map(e => ({
            text: e.innerText, tooltip: e.getAttribute('data-tooltip'), disabled: e.disabled})),
          images: [...document.images].map(e => ({src:e.src, complete:e.complete,
            width:e.naturalWidth, height:e.naturalHeight})),
          errors: window.__RAPIDROOM_SMOKE_ERRORS__
        };""")
        save(self.case / (name + "-dom.json"), data)
        return data

    def key(self, code, key, ctrl=False):
        self.execute("""document.activeElement?.blur();
          for (const type of ['keydown','keyup']) document.dispatchEvent(new KeyboardEvent(type,
            {bubbles:true,code:arguments[0],key:arguments[1],ctrlKey:arguments[2]}));
          return true;""", [code, key, ctrl])

    def slider(self, label):
        return self.execute("""const e=[...document.querySelectorAll('input[type=range]')]
          .find(e=>e.parentElement.parentElement.textContent.trim().startsWith(arguments[0]));
          if (!e) throw Error('Missing slider '+arguments[0]);
          const r=e.getBoundingClientRect();
          return {value:Number(e.value),min:Number(e.min),max:Number(e.max),step:Number(e.step),
            width:r.width,height:r.height};""", [label])

    def drag(self, label, value):
        before = self.slider(label)
        self.execute("""const e=[...document.querySelectorAll('input[type=range]')]
          .find(e=>e.parentElement.parentElement.textContent.trim().startsWith(arguments[0]));
          const r=e.getBoundingClientRect(),x=r.left+(arguments[1]-Number(e.min)) /
            (Number(e.max)-Number(e.min))*r.width;
          e.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,button:0,buttons:1,
            clientX:x,clientY:r.top+r.height/2}));return true;""", [label, value])
        time.sleep(0.3)  # React installs the document drag listeners after mousedown.
        self.execute("document.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,button:0}));return true;")
        after = wait_for(lambda: self.slider(label) if self.slider(label)["value"] != before["value"] else None,
                         label + " changes")
        # MouseEvent coordinates are integral in WebKit; allow track pixel quantization.
        tolerance = 2 * after["step"] + 3 * (after["max"] - after["min"]) / after["width"]
        if abs(after["value"] - value) > tolerance:
            raise RuntimeError(f"Slider missed requested position: {label}, requested={value}, observed={after}, before={before}")
        return {"label": label, "requested": value, "before": before, "after": after}

    def crop(self, path):
        with Image.open(path) as image:
            return image.convert("RGB").crop(self.roi)

    def stable_preview(self, name):
        previous = None
        for attempt in range(12):
            time.sleep(0.5)
            path = self.capture(name + "-" + str(attempt))
            image = self.crop(path)
            if previous is not None and ImageChops.difference(previous, image).getbbox() is None:
                stats = ImageStat.Stat(image)
                if max(stats.stddev) < 10 or max(stats.mean) < 10:
                    previous = image
                    continue
                return path
            previous = image
        raise RuntimeError("Preview did not settle: " + name)

    def run(self):
        runtime = Path(os.environ["XDG_RUNTIME_DIR"])
        if os.environ.get("DISPLAY") or os.environ["WAYLAND_DISPLAY"] != "rapidroom-smoke":
            raise RuntimeError("Private display isolation missing")
        self.start(["weston", "--backend=headless", "--renderer=gl", "--shell=kiosk-shell.so",
                    "--socket=rapidroom-smoke", f"--width={WIDTH}", f"--height={HEIGHT}",
                    "--no-config", "--idle-time=0", "--debug", "--fake-seat"], "weston")
        wait_for(lambda: (runtime / "rapidroom-smoke").is_socket(), "private compositor", 20)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = os.environ.copy()
        env["RAPIDROOM_NATIVE_UI_TEST_PORT"] = str(port)
        app = self.start([str(self.case / "engine/rapidroom")], "app", env)
        base = f"http://127.0.0.1:{port}"

        def driver_ready():
            if app.poll() is not None:
                raise RuntimeError("App exited during startup")
            try:
                return request(base + "/status")
            except (OSError, RuntimeError):
                return None

        wait_for(driver_ready, "embedded driver", 60)
        session = request(base + "/session", {"capabilities": {"alwaysMatch": {}}})["value"]["sessionId"]
        self.url = base + "/session/" + session
        wait_for(lambda: self.execute("return !!document.querySelector('#root')?.children.length && "
                                      "Array.isArray(window.__RAPIDROOM_SMOKE_ERRORS__);"), "real frontend")
        self.execute("""const e=[...document.querySelectorAll('button')]
          .find(e=>e.innerText.trim()==='Continue Session');
          if(!e)throw Error('Continue Session missing');e.click();return true;""")
        wait_for(lambda: self.execute("return [...document.images].some(e=>e.src.startsWith('asset:') && "
                                      "e.complete && e.naturalWidth>100);"), "real library thumbnail")
        self.capture("library")
        self.step("library thumbnail", self.snapshot("library")["images"])
        self.execute("""const e=[...document.images].find(e=>e.src.startsWith('asset:'));
          e.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0}));return true;""")
        wait_for(lambda: self.execute("return document.body.innerText.includes('4608 × 3072') && "
                                      "document.body.innerText.includes('Exposure');"), "editor opens")
        rect = self.execute("""const e=[...document.querySelectorAll('svg')]
          .filter(e=>e.getBoundingClientRect().width>500 && e.getBoundingClientRect().height>300)[0];
          if(!e)throw Error('Preview bounds missing');const r=e.getBoundingClientRect();
          return [r.left,r.top,r.right,r.bottom];""")
        if not (0 <= rect[0] < rect[2] <= WIDTH and 0 <= rect[1] < rect[3] <= HEIGHT):
            raise RuntimeError("Preview extends beyond compositor output")
        self.roi = tuple(round(v) for v in rect)
        wait_for(lambda: "[process_preview_job] full" in (self.case / "app.log").read_text(),
                 "GPU-processed editor preview")
        baseline = self.stable_preview("preview")
        self.step("native preview", {"roi": self.roi, "capture": str(baseline.relative_to(self.case))})
        edits = [self.drag("Exposure", 1), self.drag("Contrast", 20)]
        edited = self.stable_preview("edited")
        difference = ImageStat.Stat(ImageChops.difference(self.crop(baseline), self.crop(edited)))
        if max(difference.mean) < 5:
            raise RuntimeError("Slider edits did not change the native preview")
        self.step("edits change preview", {"edits": edits, "mean_absolute_difference": difference.mean})
        undo_count = 0
        while self.slider("Exposure")["value"] or self.slider("Contrast")["value"]:
            if undo_count >= 4:
                raise RuntimeError("Undo did not restore the edited sliders")
            before_undo = [self.slider(label)["value"] for label in ("Exposure", "Contrast")]
            self.execute("""const e=document.querySelector('button[data-tooltip^="Undo ("]');
              if(!e||e.disabled)throw Error('Undo unavailable');e.click();return true;""")
            wait_for(lambda: [self.slider(label)["value"] for label in ("Exposure", "Contrast")] != before_undo,
                     "Undo changes edited sliders")
            undo_count += 1
            time.sleep(0.3)
        restored = self.stable_preview("restored")
        if ImageChops.difference(self.crop(baseline), self.crop(restored)).getbbox() is not None:
            raise RuntimeError("Undo did not restore exact native preview pixels")
        self.step("Undo restores preview exactly", {"undo_clicks": undo_count})
        self.key("Comma", ",", True)
        wait_for(lambda: self.execute("return !!document.querySelector('#switch-enable-compact-sliders');"), "settings")
        self.execute("""const e=document.querySelector('#switch-enable-compact-sliders');
          e.closest('label').scrollIntoView({block:'center'});if(!e.checked)e.click();return true;""")
        self.capture("settings-compact")
        self.key("Escape", "Escape")
        wait_for(lambda: self.execute("return !!document.querySelector('input[type=range][aria-label=Exposure]');"),
                 "Compact adjustment layout")
        settings = self.case / "data/io.github.CyberTimon.RapidRAW/settings.json"
        wait_for(lambda: json.loads(settings.read_text()).get("adjustmentDensity") == "compact", "Compact persisted")
        self.capture("compact")
        self.step("Compact enabled and persisted", self.slider("Exposure"))
        self.key("KeyE", "e")
        wait_for(lambda: self.execute("return [...document.querySelectorAll('button[aria-haspopup=listbox]')]"
                                      ".some(e=>e.innerText.trim()==='Custom folder');"), "Export panel")
        self.execute("""[...document.querySelectorAll('button[aria-haspopup=listbox]')]
          .find(e=>e.innerText.trim()==='Custom folder').click();return true;""")
        wait_for(lambda: self.execute("return !!document.querySelector('[role=listbox]');"), "destination choices")
        self.execute("""const e=[...document.querySelectorAll('[role=listbox] button')]
          .find(e=>e.innerText.trim()==='Original image folder');
          if(!e)throw Error('Original folder choice missing');e.click();return true;""")
        self.capture("export-ready")
        self.execute("""const e=[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Export Image');
          if(!e||e.disabled)throw Error('Export Image unavailable');e.click();return true;""")
        files = wait_for(lambda: list((self.case / "input").rglob("*.jpg")), "JPEG export", 90)
        wait_for(lambda: self.execute("return document.body.innerText.includes('Export successful!');"),
                 "native export completion")
        if len(files) != 1:
            raise RuntimeError("Expected one exported JPEG")
        with Image.open(files[0]) as image:
            image.load()
            if image.format != "JPEG" or image.size != (4608, 3072):
                raise RuntimeError("Incorrect JPEG format or dimensions")
        self.capture("export-done")
        self.step("GUI JPEG export", {"file": str(files[0].relative_to(self.case)), "sha256": sha(files[0])})
        final = self.snapshot("final")
        if final["errors"]:
            raise RuntimeError("Frontend errors: " + str(final["errors"]))
        request(self.url + "/window", method="DELETE")
        app.wait(timeout=10)
        if app.returncode != 0:
            raise RuntimeError("App did not quit cleanly")
        native_errors = [line for line in (self.case / "app.log").read_text().splitlines()
                         if re.search(r"\[ERROR\]|CRITICAL|panicked at|Segmentation fault", line)]
        if native_errors:
            raise RuntimeError("Native errors: " + str(native_errors))
        self.step("clean exit and no frontend/native errors", {"exit_code": app.returncode})
        self.result["passed"] = True

    def close(self):
        for process in reversed(self.processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()


def launch(args):
    case = args.out.resolve()
    if "samples" not in case.parts:
        raise RuntimeError("Keep all photos and captures in a samples/ directory")
    case.mkdir(parents=True, exist_ok=False)
    raw = args.raw_dir.resolve() / FIXTURE
    corpus = json.loads((ROOT / "rapidroom/regression/corpus.json").read_text())
    expected = next(f["sha256"] for f in corpus["files"] if f["file"] == FIXTURE)
    guard = {"sha256": sha(raw), "mtime_ns": raw.stat().st_mtime_ns}
    if guard["sha256"] != expected:
        raise RuntimeError("Fixture differs from the published CC0 corpus")
    env = os.environ.copy()
    for key, folder in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"),
                        ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")):
        (case / folder).mkdir(mode=0o700)
        env[key] = str(case / folder)
    runtime = Path("/run/user") / str(os.getuid()) / ("rr-smoke-" + str(time.time_ns()))
    runtime.mkdir(mode=0o700)
    (case / "runtime-path.txt").write_text(str(runtime) + "\n")
    env["XDG_RUNTIME_DIR"] = str(runtime)
    for key in ("DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"):
        env.pop(key, None)
    env.update(WAYLAND_DISPLAY="rapidroom-smoke", GDK_BACKEND="wayland",
               XDG_CURRENT_DESKTOP="WESTON", QT_QPA_PLATFORM="wayland",
               SDL_VIDEODRIVER="wayland", GIO_USE_VFS="local", NO_AT_BRIDGE="1")
    (case / "input").mkdir()
    shutil.copy2(raw, case / "input/smoke.ARW")
    settings = case / "data/io.github.CyberTimon.RapidRAW"
    settings.mkdir()
    save(settings / "settings.json", {"rootFolders": [str(case / "input")],
         "lastRootPath": str(case / "input"), "language": "en", "useWgpuRenderer": False,
         "editorPreviewResolution": 1280, "decorations": False})
    try:
        with args.lock.open("a") as lock:
            print("Waiting for native UI lock", flush=True)
            fcntl.flock(lock, fcntl.LOCK_EX)
            (case / "engine").mkdir()
            engine = args.engine.resolve()
            source_hash = sha(engine)
            shutil.copy2(engine, case / "engine/rapidroom")
            if sha(case / "engine/rapidroom") != source_hash:
                raise RuntimeError("Engine changed while pinning")
            for name in ("resources", "lensfun_db"):
                source = engine.parent / name
                if source.is_dir():
                    (case / "engine" / name).symlink_to(source)
            save(case / "environment.json", {"engine_source": str(engine), "engine_sha256": source_hash,
                 "fixture": FIXTURE, "fixture_guard": guard, "compositor": "Weston headless GL",
                 "weston_version": subprocess.check_output(["weston", "--version"], text=True).strip(),
                 "driver": "tauri-plugin-wdio-webdriver 1.4.0", "viewport": [WIDTH, HEIGHT],
                 "harness_sha256": sha(Path(__file__).resolve())})
            with (case / "session-bus.log").open("w") as log:
                result = subprocess.run(["nice", "-n", "10", "dbus-run-session", "--", sys.executable,
                                         str(Path(__file__).resolve()), "--inside", str(case)],
                                        env=env, stdout=log, stderr=subprocess.STDOUT)
            if {"sha256": sha(raw), "mtime_ns": raw.stat().st_mtime_ns} != guard:
                raise RuntimeError("Original fixture was changed")
            if sha(engine) != source_hash:
                raise RuntimeError("Source engine changed during the locked test")
            save(case / "source-guards.json", {"fixture_unchanged": True, "engine_unchanged": True})
            print("Native UI result: " + str(case / "result.json"), flush=True)
            return result.returncode
    finally:
        for name in ("doc", "gvfs"):
            subprocess.run(["fusermount3", "-uz", str(runtime / name)], capture_output=True)
        shutil.rmtree(runtime)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", type=Path)
    parser.add_argument("--raw-dir", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--lock", type=Path, default=Path("/tmp/rapidroom-build.lock"))
    parser.add_argument("--inside", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.inside:
        case = args.inside.resolve()
        runtime = Path((case / "runtime-path.txt").read_text().strip())
        if runtime.parent != Path("/run/user") / str(os.getuid()) or not runtime.name.startswith("rr-smoke-"):
            raise RuntimeError("Unexpected private runtime path")
        if os.environ.get("XDG_RUNTIME_DIR") != str(runtime):
            raise RuntimeError("Private runtime mismatch")
        smoke = Smoke(case)
        try:
            smoke.run()
            return 0
        except Exception as error:
            smoke.result["failure"] = str(error)
            if smoke.url:
                try:
                    smoke.snapshot("failure")
                    smoke.capture("failure")
                except Exception as capture_error:
                    smoke.result["failure_capture_error"] = str(capture_error)
            print("FAIL " + str(error), flush=True)
            return 1
        finally:
            smoke.close()
            save(case / "result.json", smoke.result)
    if not all((args.engine, args.raw_dir, args.out)):
        parser.error("--engine, --raw-dir and --out are required")
    return launch(args)


if __name__ == "__main__":
    sys.exit(main())
