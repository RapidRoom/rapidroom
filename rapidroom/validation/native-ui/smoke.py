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
import shlex
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request

from PIL import Image, ImageChops, ImageStat

sys.dont_write_bytecode = True


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
        self.execute("""document.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,button:0}));return true;""")
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

    def terminal_type(self, text):
        # Exercise xterm's ordinary keyboard listeners, then the real Tauri PTY.
        self.execute("""const e=document.querySelector('[data-terminal-panel] textarea');
          if(!e)throw Error('Terminal input missing');e.focus();
          for(const c of arguments[0]) {
            if(c==='\\r') {
              e.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,cancelable:true,
                key:'Enter',code:'Enter',keyCode:13,which:13}));
              e.dispatchEvent(new KeyboardEvent('keyup',{bubbles:true,key:'Enter',code:'Enter',keyCode:13}));
            } else {
              e.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,cancelable:true,key:c,keyCode:0}));
              e.dispatchEvent(new KeyboardEvent('keypress',{bubbles:true,cancelable:true,
                key:c,charCode:c.charCodeAt(0),keyCode:c.charCodeAt(0),which:c.charCodeAt(0)}));
              e.dispatchEvent(new KeyboardEvent('keyup',{bubbles:true,key:c,keyCode:0}));
            }
          }return true;""", [text])

    def terminal_text(self):
        return self.execute("return document.querySelector('[data-terminal-panel] .xterm-rows')?.textContent || ''; ")

    def terminal_drag(self, region):
        points = self.execute("""const a=document.querySelector('[data-tab-id=layout-tab-terminal]'),
          b=document.querySelector('[data-layout-region="'+arguments[0]+'"]');
          if(!a||!b)throw Error('Dock target missing');const r=a.getBoundingClientRect(),s=b.getBoundingClientRect();
          // The empty split overlay depends on the existing switcher's placement.
          const switcher=b.querySelector('[data-layout-tab]')?.parentElement;
          const tabsAtBottom=switcher?.classList.contains('border-t');
          const y=arguments[0].endsWith('Top') ? s.top+s.height*(tabsAtBottom?0.75:0.25) : s.top+s.height/2;
          return [r.left+r.width/2,r.top+r.height/2,s.left+s.width/2,y];""", [region])
        self.execute("""const e=document.querySelector('[data-tab-id=layout-tab-terminal]');
          e.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,cancelable:true,
            pointerId:1,pointerType:'mouse',isPrimary:true,button:0,buttons:1,
            clientX:arguments[0],clientY:arguments[1]}));return true;""", points[:2])
        time.sleep(0.2)
        for x, y in [(points[0]+10, points[1]+10), (points[2], points[3])]:
            self.execute("""document.dispatchEvent(new PointerEvent('pointermove',
              {bubbles:true,pointerId:1,pointerType:'mouse',isPrimary:true,buttons:1,
               clientX:arguments[0],clientY:arguments[1]}));return true;""", [x, y])
            time.sleep(0.3)
        self.execute("""document.dispatchEvent(new PointerEvent('pointerup',
          {bubbles:true,pointerId:1,pointerType:'mouse',isPrimary:true,button:0,
           clientX:arguments[0],clientY:arguments[1]}));return true;""", points[2:])
        wait_for(lambda: self.execute("return document.querySelector('[data-terminal-panel]')"
          "?.closest('[data-layout-region]')?.dataset.layoutRegion === arguments[0];", [region]),
                 "terminal docks in " + region)

    def terminal_flow(self):
        policy = self.execute("return [...document.querySelectorAll('meta[http-equiv]')]"
          ".filter(e=>e.httpEquiv.toLowerCase()==='content-security-policy').map(e=>e.content);")
        # Tauri's custom protocol supplies CSP as a response header on WebKitGTK.
        headers = request(self.url + "/execute/async", {"script": """
          const done=arguments[arguments.length-1];
          fetch(location.href).then(r=>done({policy:r.headers.get('content-security-policy')}))
            .catch(e=>done({error:String(e)}));""", "args": []})["value"]
        if headers.get("policy"):
            policy.append(headers["policy"])
        if not policy or any("unsafe-eval" in value or "clerk" in value.lower() for value in policy):
            raise RuntimeError("Native strict CSP missing or broadened: " + str(headers))
        control = request(self.url + "/execute/async", {"script": """
          const callback=arguments[arguments.length-1], script=document.createElement('script');
          let violation=null,finished=false;
          const listener=e=>{if(e.blockedURI==='data' && e.effectiveDirective.startsWith('script-src'))
            violation={directive:e.effectiveDirective,policy:e.originalPolicy};};
          document.addEventListener('securitypolicyviolation',listener);
          const done=()=>{if(finished)return;finished=true;script.remove();
            document.removeEventListener('securitypolicyviolation',listener);
            callback({executed:window.__RR_CSP_CONTROL__===true,violation});};
          script.src='data:text/javascript,window.__RR_CSP_CONTROL__=true';
          script.onload=done;script.onerror=()=>setTimeout(done,100);
          document.body.append(script);setTimeout(done,2000);""", "args": []})["value"]
        if control.get("executed") or not control.get("violation"):
            raise RuntimeError("Native CSP negative control failed: " + str(control))
        self.step("native strict CSP", {"policy": policy, "nonlocal_script_control": control})
        self.execute("""document.querySelector('[data-tab-id=layout-tab-terminal]').click();return true;""")
        wait_for(lambda: self.execute("return !!document.querySelector('[data-terminal-open]');"), "terminal panel")
        self.execute("""document.querySelector('[data-terminal-open]').click();return true;""")
        wait_for(lambda: self.execute("return !!document.querySelector('[data-terminal-panel] textarea');"), "xterm input")
        env_file = self.case / "tools/pty-env.txt"
        self.terminal_type("printf '%s_%s\\n' RR_NATIVE PTY; printf '%s|%s' \"$PWD\" \"$RAPIDROOM_FOLDER\" > " + shlex.quote(str(env_file)) + "\r")
        wait_for(lambda: env_file.exists(), "real PTY command executes")
        expected = str(self.case / "input")
        if env_file.read_text() != expected + "|" + expected:
            raise RuntimeError("PTY folder/environment mismatch")
        wait_for(lambda: "RR_NATIVE_PTY" in self.terminal_text(), "real PTY output")
        count = self.terminal_text().count("RR_NATIVE_PTY")
        self.capture("terminal-bottom")
        self.terminal_drag("leftTop")
        if self.terminal_text().count("RR_NATIVE_PTY") != count:
            raise RuntimeError("Docking replayed or lost terminal output")
        self.capture("terminal-left")
        self.terminal_drag("bottom")
        height = self.execute("""const h=document.querySelector('[data-bottom-dock]').getBoundingClientRect().height,
          e=document.querySelector('[aria-label="Resize bottom panel"]');
          e.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,key:'ArrowUp'}));return h;""")
        wait_for(lambda: self.execute("return document.querySelector('[data-bottom-dock]').getBoundingClientRect().height;") >= height + 19,
                 "bottom dock resizes")
        self.execute("""document.querySelector('[aria-label="Collapse bottom panel"]').click();return true;""")
        wait_for(lambda: self.execute("return !document.querySelector('[data-terminal-panel]');"), "dock collapsed")
        self.execute("""document.querySelector('[data-tab-id=layout-tab-terminal]').click();return true;""")
        wait_for(lambda: self.execute("return !!document.querySelector('[data-terminal-panel] textarea');"), "same tab reopened")
        if self.terminal_text().count("RR_NATIVE_PTY") != count:
            raise RuntimeError("Panel reopen replayed or lost output")
        self.step("PTY input/output, docking, resize and collapse", {"folder": expected, "marker_occurrences": count})
        if json.loads((self.case / "terminal-test.json").read_text())["real_assistants"]:
            from terminal_clients import run_launch_checks
            run_launch_checks(self.case, self)
        else:
            records = self.case / "tools/agents.txt"
            for agent in ("claude", "codex"):
                self.execute("""document.querySelector('[data-start-agent="'+arguments[0]+'"]').click();return true;""", [agent])
                wait_for(lambda: records.exists() and agent in records.read_text(), "built-in stub " + agent)
            self.execute("""document.querySelector('[aria-label="Collapse bottom panel"]').click();return true;""")
            self.execute("""const e=document.querySelector('[data-agent-launcher] select');
              e.value='external';e.dispatchEvent(new Event('change',{bubbles:true}));return true;""")
            for agent in ("claude", "codex"):
                before = records.read_text().count(agent + "|")
                self.execute("""document.querySelector('[data-start-agent="'+arguments[0]+'"]').click();return true;""", [agent])
                wait_for(lambda: records.read_text().count(agent + "|") > before, "external stub " + agent)
                if self.execute("return !!document.querySelector('[data-terminal-panel]');"):
                    raise RuntimeError("External start opened the built-in panel")
            lines = records.read_text().splitlines()
            if len(lines) != 4 or any(line.split('|')[1:3] != [expected, expected] for line in lines):
                raise RuntimeError("Built-in/external stub launch environment differs")
            self.step("built-in and external shell-typed assistant launch", {"records": lines, "real_assistants_started": False, "mcp_parity": "pending #116 integration"})
        # Leave a real background job in a PTY, then verify app quit removes it.
        self.execute("""document.querySelector('[data-tab-id=layout-tab-terminal]').click();return true;""")
        wait_for(lambda: self.execute("""return !!document.querySelector('[aria-label="New terminal tab"]');"""), "terminal header")
        self.execute("""document.querySelector('[aria-label="New terminal tab"]').click();return true;""")
        wait_for(lambda: self.execute("return !!document.querySelector('[data-terminal-panel] textarea');"), "fresh shell")
        pid_file = self.case / "tools/background-pid.txt"
        self.terminal_type("sleep 120 & printf '%s' \"$!\" > " + shlex.quote(str(pid_file)) + "\r")
        wait_for(lambda: pid_file.exists() and pid_file.read_text().strip(), "owned background process")
        self.background_pid = int(pid_file.read_text())
        self.capture("terminal-final")

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
        startup = {"phase": "read-only frontend readiness", "script_timeouts_retried": 0}
        save(self.case / "driver-startup.json", startup)

        def frontend_ready():
            try:
                return self.execute("return !!document.querySelector('#root')?.children.length && "
                                    "Array.isArray(window.__RAPIDROOM_SMOKE_ERRORS__);")
            except RuntimeError as error:
                # The driver can advertise readiness before its first script
                # callback is attached. Only this side-effect-free probe retries.
                if startup["script_timeouts_retried"] or not re.search(
                    r"['\"]error['\"]\s*:\s*['\"]script timeout['\"]", str(error)
                ):
                    raise
                startup.update(script_timeouts_retried=1, first_probe_error=str(error))
                save(self.case / "driver-startup.json", startup)
                return False

        wait_for(frontend_ready, "real frontend", 90)
        startup["phase"] = "frontend ready before Continue Session"
        save(self.case / "driver-startup.json", startup)
        self.execute("""const e=[...document.querySelectorAll('button')]
          .find(e=>e.innerText.trim()==='Continue Session');
          if(!e)throw Error('Continue Session missing');e.click();return true;""")
        wait_for(lambda: self.execute("return [...document.images].some(e=>e.src.startsWith('asset:') && "
                                      "e.complete && e.naturalWidth>100);"), "real library thumbnail")
        self.capture("library")
        self.step("library thumbnail", self.snapshot("library")["images"])
        if (self.case / "dock-layout-test.json").exists():
            from dock_layout import run_layout_checks
            run_layout_checks(self.case, self)
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
        if (self.case / "point-color-test.json").exists():
            from point_color import run_point_color_checks
            run_point_color_checks(self, wait_for, baseline)
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
        if (self.case / "crop-noop-test.json").exists():
            from crop_noop import run_crop_checks
            run_crop_checks(self.case, self, restored)
        self.key("Comma", ",", True)
        wait_for(lambda: self.execute("return !!document.querySelector('#switch-enable-compact-sliders');"), "settings")
        self.execute("""const e=document.querySelector('#switch-enable-compact-sliders');
          e.closest('label').scrollIntoView({block:'center'});if(!e.checked)e.click();return true;""")
        self.capture("settings-compact")
        self.key("Escape", "Escape")
        wait_for(lambda: self.execute("return !document.querySelector('#switch-enable-compact-sliders');"),
                 "settings overlay closes")
        wait_for(lambda: self.execute("return !!document.querySelector('input[type=range][aria-label=Exposure]');"),
                 "Compact adjustment layout")
        settings = self.case / "data/io.github.CyberTimon.RapidRAW/settings.json"
        wait_for(lambda: json.loads(settings.read_text()).get("adjustmentDensity") == "compact", "Compact persisted")
        self.stable_preview("compact")
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
        if (self.case / "mcp-packaging-test.json").exists():
            from mcp_packaging import run_packaging_checks
            run_packaging_checks(self.case, self)
        elif (self.case / "terminal-test.json").exists():
            self.terminal_flow()
        if (self.case / "mcp-compact-test.json").exists():
            from mcp_compact import run_compact_checks
            run_compact_checks(self.case, self)
        elif (self.case / "mcp-revisions-test.json").exists():
            from mcp_revisions import run_revision_checks
            run_revision_checks(self.case, self)
        elif (self.case / "mcp-measure-test.json").exists():
            from mcp_measure import run_measure_checks
            run_measure_checks(self.case, self)
        elif (self.case / "mcp-history-test.json").exists():
            from mcp_history import run_history_checks
            run_history_checks(self.case, self)
        elif (self.case / "mcp-clients-test.json").exists() and not (self.case / "terminal-test.json").exists():
            from mcp_clients import run_client_checks
            self.step("real Claude Code and Codex edit the open photo", run_client_checks(self.case, self))
        final = self.snapshot("final")
        if final["errors"]:
            raise RuntimeError("Frontend errors: " + str(final["errors"]))
        request(self.url + "/window", method="DELETE")
        app.wait(timeout=10)
        if app.returncode != 0:
            raise RuntimeError("App did not quit cleanly")
        if (self.case / "mcp-packaging-test.json").exists():
            if (self.case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").exists():
                raise RuntimeError("Endpoint file survived clean app exit")
            self.step("endpoint removed on app exit", {"endpoint_exists": False})
        if hasattr(self, "background_pid"):
            def background_gone():
                path = Path(f"/proc/{self.background_pid}/stat")
                return not path.exists() or path.read_text().rsplit(") ", 1)[1].startswith("Z")
            wait_for(background_gone, "PTY background process cleanup", 10)
            self.step("PTY background process removed on app quit", {"pid": self.background_pid})
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
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    if args.mcp_clients and not args.crop_noop:
        # Resolve version-manager wrappers before isolating the app's XDG paths.
        for name in ("claude", "codex"):
            binary = shutil.which(name)
            if shutil.which("mise"):
                resolved = subprocess.run(["mise", "which", name], capture_output=True, text=True)
                if resolved.returncode == 0:
                    binary = resolved.stdout.strip()
            if not binary or not Path(binary).is_file() or not os.access(binary, os.X_OK):
                raise RuntimeError("Installed client unavailable: " + name)
            env["RAPIDROOM_TEST_" + name.upper() + "_BIN"] = str(Path(binary).resolve())
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
    if args.point_color:
        save(case / "point-color-test.json", {"issue": 160})
    if args.dock_layout:
        save(case / "dock-layout-test.json", {"issues": [145, 146], "viewport": [WIDTH, HEIGHT]})
        for index in range(1, 24):
            shutil.copy2(raw, case / f"input/smoke-{index:02d}.ARW")
    if args.mcp_measure:
        save(case / "mcp-measure-test.json", {"issue": 117, "real_model_requests": True})
    if args.mcp_revisions:
        save(case / "mcp-revisions-test.json", {"issue": 148, "real_model_requests": True})
    if args.mcp_compact or args.mcp_compact_baseline:
        save(case / "mcp-compact-test.json", {"issue": 147, "baseline": args.mcp_compact_baseline, "real_model_requests": not args.mcp_compact_baseline})
    if args.mcp_history:
        save(case / "mcp-history-test.json", {"issue": 115, "real_model_requests": True})
    if args.mcp_packaging:
        save(case / "mcp-packaging-test.json", {"issue": 135, "control_default": False})
    if args.crop_noop:
        save(case / "crop-noop-test.json", {"issue": 149, "real_model_requests": False})
    if args.mcp_clients:
        if not args.crop_noop and not args.mcp_compact_baseline:
            save(case / "mcp-clients-test.json", {"clients": ["claude", "codex"], "real_model_requests": True})
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            env["RAPIDRAW_MCP_PORT"] = str(sock.getsockname()[1])
    settings = case / "data/io.github.CyberTimon.RapidRAW"
    settings.mkdir()
    save(settings / "settings.json", {"rootFolders": [str(case / "input")],
         "lastRootPath": str(case / "input"), "language": "en", "useWgpuRenderer": False,
         "editorPreviewResolution": 1280, "decorations": False,
         "mcpEnabled": args.mcp_clients and not args.mcp_packaging})
    if args.mcp_packaging:
        initial = json.loads((settings / "settings.json").read_text())
        initial.pop("mcpEnabled")
        save(settings / "settings.json", initial)
    if args.terminal:
        tools = case / "tools"
        tools.mkdir()
        shell = tools / "test-shell"
        shell.write_text("#!/bin/sh\nunset ENV BASH_ENV\nHISTFILE=" + shlex.quote(str(tools / "shell-history")) + "\nPATH=" + shlex.quote(str(tools)) + ":$PATH\nexport PATH HISTFILE\nexec /bin/sh -i\n")
        shell.chmod(0o700)
        for agent in ("claude", "codex"):
            stub = tools / agent
            if args.terminal_clients:
                stub.write_text("#!/bin/sh\nexec " + shlex.quote(sys.executable) + " " +
                    shlex.quote(str(ROOT / "rapidroom/validation/native-ui/terminal_clients.py")) + " " +
                    shlex.quote(str(case)) + " " + shlex.quote(agent) + ' "$@"\n')
            else:
                stub.write_text("#!/bin/sh\nprintf '%s|%s|%s|%s\\n' " + shlex.quote(agent) + " \"$PWD\" \"$RAPIDROOM_FOLDER\" \"${RAPIDROOM_MCP_ENDPOINT:-}\" >> " + shlex.quote(str(tools / "agents.txt")) + "\nprintf 'RR_STUB_" + agent + "\\n'\n")
            stub.chmod(0o700)
        terminal = tools / "ghostty"
        terminal.write_text("#!/bin/sh\nwhile [ $# -gt 0 ]; do\ncase \"$1\" in\n--working-directory=*) cd \"${1#*=}\" || exit 1; shift;;\n-e) shift; break;;\n*) shift;;\nesac\ndone\nPATH=" + shlex.quote(str(tools)) + ":$PATH\nexport PATH\nexec \"$@\"\n")
        terminal.chmod(0o700)
        env["TERMINAL"] = str(terminal)
        # External mock exits after the fixed CLI, without opening a desktop window.
        exit_shell = tools / "exit-shell"
        exit_shell.write_text("#!/bin/sh\nexit 0\n")
        exit_shell.chmod(0o700)
        env["SHELL"] = str(exit_shell)
        data = json.loads((settings / "settings.json").read_text())
        data["terminalSettings"] = {"shell": str(shell), "startIn": "built-in"}
        save(settings / "settings.json", data)
        save(case / "terminal-test.json", {"real_assistants": args.terminal_clients, "terminal_launcher": "owned executable stub"})
    try:
        with args.lock.open("a") as lock:
            print("Waiting for native UI lock", flush=True)
            fcntl.flock(lock, fcntl.LOCK_EX)
            (case / "engine").mkdir()
            engine = args.engine.resolve()
            source_hash = sha(engine)
            shutil.copy2(engine, case / "engine/rapidroom")
            if args.mcp_clients:
                adapter = engine.parent / "rapidroom-mcp-stdio"
                if not adapter.is_file():
                    raise RuntimeError("Authenticated real-client test requires the pinned stdio adapter")
                shutil.copy2(adapter, case / "engine/rapidroom-mcp-stdio")
                adapter_hash = sha(adapter)
                if sha(case / "engine/rapidroom-mcp-stdio") != adapter_hash:
                    raise RuntimeError("Pinned stdio adapter copy differs")
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
            if args.mcp_clients and (sha(adapter) != adapter_hash or sha(case / "engine/rapidroom-mcp-stdio") != adapter_hash):
                raise RuntimeError("Source stdio adapter changed during the locked test")
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
    parser.add_argument("--dock-layout", action="store_true", help="Check stepped dock resize overlap and compact terminal controls (implies --terminal)")
    parser.add_argument("--terminal", action="store_true", help="Exercise terminal docking/PTY and owned CLI/terminal stubs")
    parser.add_argument("--mcp-clients", action="store_true", help="Run installed real Claude Code and Codex clients")
    parser.add_argument("--terminal-clients", action="store_true", help="Use real clients from both terminal launch paths (implies --terminal --mcp-clients)")
    parser.add_argument("--mcp-history", action="store_true", help="Run actual-client labelled history/schema/preview scenario (implies --mcp-clients)")
    parser.add_argument("--mcp-packaging", action="store_true", help="Verify default-off, launch consent, runtime toggle and real registered clients")
    parser.add_argument("--mcp-measure", action="store_true", help="Run read-only measurement/comparison scenario (implies --mcp-clients)")
    parser.add_argument("--mcp-revisions", action="store_true", help="Real clients read latest state across a GUI edit and receive strict stale-write details")
    parser.add_argument("--mcp-compact", action="store_true", help="Measure three-mask compact payloads and verify real clients with local schemas")
    parser.add_argument("--mcp-compact-baseline", action="store_true", help="Capture old three-mask payload bytes without real model requests")
    parser.add_argument("--point-color", action="store_true", help="Pick a Point Color and verify zero-shift pixels, shifts and Undo")
    parser.add_argument("--crop-noop", action="store_true", help="Check native crop history/revision/sidecars with private MCP reads; no model requests")
    parser.add_argument("--inside", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    global WIDTH, HEIGHT
    if args.dock_layout:
        args.terminal = True
        WIDTH, HEIGHT = 1800, 1048
    if args.mcp_history or args.mcp_measure or args.mcp_revisions or args.mcp_compact or args.mcp_compact_baseline or args.crop_noop:
        args.mcp_clients = True
    if args.mcp_packaging:
        args.terminal_clients = True
    if args.terminal_clients:
        args.terminal = args.mcp_clients = True
    if args.inside:
        case = args.inside.resolve()
        if (case / "dock-layout-test.json").exists():
            WIDTH, HEIGHT = json.loads((case / "dock-layout-test.json").read_text())["viewport"]
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
