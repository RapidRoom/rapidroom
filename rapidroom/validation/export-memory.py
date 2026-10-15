#!/usr/bin/env python3
"""Measure legacy/bounded production-shader 60MP tests on Linux DRM.
Pass the lib-test executable emitted by cargo test --lib --no-run. Each run
owns the shared GPU lock. Only its DRM clients are sampled, deduplicated by
client ID. Resident/shared metrics are never double-counted as total memory.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

BUDGET = 1536 * 1024 * 1024
UNITS = {"B": 1, "KiB": 1024, "MiB": 1024**2, "GiB": 1024**3}


def drm_memory(pid):
    clients = {}
    for fd in Path(f"/proc/{pid}/fdinfo").glob("*"):
        try:
            data = fd.read_text()
        except (FileNotFoundError, PermissionError):
            continue
        client = re.search(r"^drm-client-id:\s*(\S+)", data, re.M)
        if not client:
            continue
        metrics = {}
        for name, value, unit in re.findall(r"^(drm-(?:total|resident|memory)-\S+):\s*(\d+)\s+(\S+)", data, re.M):
            if unit in UNITS:
                metrics[name] = int(value) * UNITS[unit]
        clients[client.group(1)] = metrics
    totals = {}
    for metrics in clients.values():
        for key, value in metrics.items():
            totals[key] = totals.get(key, 0) + value
    return totals


def benchmark(binary, root, out, mode):
    env = os.environ.copy()
    env.update(RAPIDROOM_EXPORT_MEMORY_MODE=mode, CARGO_BUILD_JOBS="4", CARGO_INCREMENTAL="0")
    env["LD_LIBRARY_PATH"] = ":".join(filter(None, [str(root / "src-tauri/resources"), str(binary.parent), env.get("LD_LIBRARY_PATH")]))
    command = ["nice", "-n", "10", str(binary), "tiled_export_sixty_mp_memory", "--ignored", "--nocapture", "--test-threads=1"]
    peaks = {}
    with open("/tmp/rapidroom-gpu.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        subprocess.run(["df", "-h", "/home"], check=True)
        if shutil.disk_usage(out).free < 25 * 1024**3:
            raise RuntimeError("Memory sample stopped: less than 25 GiB free")
        start = time.time()
        with (out / f"{mode}.log").open("w") as log:
            process = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
            while process.poll() is None:
                for key, value in drm_memory(process.pid).items():
                    peaks[key] = max(peaks.get(key, 0), value)
                try:
                    process.wait(timeout=0.05)
                except subprocess.TimeoutExpired:
                    pass
        code = process.returncode
    log = (out / f"{mode}.log").read_text()
    checksum = re.search(rf"60MP memory {mode}: checksum=(\d+)", log)
    families = {f: sum(v for k, v in peaks.items() if k.startswith(f"drm-{f}-")) for f in ("total", "resident", "memory")}
    peak = families["total"] or families["memory"]
    return {"mode": mode, "command": command, "returncode": code, "elapsed_seconds": time.time() - start,
            "peak_metric_bytes": peaks, "peak_family_bytes": families, "peak_gpu_total_bytes": peak,
            "checksum": checksum.group(1) if checksum else None,
            "passed": code == 0 and checksum is not None and peak > 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    args.out.mkdir(parents=True, exist_ok=True)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=root):
        raise SystemExit("Memory evidence requires a clean source tree")
    receipt = {"head": head, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "budget_bytes": BUDGET, "runs": [], "passed": False}
    for mode in ("legacy", "bounded"):
        receipt["runs"].append(benchmark(binary, root, args.out, mode))
        (args.out / "memory.json").write_text(json.dumps(receipt, indent=2) + "\n")
        if not receipt["runs"][-1]["passed"]:
            raise SystemExit(f"{mode} failed or DRM metrics unavailable; inspect receipt/log")
    old, new = receipt["runs"]
    receipt["passed"] = old["checksum"] == new["checksum"] and new["peak_gpu_total_bytes"] <= BUDGET and new["peak_gpu_total_bytes"] < old["peak_gpu_total_bytes"]
    (args.out / "memory.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if not receipt["passed"]:
        raise SystemExit("Checksum, measured budget or memory reduction failed")
    print(json.dumps({"before": old["peak_gpu_total_bytes"], "after": new["peak_gpu_total_bytes"], "budget": BUDGET}))


if __name__ == "__main__":
    main()
