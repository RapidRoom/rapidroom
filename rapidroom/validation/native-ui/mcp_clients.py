"""Real-client MCP regression, called inside the locked native smoke."""

import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import tomllib
import urllib.error
import urllib.request

from PIL import ImageChops


def rpc(url, method, params, version="2026-07-28", token=None):
    params = dict(params)
    if version >= "2026-07-28" and method != "initialize":
        params["_meta"] = {"io.modelcontextprotocol/protocolVersion": version,
                           "io.modelcontextprotocol/clientCapabilities": {}}
    headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream",
               "Mcp-Protocol-Version": version, "Mcp-Method": method}
    if token is not None:
        headers["Authorization"] = "Bearer " + token
    if method == "tools/call":
        headers["Mcp-Name"] = params["name"]
    body = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    request = urllib.request.Request(url, json.dumps(body).encode(), headers)
    try:
        with urllib.request.urlopen(request, timeout=90) as response:
            value = json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"{method} ({version}): HTTP {error.code}: {error.read().decode()}") from error
    if "error" in value:
        raise RuntimeError("MCP error: " + str(value["error"]))
    return value["result"]


def image_state(url, image, version="2026-07-28", token=None):
    result = rpc(url, "tools/call", {"name": "get_image_state", "arguments": {"imagePath": image}}, version, token)
    if version >= "2026-07-28" and result.get("resultType") != "complete":
        raise RuntimeError("Tool result lacks the required complete discriminator")
    if result.get("isError"):
        raise RuntimeError("get_image_state failed: " + str(result))
    return json.loads(next(content["text"] for content in result["content"] if content["type"] == "text"))


def execute_client(command, folder, name, config_home):
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("HCOM_") and key not in {"CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CODEX_THREAD_ID"}}
    env["XDG_CONFIG_HOME"] = str(config_home)
    output = folder / (name + ".jsonl")
    with output.open("w") as log, (folder / (name + ".stderr.log")).open("w") as errors:
        output.chmod(0o600)
        (folder / (name + ".stderr.log")).chmod(0o600)
        process = subprocess.Popen(command, cwd=folder, env=env, stdout=log, stderr=errors,
                                   start_new_session=True)
        try:
            code = process.wait(timeout=300)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise RuntimeError(name + " real client timed out")
    if code:
        raise RuntimeError(f"{name} client exited {code}; inspect its owned client-test logs")
    events = [json.loads(line) for line in output.read_text().splitlines() if line.startswith("{")]
    return events


def objects(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from objects(child)


def run_client_checks(case, smoke):
    folder = case / "client-tests"
    folder.mkdir(mode=0o700)
    endpoint = case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json"
    data = json.loads(endpoint.read_text())
    url, token = data["url"], data["token"]
    if stat.S_IMODE(endpoint.stat().st_mode) != 0o600:
        raise RuntimeError("Endpoint is not mode 0600")
    for rejected_token in (None, "invalid"):
        body = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}
        headers = {"Content-Type": "application/json"}
        if rejected_token:
            headers["Authorization"] = "Bearer " + rejected_token
        request = urllib.request.Request(url, json.dumps(body).encode(), headers)
        try:
            urllib.request.urlopen(request, timeout=10).close()
            raise RuntimeError("Unauthenticated request was accepted")
        except urllib.error.HTTPError as error:
            if error.code != 401:
                raise RuntimeError("Unexpected authentication rejection status") from error
    # Origin protection must still reject authenticated browser requests.
    request = urllib.request.Request(url, json.dumps({"jsonrpc": "2.0", "id": 1,
        "method": "initialize", "params": {}}).encode(),
        {"Content-Type": "application/json", "Authorization": "Bearer " + token, "Origin": "https://example.com"})
    try:
        urllib.request.urlopen(request, timeout=10).close()
        raise RuntimeError("Authenticated browser request was accepted")
    except urllib.error.HTTPError as error:
        if error.code != 403:
            raise RuntimeError("Unexpected Origin rejection status") from error
    adapter = str(case / "engine/rapidroom-mcp-stdio")
    image = str(case / "input/smoke.ARW")
    results = {"wire_protocols": {}, "clients": {}, "unauthenticated_and_wrong_token_refused": True, "endpoint_mode": "0600", "authenticated_origin_refused": True, "transport": "stdio with automatic per-user endpoint discovery"}
    for version in ("2025-06-18", "2026-07-28"):
        initialized = rpc(url, "initialize", {"protocolVersion": version, "capabilities": {},
                          "clientInfo": {"name": "RapidRoom regression", "version": "1"}}, version, token)
        if initialized["protocolVersion"] != version:
            raise RuntimeError("Unexpected negotiated version: " + str(initialized))
        listed = rpc(url, "tools/list", {}, version, token)
        if listed.get("ttlMs") != 0 or listed.get("cacheScope") != "private":
            raise RuntimeError("Missing required tools/list cache hints")
        if not any(tool["name"] == "update_adjustments" for tool in listed["tools"]):
            raise RuntimeError("Editing tool missing from discovery")
        results["wire_protocols"][version] = {"tools": len(listed["tools"]), "ttlMs": 0, "cacheScope": "private"}
        if image_state(url, image, version, token)["imagePath"] != image:
            raise RuntimeError("Wire state read returned another image")
        results["wire_protocols"][version]["state_read"] = True
    config = folder / "claude-mcp.json"
    config.write_text(json.dumps({"mcpServers": {"rapidroom": {"command": adapter}}}))
    user_config = Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex"))) / "config.toml"
    settings = tomllib.loads(user_config.read_text()) if user_config.exists() else {}
    allowed = ["get_image_state", "update_adjustments"]
    for name, exposure in (("claude", 0.5), ("codex", 1.0)):
        binary = os.environ["RAPIDROOM_TEST_" + name.upper() + "_BIN"]
        before = image_state(url, image, token=token)
        before_pixels = smoke.crop(smoke.stable_preview(name + "-before"))
        prompt = ("Perform this regression check using only the rapidroom MCP tools. "
                  f"Call get_image_state with imagePath {json.dumps(image)}, then call update_adjustments "
                  f"with that exact imagePath, changes {{\"exposure\": {exposure}}}, and expectedRevision "
                  "from the state you just read. Do not change any other field. Do not use shell, files, "
                  "other servers, or other tools. Finish after the successful edit with a short confirmation.")
        if name == "claude":
            command = [binary, "-p", "--strict-mcp-config", "--mcp-config", str(config),
                       "--tools", "", "--allowedTools", ",".join("mcp__rapidroom__" + tool for tool in allowed),
                       "--setting-sources", "", "--settings", '{"disableAllHooks":true}',
                       "--disable-slash-commands", "--no-session-persistence", "--max-turns", "6",
                       "--output-format", "stream-json", "--verbose", "--debug-file", str(folder / "claude-debug.log"),
                       "--system-prompt", "You are an MCP interoperability regression client. Follow the requested tool calls exactly.", prompt]
        else:
            command = [binary, "exec", "--ignore-user-config", "--ignore-rules", "--ephemeral",
                       "--skip-git-repo-check", "--sandbox", "read-only", "--json", "--color", "never",
                       "-c", 'approval_policy="never"', "-c", "mcp_servers.rapidroom.command=" + json.dumps(adapter),
                       "-c", 'mcp_servers.rapidroom.env_vars=["XDG_CONFIG_HOME"]',
                       "-c", "mcp_servers.rapidroom.enabled_tools=" + json.dumps(allowed),
                       "-c", 'mcp_servers.rapidroom.required=true',
                       "-c", 'mcp_servers.rapidroom.default_tools_approval_mode="auto"']
            for tool in allowed:
                command += ["-c", f'mcp_servers.rapidroom.tools.{tool}.approval_mode="approve"']
            if settings.get("model"):
                command += ["--model", settings["model"]]
            command += [prompt]
        version = subprocess.check_output([binary, "--version"], text=True).strip()
        events = execute_client(command, folder, name, case / "config")
        if name == "claude":
            called = any(value.get("type") == "tool_use" and value.get("name") == "mcp__rapidroom__update_adjustments"
                         for value in objects(events))
        else:
            called = any(value.get("type") == "mcp_tool_call" and value.get("server") == "rapidroom"
                         and value.get("tool") == "update_adjustments" and value.get("status") == "completed"
                         for value in objects(events))
        if not called:
            raise RuntimeError(name + " did not record a real MCP editing tool call")
        after = image_state(url, image, token=token)
        after_pixels = smoke.crop(smoke.stable_preview(name + "-after"))
        if after["adjustments"]["exposure"] != exposure or before["editRevision"] == after["editRevision"]:
            raise RuntimeError(name + " did not change the active photo exactly")
        if smoke.slider("Exposure")["value"] != exposure or ImageChops.difference(before_pixels, after_pixels).getbbox() is None:
            raise RuntimeError(name + " edit did not update the native UI/preview")
        results["clients"][name] = {"version": version, "actual_mcp_edit_call": True, "exposure": exposure,
                                    "revision_changed": True, "native_slider_and_preview_changed": True}
        (folder / "result.json").write_text(json.dumps(results, indent=2) + "\n")
    return results
