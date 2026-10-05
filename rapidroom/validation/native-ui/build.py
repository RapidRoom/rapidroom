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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="New engine directory under samples/")
    parser.add_argument("--lock", type=Path, default=Path("/tmp/rapidroom-build.lock"))
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
    target = Path(env.get("CARGO_TARGET_DIR", str(root / "src-tauri/target"))).resolve()
    env["CARGO_TARGET_DIR"] = str(target)
    with args.lock.open("a") as lock:
        print("Waiting for native UI build lock", flush=True)
        fcntl.flock(lock, fcntl.LOCK_EX)
        before = source(root)
        with (out / "build.log").open("w") as log:
            subprocess.run(["nice", "-n", "10", "npm", "run", "tauri", "build", "--", "--no-bundle",
                            "--features", "native-ui-test,mcp" if args.mcp_clients else "native-ui-test", "--", "--locked"],
                           cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        if source(root) != before:
            raise RuntimeError("Source changed during the build")
        shutil.copy2(target / "release/rapidroom", out / "rapidroom")
        for name in ("resources", "lensfun_db"):
            (out / name).symlink_to(root / "src-tauri" / name)
        with (out / "rapidroom").open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        (out / "build.json").write_text(json.dumps({"source": before, "engine_sha256": digest,
              "profile": "release", "features": ["native-ui-test"] + (["mcp"] if args.mcp_clients else []), "cargo_locked": True,
              "jobs": 4, "resources": "symlinked source resources; no AI operation in minimum smoke"}, indent=2) + "\n")
    print("Pinned native UI release: " + str(out / "rapidroom"), flush=True)


if __name__ == "__main__":
    main()
