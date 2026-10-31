"""Compare real Culling/ordinary IPC outputs across decoded-cache compaction (#406/#474)."""

import base64
import hashlib

from PIL import Image

from smoke import request, save, wait_for


def prepare(case):
    save(case / "input/smoke.ARW.rrdata", {"adjustments": {
        "lensCorrectionMode": "auto", "cameraLensCorrectionVersion": 1,
        "lensMaker": "Sony", "lensModel": "FE 24-70mm f/4 ZA OSS (crop 1.0x)",
    }})
    # Four tiny sources push the RAW beyond the three-photo f32 window while
    # keeping it inside the five-entry cache. Never modify the CC0 original.
    for index in range(4):
        Image.new("RGB", (32, 24), (30 + index * 20, 80, 120)).save(
            case / "input" / f"precision-other-{index}.jpg"
        )
    save(case / "culling-precision-test.json", {"other_photos": 4})


def run_culling_precision(smoke):
    def invoke(command, payload, binary=False):
        script = """const done=arguments[arguments.length-1];
          window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(async value=>{
            if(!arguments[2])return value;
            return await new Promise((resolve,reject)=>{
              const reader=new FileReader();
              reader.onload=()=>resolve(reader.result.split(',')[1]);
              reader.onerror=reject;
              reader.readAsDataURL(new Blob([new Uint8Array(value)]));
            });
          }).then(value=>done({value})).catch(e=>done({error:String(e)}));"""
        result = request(
            smoke.url + "/execute/async",
            {"script": script, "args": [command, payload, binary]},
        )["value"]
        if "error" in result:
            raise RuntimeError(result["error"])
        return base64.b64decode(result["value"]) if binary else result["value"]

    source = str(smoke.case / "input/smoke.ARW")
    metadata = invoke("load_metadata", {"path": source})
    adjustments = metadata.get("adjustments") or {}
    if adjustments.get("lensCorrectionMode") != "auto" or not adjustments.get("lensDistortionParams"):
        raise RuntimeError("Culling regression requires the saved automatic lens payload")
    generation = invoke("begin_culling_preview_batch", {})
    rows = []

    def render(stage, command):
        payload = {"path": source, "jsAdjustments": adjustments}
        if command == "generate_culling_preview_for_path":
            payload["generation"] = generation
        data = invoke(command, payload, binary=True)
        path = smoke.case / (stage + ".jpg")
        path.write_bytes(data)
        with Image.open(path) as image:
            rgb = image.convert("RGB")
            row = {
                "stage": stage,
                "command": command,
                "jpeg_sha256": hashlib.sha256(data).hexdigest(),
                "pixel_sha256": hashlib.sha256(rgb.tobytes()).hexdigest(),
                "dimensions": list(rgb.size),
            }
        rows.append(row)
        return row

    first = render("first-ordinary", "generate_preview_for_path")
    render("first-culling", "generate_culling_preview_for_path")
    for index in range(4):
        invoke("load_image", {"path": str(smoke.case / "input" / f"precision-other-{index}.jpg")})
    render("return-culling", "generate_culling_preview_for_path")
    render("return-ordinary", "generate_preview_for_path")
    render("repeat-culling", "generate_culling_preview_for_path")
    checks = {
        row["stage"]: all(row[key] == first[key] for key in ("jpeg_sha256", "pixel_sha256", "dimensions"))
        for row in rows
    }
    save(smoke.case / "culling-precision.json", {
        "adjustments": adjustments, "rows": rows, "exact": checks,
        "other_photo_count": 4,
    })
    if not all(checks.values()):
        raise RuntimeError("Culling differs from the original full-precision preview: " + str(checks))
    # Reselect through the real UI so its load generation follows the backend.
    for name in ("precision-other-0.jpg", "smoke.ARW"):
        smoke.execute("""const e=[...document.querySelectorAll('[data-image-path]')]
          .find(e=>e.getAttribute('data-image-path')?.endsWith('/'+arguments[0])
            &&e.getBoundingClientRect().width>0);
          if(!e)throw Error('Missing precision thumbnail');
          e.dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));return true;""", [name])
        wait_for(lambda: smoke.execute("""return [...document.querySelectorAll('[data-editor-info-line] [title]')]
          .some(e=>e.title.startsWith(arguments[0]+' - '));""", [name]), "restored " + name)
    smoke.stable_preview("culling-precision-restored")
    smoke.step("ordinary and Culling previews remain byte-exact after compaction", checks)
