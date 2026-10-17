#!/usr/bin/env python3
"""Build and pin the optional native UI test release under the shared lock."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def source(root):
    return {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD"], cwd=root)).hexdigest(),
        "status": subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True),
    }


def build_and_pin(root, manifest, binary, destination, log_prefix, env):
    command = ["nice", "-n", "10", "cargo", "build", "--manifest-path", str(manifest),
               "--release", "--locked", "--bin", binary, "--message-format=json-render-diagnostics"]
    if binary == "RapidRAW":
        command += ["--features", env["RAPIDROOM_NATIVE_BUILD_FEATURES"]]
    artifacts = []
    finished = False
    with log_prefix.with_suffix(".jsonl").open("w") as messages, log_prefix.with_suffix(".log").open("w") as log:
        process = subprocess.Popen(command, cwd=root, env=env, stdout=subprocess.PIPE,
                                   stderr=log, text=True)
        try:
            for line in process.stdout:
                messages.write(line)
                messages.flush()
                message = json.loads(line)
                if message.get("reason") == "build-finished":
                    finished = message.get("success") is True
                if (message.get("reason") == "compiler-artifact"
                        and Path(message.get("manifest_path", "")).resolve() == manifest.resolve()
                        and message.get("target", {}).get("name") == binary
                        and "bin" in message.get("target", {}).get("kind", [])
                        and not message.get("profile", {}).get("test")
                        and message.get("executable")):
                    artifacts.append(message)
            if process.wait() != 0 or not finished:
                raise RuntimeError("Cargo build failed; no executable pinned")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait()
    if len(artifacts) != 1:
        raise RuntimeError(f"Expected one compiler-artifact for {binary}, got {len(artifacts)}")
    artifact = artifacts[0]
    executable = Path(artifact["executable"])
    if not executable.is_absolute() or not executable.is_file():
        raise RuntimeError("Cargo did not report an existing absolute executable path")
    shutil.copy2(executable, destination)
    with destination.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    with executable.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != digest:
            raise RuntimeError("Executable changed while pinning under the build lock")
    receipt = {"command": command, "compiler_artifact": artifact,
               "pinned_path": str(destination), "sha256": digest,
               "copied_under_build_lock": True}
    Path(str(log_prefix) + "-artifact.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="New engine directory under samples/")
    parser.add_argument("--lock", type=Path, default=Path("/tmp/rapidroom-build.lock"))
    parser.add_argument("--terminal", action="store_true", help="Include the optional terminal for its native smoke")
    parser.add_argument("--mcp-clients", action="store_true", help="Include MCP for real-client regression checks")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    out = args.out.resolve()
    if "samples" not in out.parts:
        parser.error("Keep the pinned engine under samples/")
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "4"
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["CARGO_TARGET_DIR"] = str(Path(env.get("CARGO_TARGET_DIR", str(root / "src-tauri/target"))).resolve())
    env["CARGO_INCREMENTAL"] = "0"
    features = ["native-ui-test"] + (["terminal"] if args.terminal else []) + (["mcp"] if args.mcp_clients else [])
    with args.lock.open("a") as lock:
        print("Waiting for native UI build lock", flush=True)
        fcntl.flock(lock, fcntl.LOCK_EX)
        subprocess.run(["df", "-h", "/home"], check=True)
        if shutil.disk_usage(out).free < 25 * 1024**3:
            raise RuntimeError("Native build stopped: less than 25 GiB free")
        before = source(root)
        with (out / "frontend-build.log").open("w") as log:
            subprocess.run(["nice", "-n", "10", "npm", "run", "build"], cwd=root,
                           env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        env["RAPIDROOM_NATIVE_BUILD_FEATURES"] = ",".join(features + ["tauri/custom-protocol"])
        digest = build_and_pin(root, root / "src-tauri/Cargo.toml", "RapidRAW",
                               out / "rapidroom", out / "build", env)
        adapter_digest = None
        if args.mcp_clients:
            adapter_digest = build_and_pin(root, root / "rapidroom/mcp-client/Cargo.toml",
                                          "rapidroom-mcp-stdio", out / "rapidroom-mcp-stdio",
                                          out / "stdio-build", env)
        if source(root) != before:
            raise RuntimeError("Source changed during the build")
        for name in ("resources", "lensfun_db"):
            (out / name).symlink_to(root / "src-tauri" / name)
        (out / "build.json").write_text(json.dumps({"source": before, "engine_sha256": digest,
              "stdio_adapter_sha256": adapter_digest,
              "profile": "release", "features": features + ["tauri/custom-protocol"], "cargo_locked": True,
              "executable_provenance": "build-artifact.json; Cargo compiler-artifact from this locked build",
              "jobs": 4, "resources": "symlinked source resources; no AI operation in minimum smoke"}, indent=2) + "\n")
    print("Pinned native UI release: " + str(out / "rapidroom"), flush=True)


if __name__ == "__main__":
    main()
