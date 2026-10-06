"""Read-only measurement proof against the native renderer and installed Claude."""

import base64
import hashlib
import io
import json
import math

from PIL import Image

from mcp_clients import client_command, execute_client, image_state, objects, rpc, run_client_checks
from mcp_history import read_tool


def images(url, token, name, arguments):
    value = rpc(url, "tools/call", {"name": name, "arguments": arguments}, token=token)
    if value.get("isError"):
        raise RuntimeError(name + " failed: " + str(value))
    payloads = [base64.b64decode(item["data"]) for item in value["content"] if item["type"] == "image"]
    frames = []
    for payload in payloads:
        with Image.open(io.BytesIO(payload)) as frame:
            frames.append(frame.convert("RGB"))
    return value, payloads, frames


def check_statistics(value, frame):
    pixels = list(frame.getdata())
    count = len(pixels)
    if value["pixelCount"] != count or (value["width"], value["height"]) != frame.size:
        raise RuntimeError("Statistics do not describe the rendered frame")
    linear = lambda v: v / 255 / 12.92 if v / 255 <= 0.04045 else ((v / 255 + 0.055) / 1.055) ** 2.4
    luminances = []
    luminance_histogram = [0] * 256
    luminance_sum = 0.0
    for pixel in pixels:
        luminance = 0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2])
        luminance_sum += luminance
        luminances.append(luminance)
        luminance_histogram[min(255, math.floor(luminance * 255))] += 1
    luminances.sort()
    median = lambda values: (values[(count - 1) // 2] + values[count // 2]) / 2
    if abs(value["meanLuminance"] - luminance_sum / count) > 1e-12 or abs(value["medianLuminance"] - median(luminances)) > 1e-12:
        raise RuntimeError("Linear luminance disagrees with independently decoded rendered pixels")
    if value["histogram"]["linearLuminance"] != luminance_histogram:
        raise RuntimeError("Linear luminance histogram is not exact")
    if "luminancePercentiles" in value:
        for name, fraction in (("p05", 0.05), ("p25", 0.25), ("p50", 0.5), ("p75", 0.75), ("p95", 0.95)):
            rank = fraction * (count - 1)
            lower, upper = math.floor(rank), math.ceil(rank)
            expected = luminances[lower] + (luminances[upper] - luminances[lower]) * (rank - lower)
            if abs(value["luminancePercentiles"][name] - expected) > 1e-12:
                raise RuntimeError("Luminance percentile disagrees with decoded pixels: " + name)
    for channel, name in enumerate(["red", "green", "blue"]):
        values = sorted(pixel[channel] for pixel in pixels)
        histogram = [0] * 256
        for pixel in pixels:
            histogram[pixel[channel]] += 1
        if value["histogram"][name] != histogram or value["meanRGB"][channel] != sum(values) / count or value["medianRGB"][channel] != median(values):
            raise RuntimeError("Channel statistics disagree with rendered pixels: " + name)
        clipping = value["clipping"][name]
        if clipping["shadowPixels"] != histogram[0] or clipping["highlightPixels"] != histogram[255]:
            raise RuntimeError("Incorrect channel clipping counts")
        if abs(clipping["shadowPercent"] - histogram[0] * 100 / count) > 1e-12 or abs(clipping["highlightPercent"] - histogram[255] * 100 / count) > 1e-12:
            raise RuntimeError("Incorrect channel clipping percentages")


def run_measure_checks(case, smoke):
    smoke.step("authenticated endpoint and MCP discovery", run_client_checks(case, smoke, wire_only=True))
    endpoint = json.loads((case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text())
    url, token = endpoint["url"], endpoint["token"]
    path = str(case / "input/smoke.ARW")
    # Prepare a real copy through the existing native app command, outside the read-only check.
    copy = smoke.execute("return window.__TAURI_INTERNALS__.invoke('create_virtual_copy', {sourceVirtualPath:arguments[0]});", [path])
    if not isinstance(copy, str) or not copy.startswith(path + "?vc="):
        raise RuntimeError("Native virtual-copy preparation did not return the real path")
    initial = image_state(url, path, token=token)
    history = read_tool(url, token, "history_list", {"imagePath": path})
    def sidecars():
        return {f.name: [hashlib.sha256(f.read_bytes()).hexdigest(), f.stat().st_mtime_ns]
                for f in (case / "input").glob("*.rrdata")}
    initial_sidecars = sidecars()
    common = {"imagePath": path, "expectedRevision": initial["editRevision"]}
    def unchanged():
        if image_state(url, path, token=token) != initial or read_tool(url, token, "history_list", {"imagePath": path}) != history or sidecars() != initial_sidecars:
            raise RuntimeError("Read-only measurement changed state, revision, history or sidecar bytes/mtime")

    parameters = {**common, "maxDimension": 128, "separateImages": True,
                  "variants": [{"name": "Current"}, {"name": "Twin"}]}
    _, first, frames = images(url, token, "render_compare", parameters)
    _, second, _ = images(url, token, "render_compare", parameters)
    if len(first) != 2 or first != second or first[0] != first[1] or max(frames[0].size) > 128:
        raise RuntimeError("Identical variants are not byte-stable or size-limited")
    unchanged()
    stats = read_tool(url, token, "analyze", {**common, "maxDimension": 128, "histogram": True})
    check_statistics(stats, frames[0])
    unchanged()
    sample = read_tool(url, token, "sample_region", {**common, "maxDimension": 128, "histogram": True,
                       "region": {"x":0.25,"y":0.25,"width":0.5,"height":0.5},"suggestWhiteBalance":True})
    width, height = frames[0].size
    region = frames[0].crop((math.floor(width/4),math.floor(height/4),math.ceil(width*0.75),math.ceil(height*0.75)))
    check_statistics(sample, region)
    if sample["whiteBalanceSuggestion"].get("applied", False):
        raise RuntimeError("White-balance suggestion applied an edit")
    unchanged()
    smoke.step("exact clipping/RGB/linear luminance/histograms match decoded pre-JPEG pixels", {
        "full_dimensions":list(frames[0].size),"region_dimensions":list(region.size),"pixel_count":stats["pixelCount"],
        "independent_pixel_statistics":True,"identical_variants_and_replays_byte_stable":True,
        "mean_luminance":stats["meanLuminance"],"white_balance_suggestion_only":True})

    _, small_bytes, small = images(url, token, "render_region", {**common,"region":{"x":100,"y":100,"width":128,"height":96}})
    _, _, large = images(url, token, "render_region", {**common,"region":{"x":96,"y":96,"width":256,"height":192}})
    if small[0].size != (128,96) or large[0].size != (256,192) or small[0].tobytes() != large[0].crop((4,4,132,100)).tobytes():
        raise RuntimeError("Native 1:1 nested crop coordinates disagree")
    unchanged()
    smoke.step("native crops use consistent 1:1 rendered pixel coordinates", {
        "small_dimensions":[128,96],"large_dimensions":[256,192],"nested_pixels_identical":True,
        "png_sha256":hashlib.sha256(small_bytes[0]).hexdigest()})

    comparison = {**common,"maxDimension":512,"variants":[{"name":"Current"},{"imagePath":copy}]}
    response, payloads, sheets = images(url, token, "render_compare", comparison)
    label = response["structuredContent"]["labels"]
    if label != ["Current","smoke.ARW (VC)"] or max(sheets[0].size)>512:
        raise RuntimeError("Contact-sheet labels do not match the real virtual copy")
    (case / "captures/mcp-labelled-comparison.png").write_bytes(payloads[0])
    for tool, invalid in [
        ("render_compare", {**common,"variants":[{"name":"Only one"}]}),
        ("render_compare", {**common,"variants":[{"name":"A"}]*7}),
        ("render_compare", {**common,"variants":[{"name":"A"},{"name":"Rotated","changes":{"rotation":90}}]}),
        ("render_region", {**common,"region":{"x":0,"y":0,"width":2049,"height":1}}),
        ("render_region", {**common,"region":{"x":100000,"y":0,"width":1,"height":1}}),
        ("analyze", {**common,"maxDimension":4097}),
    ]:
        value = rpc(url,"tools/call",{"name":tool,"arguments":invalid},token=token)
        if not value.get("isError"):
            raise RuntimeError("Invalid measurement limit was accepted: " + tool)
        unchanged()
    smoke.step("real virtual-copy labels, bounded contact sheet and invalid limits refused", {
        "labels":label,"dimensions":list(sheets[0].size),"png_sha256":hashlib.sha256(payloads[0]).hexdigest(),
        "invalid_variant_geometry_size_requests_refused":6,"sidecar_bytes_mtime_unchanged":True})

    folder = case / "measure-client-tests"
    folder.mkdir(mode=0o700)
    prompt = (f"Run a read-only rapidroom MCP regression on imagePath {json.dumps(path)} using only these tools. "
        "First get_image_state. Then analyze maxDimension 256. Then sample_region maxDimension 256 with "
        "region {x:0.25,y:0.25,width:0.5,height:0.5} and suggestWhiteBalance true. Then render_region with "
        "native pixel region {x:100,y:100,width:128,height:96}. Then render_compare maxDimension 512 with "
        "variants [{name:'Current'}, {name:'Brighter',changes:{exposure:0.2}}]. Await each result; "
        "do not edit, use a shell or call unrelated tools. Finish with a short confirmation.")
    tools = ["get_image_state","analyze","sample_region","render_region","render_compare"]
    command, version = client_command(case,folder,"claude",0,prompt_override=prompt,tools=tools)
    events = execute_client(command,folder,"claude",case / "config")
    calls = {value["name"].removeprefix("mcp__rapidroom__") for value in objects(events)
             if value.get("type")=="tool_use" and value.get("name","").startswith("mcp__rapidroom__")}
    if not set(tools).issubset(calls) or any(value.get("is_error") for value in objects(events) if value.get("type")=="tool_result"):
        raise RuntimeError("Actual Claude measurement calls missing or failed")
    unchanged()
    smoke.step("actual Claude read-only measurements and proposed comparison preserve state/history/files", {
        "version":version,"called":sorted(calls),"state_revision_history_and_sidecars_unchanged":True})
