#!/usr/bin/env python3
"""RapidRAW image-regression baseline (Stage 2).

Subcommands
  fetch     download / verify the corpus raws ($RAPIDROOM_CORPUS, default ~/.cache/rapidroom-corpus)
  render    render corpus x adjustment sets with one engine into a directory
  compare   compare two render directories; nonzero exit on failure
  hashes    write the per-image pixel hashes of a render directory (the CI reference format)
  check     compare pixel hashes and dimensions only (render dirs or hash files); nonzero exit on failure
  init      record the FIRST reference manifest (only if none exists yet)
  bless     replace the reference (maintainers only: --i-am-a-maintainer, interactive TTY)

Needs python3 + numpy only (ImageMagick `magick` is used only as a fallback
TIFF decoder). See README.md for the workflow and how tolerances were set.
"""
import argparse
import datetime as dt
import getpass
import hashlib
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.dirname(HERE)
CORPUS_JSON = os.path.join(HERE, "corpus.json")
ADJ_DIR = os.path.join(HERE, "adjustments")
TOL_JSON = os.path.join(HERE, "tolerances.json")
REF_MANIFEST = os.path.join(HERE, "reference-manifest.json")
CI_REFERENCE = os.path.join(HERE, "ci-reference.json")
BLESS_LOG = os.path.join(HERE, "BLESS-LOG.md")
DEFAULT_RAW_DIR = os.environ.get("RAPIDROOM_CORPUS", os.path.join(os.path.expanduser("~"), ".cache", "rapidroom-corpus"))
NEUTRAL = "neutral"  # adjustment set name for "no --adjustments" (engine defaults)


# --------------------------------------------------------------------------- utils

def sha256_file(path, bufsize=1 << 22):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(bufsize)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def load_json(path):
    with open(path) as f:
        return json.load(f)


def save_json(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(obj, f, indent=2, sort_keys=False)
        f.write("\n")
    os.replace(tmp, path)


def now_iso():
    return dt.datetime.now().astimezone().isoformat(timespec="seconds")


def run_quiet(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception as e:  # noqa: BLE001
        return f"<unavailable: {e}>"


def corpus_files(only=None):
    files = load_json(CORPUS_JSON)["files"]
    if only:
        want = set(only.split(","))
        files = [f for f in files if f["id"] in want]
        missing = want - {f["id"] for f in files}
        if missing:
            sys.exit(f"unknown corpus ids: {sorted(missing)}")
    return files


def adjustment_sets(names=None):
    """Return {name: path-or-None}. 'neutral' = no --adjustments flag."""
    sets = {NEUTRAL: None}
    for fn in sorted(os.listdir(ADJ_DIR)):
        if fn.endswith(".json"):
            sets[fn[:-5]] = os.path.join(ADJ_DIR, fn)
    if names:
        want = names.split(",")
        bad = [n for n in want if n not in sets]
        if bad:
            sys.exit(f"unknown adjustment sets: {bad} (have {list(sets)})")
        sets = {n: sets[n] for n in want}
    return sets


# --------------------------------------------------------------------------- TIFF

TIFF_TYPES = {1: ("B", 1), 2: ("s", 1), 3: ("H", 2), 4: ("I", 4), 5: ("II", 8), 7: ("B", 1),
              16: ("Q", 8)}


def _tiff_tags(f):
    head = f.read(8)
    if head[:2] == b"II":
        bo = "<"
    elif head[:2] == b"MM":
        bo = ">"
    else:
        raise ValueError("not a TIFF")
    magic, off = struct.unpack(bo + "HI", head[2:8])
    if magic != 42:
        raise ValueError("BigTIFF / unknown TIFF variant")
    f.seek(off)
    (n,) = struct.unpack(bo + "H", f.read(2))
    tags = {}
    for _ in range(n):
        tag, typ, cnt, raw = struct.unpack(bo + "HHI4s", f.read(12))
        fmt, size = TIFF_TYPES.get(typ, ("B", 1))
        if typ in (2, 7) or typ not in TIFF_TYPES:
            nbytes = cnt * size
            tags[tag] = ("bytes", cnt, raw if nbytes <= 4 else struct.unpack(bo + "I", raw)[0])
            continue
        nbytes = cnt * size
        if nbytes <= 4:
            vals = struct.unpack(bo + fmt[0] * cnt, raw[:nbytes])
        else:
            (voff,) = struct.unpack(bo + "I", raw)
            pos = f.tell()
            f.seek(voff)
            vals = struct.unpack(bo + fmt[0] * (cnt * len(fmt)), f.read(nbytes))
            f.seek(pos)
        tags[tag] = ("vals", cnt, vals)
    return bo, tags


def _tag(tags, t, default=None):
    if t not in tags:
        return default
    kind, cnt, v = tags[t]
    return v


def read_tiff(path):
    """Return (HxWx3 uint16 array, info dict). Handles uncompressed chunky
    8/16-bit RGB(A) strips (what RapidRAW writes); otherwise falls back to
    ImageMagick."""
    with open(path, "rb") as f:
        bo, tags = _tiff_tags(f)
        w = _tag(tags, 256)[0]
        h = _tag(tags, 257)[0]
        bps = _tag(tags, 258, (1,))
        spp = _tag(tags, 277, (1,))[0]
        comp = _tag(tags, 259, (1,))[0]
        planar = _tag(tags, 284, (1,))[0]
        info = {"width": w, "height": h, "bits": bps[0], "samples": spp, "compression": comp,
                "icc": 34675 in tags, "icc_desc": None}
        if 34675 in tags:
            kind, cnt, off = tags[34675]
            icc = None
            if kind == "vals":  # written as BYTE (type 1), values already read
                icc = bytes(off)
            elif isinstance(off, int):  # UNDEFINED (type 7) at an offset
                f.seek(off)
                icc = f.read(cnt)
            if icc:
                info["icc_desc"] = _icc_description(icc)
        ok = comp == 1 and planar == 1 and spp in (3, 4) and all(b == bps[0] for b in bps) \
            and bps[0] in (8, 16) and 273 in tags and 279 in tags
        if ok:
            offs = _tag(tags, 273)
            cnts = _tag(tags, 279)
            buf = bytearray()
            for o, c in zip(offs, cnts):
                f.seek(o)
                buf += f.read(c)
            dtype = np.dtype(bo + ("u2" if bps[0] == 16 else "u1"))
            a = np.frombuffer(bytes(buf), dtype=dtype, count=w * h * spp).reshape(h, w, spp)
            a = a[:, :, :3]
            if bps[0] == 8:
                a = a.astype(np.uint16) * 257
            else:
                a = a.astype(np.uint16, copy=False)
            info["decoder"] = "builtin"
            return np.ascontiguousarray(a), info
    # Fallback: ImageMagick -> 16-bit PPM
    out = subprocess.run(["magick", path, "-depth", "16", "-alpha", "off", "ppm:-"],
                         capture_output=True, check=True).stdout
    m = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", out)
    w, h, mx = int(m.group(1)), int(m.group(2)), int(m.group(3))
    a = np.frombuffer(out[m.end():], dtype=">u2", count=w * h * 3).reshape(h, w, 3).astype(np.uint16)
    info["decoder"] = "magick"
    return a, info


def _icc_description(icc):
    try:
        (ntags,) = struct.unpack(">I", icc[128:132])
        for i in range(ntags):
            sig, off, size = struct.unpack(">4sII", icc[132 + 12 * i:144 + 12 * i])
            if sig == b"desc":
                d = icc[off:off + size]
                if d[:4] == b"desc":
                    (n,) = struct.unpack(">I", d[8:12])
                    return d[12:12 + n].rstrip(b"\0").decode("latin1")
                if d[:4] == b"mluc":
                    (nrec,) = struct.unpack(">I", d[8:12])
                    _, _, ln, lo = struct.unpack(">2s2sII", d[16:28])
                    return d[lo:lo + ln].decode("utf-16-be")
    except Exception:  # noqa: BLE001
        pass
    return "<unparsed>"


def pixel_sha256(a):
    return hashlib.sha256(np.ascontiguousarray(a).astype("<u2", copy=False).tobytes()).hexdigest()


# --------------------------------------------------------------------------- colour

def srgb_to_linear(v):
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)


M_SRGB_TO_XYZ = np.array([[0.4124564, 0.3575761, 0.1804375],
                          [0.2126729, 0.7151522, 0.0721750],
                          [0.0193339, 0.1191920, 0.9503041]])
WHITE_D65 = np.array([0.95047, 1.0, 1.08883])


def downsample_linear(a16, long_side):
    """Box-downsample a uint16 sRGB-encoded image in linear light. Returns float64
    linear RGB (h, w, 3). Integer factor so ref and test map identically."""
    h, w, _ = a16.shape
    k = max(1, int(np.ceil(max(h, w) / long_side)))
    hh, ww = h // k, w // k
    out = np.zeros((hh, ww, 3))
    for y in range(hh):  # row-block loop keeps memory small on 60+ MP images
        blk = a16[y * k:(y + 1) * k, :ww * k].astype(np.float64) / 65535.0
        lin = srgb_to_linear(blk)
        out[y] = lin.reshape(k, ww, k, 3).mean(axis=(0, 2))
    return out


def linear_to_lab(lin):
    xyz = lin @ M_SRGB_TO_XYZ.T / WHITE_D65
    e = 216 / 24389
    kappa = 24389 / 27
    f = np.where(xyz > e, np.cbrt(xyz), (kappa * xyz + 16) / 116)
    L = 116 * f[..., 1] - 16
    a = 500 * (f[..., 0] - f[..., 1])
    b = 200 * (f[..., 1] - f[..., 2])
    return np.stack([L, a, b], axis=-1)


def ciede2000(lab1, lab2):
    L1, a1, b1 = lab1[..., 0], lab1[..., 1], lab1[..., 2]
    L2, a2, b2 = lab2[..., 0], lab2[..., 1], lab2[..., 2]
    C1 = np.hypot(a1, b1)
    C2 = np.hypot(a2, b2)
    Cb = (C1 + C2) / 2
    G = 0.5 * (1 - np.sqrt(Cb ** 7 / (Cb ** 7 + 25.0 ** 7)))
    a1p, a2p = (1 + G) * a1, (1 + G) * a2
    C1p, C2p = np.hypot(a1p, b1), np.hypot(a2p, b2)
    h1p = np.degrees(np.arctan2(b1, a1p)) % 360
    h2p = np.degrees(np.arctan2(b2, a2p)) % 360
    dLp = L2 - L1
    dCp = C2p - C1p
    dhp = h2p - h1p
    dhp = np.where(dhp > 180, dhp - 360, dhp)
    dhp = np.where(dhp < -180, dhp + 360, dhp)
    dhp = np.where(C1p * C2p == 0, 0, dhp)
    dHp = 2 * np.sqrt(C1p * C2p) * np.sin(np.radians(dhp / 2))
    Lbp = (L1 + L2) / 2
    Cbp = (C1p + C2p) / 2
    hsum = h1p + h2p
    hbp = np.where(np.abs(h1p - h2p) > 180, (hsum + 360) / 2, hsum / 2)
    hbp = np.where(hbp >= 360, hbp - 360, hbp)
    hbp = np.where(C1p * C2p == 0, hsum, hbp)
    T = (1 - 0.17 * np.cos(np.radians(hbp - 30)) + 0.24 * np.cos(np.radians(2 * hbp))
         + 0.32 * np.cos(np.radians(3 * hbp + 6)) - 0.20 * np.cos(np.radians(4 * hbp - 63)))
    dth = 30 * np.exp(-(((hbp - 275) / 25) ** 2))
    Rc = 2 * np.sqrt(Cbp ** 7 / (Cbp ** 7 + 25.0 ** 7))
    Sl = 1 + 0.015 * (Lbp - 50) ** 2 / np.sqrt(20 + (Lbp - 50) ** 2)
    Sc = 1 + 0.045 * Cbp
    Sh = 1 + 0.015 * Cbp * T
    Rt = -np.sin(np.radians(2 * dth)) * Rc
    return np.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2
                   + Rt * (dCp / Sc) * (dHp / Sh))


# --------------------------------------------------------------------------- fetch

def cmd_fetch(args):
    raw_dir = args.raw_dir
    os.makedirs(raw_dir, exist_ok=True)
    bad = 0
    for e in corpus_files(args.only):
        p = os.path.join(raw_dir, e["file"])
        if not (os.path.isfile(p) and os.path.getsize(p) > 0):
            print(f"download {e['id']}")
            r = subprocess.run(["curl", "-sSL", "--fail", "--retry", "3", "-o", p + ".part", e["url"]])
            if r.returncode != 0:
                print(f"  FAILED (curl exit {r.returncode})")
                bad += 1
                continue
            os.replace(p + ".part", p)
        got = sha256_file(p)
        ok = got == e["sha256"]
        print(f"{'ok ' if ok else 'BAD'} {e['id']:32s} {os.path.getsize(p):>11,d} B  sha256 {got[:16]}")
        bad += not ok
    sys.exit(1 if bad else 0)


# --------------------------------------------------------------------------- render

def engine_info(engine):
    engine = os.path.realpath(engine)
    info = {"path": engine, "sha256": sha256_file(engine), "size": os.path.getsize(engine),
            "mtime": dt.datetime.fromtimestamp(os.path.getmtime(engine)).isoformat(timespec="seconds")}
    bi = os.path.join(os.path.dirname(engine), "BUILD-INFO")
    if os.path.isfile(bi):
        info["build_info"] = open(bi).read().strip()
    if engine.startswith("/usr/"):
        owner = run_quiet(["pacman", "-Qo", engine])
        if owner:
            info["package"] = owner
    return info


def environment_info():
    gpus = []
    for card in sorted(os.listdir("/sys/class/drm")) if os.path.isdir("/sys/class/drm") else []:
        ue = f"/sys/class/drm/{card}/device/uevent"
        if re.fullmatch(r"card\d+", card) and os.path.isfile(ue):
            kv = dict(l.split("=", 1) for l in open(ue).read().split() if "=" in l)
            gpus.append({"card": card, "driver": kv.get("DRIVER"), "pci_id": kv.get("PCI_ID")})
    env = {
        "hostname": platform.node(),
        "kernel": platform.release(),
        "cpu_count": os.cpu_count(),
        "gpus": gpus,
        "packages": run_quiet(["pacman", "-Q", "mesa", "vulkan-intel", "intel-media-driver",
                               "vulkan-icd-loader"]).splitlines(),
        "python": platform.python_version(),
        "numpy": np.__version__,
    }
    if shutil.which("dpkg-query"):  # Debian/Ubuntu, e.g. the CI runner
        env["debian_packages"] = run_quiet(["dpkg-query", "-W", "-f", "${Package} ${Version}\\n",
                                            "mesa-vulkan-drivers", "libvulkan1", "libwebkit2gtk-4.1-0"]).splitlines()
    vk = run_quiet(["vulkaninfo", "--summary"])
    if vk and not vk.startswith("<unavailable"):
        env["vulkan"] = sorted({l.strip() for l in vk.splitlines()
                                if re.match(r"\s*(deviceName|driverName|driverInfo|apiVersion|conformanceVersion)\s*=", l)})
    return env


def resolve_adjustments(src, dst):
    """Copy an adjustment JSON, making lutPath absolute (relative to adjustments/)."""
    adj = load_json(src)
    adj.pop("_comment", None)
    if isinstance(adj.get("lutPath"), str) and not os.path.isabs(adj["lutPath"]):
        lut = os.path.join(ADJ_DIR, adj["lutPath"])
        if not os.path.isfile(lut):
            sys.exit(f"LUT not found: {lut}")
        adj["lutPath"] = lut
    save_json(dst, adj)
    return adj


GPU_RE = re.compile(r"\[process_image_for_export\] (\d+)x(\d+) processed .*? on GPU in ([0-9.]+)(µs|ms|s)")


def parse_gpu_seconds(log_text):
    m = GPU_RE.search(log_text)
    if not m:
        return None
    v = float(m.group(3))
    return v * {"µs": 1e-6, "ms": 1e-3, "s": 1.0}[m.group(4)]


def cmd_render(args):
    global ADJ_DIR
    if args.adj_dir:  # extra/experimental sets (e.g. sensitivity checks); not the baseline sets
        ADJ_DIR = os.path.abspath(args.adj_dir)
    engine = os.path.realpath(args.engine)
    if not os.access(engine, os.X_OK):
        sys.exit(f"engine not executable: {engine}")
    out = os.path.abspath(args.out)
    if os.path.exists(os.path.join(out, "manifest.json")) and not args.force:
        sys.exit(f"{out} already has a manifest.json; use a new directory or --force")
    os.makedirs(out, exist_ok=True)
    files = corpus_files(args.only)
    sets = adjustment_sets(args.sets)

    # 1. Corpus integrity + no sidecars (the neutral set reads <raw>.rrdata if present).
    print("verifying corpus ...", flush=True)
    for e in files:
        p = os.path.join(args.raw_dir, e["file"])
        if not os.path.isfile(p):
            sys.exit(f"missing raw {p}; run: baseline.py fetch")
        if not args.skip_hash and sha256_file(p) != e["sha256"]:
            sys.exit(f"sha256 mismatch for {p}")
    stray = [f for f in os.listdir(args.raw_dir) if f.endswith((".rrdata", ".rrexif", ".xmp"))]
    if stray:
        sys.exit(f"sidecar files in {args.raw_dir} would change renders: {stray}")

    # 2. Pinned environment: fresh, empty app data/config/cache so the user's
    #    RapidRAW settings.json (tone-mapper override, raw highlight compression,
    #    processing backend, GPU-crash fallback flag ...) cannot leak in.
    envroot = os.path.join(out, "_env")
    shutil.rmtree(envroot, ignore_errors=True)
    for d in ("data", "config", "cache", "adjustments"):
        os.makedirs(os.path.join(envroot, d))
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("WGPU_", "RUST_LOG", "VK_", "MESA_", "ORT_"))}
    env.update(XDG_DATA_HOME=os.path.join(envroot, "data"),
               XDG_CONFIG_HOME=os.path.join(envroot, "config"),
               XDG_CACHE_HOME=os.path.join(envroot, "cache"))
    resolved = {}
    for name, path in sets.items():
        if path:
            dst = os.path.join(envroot, "adjustments", name + ".json")
            resolve_adjustments(path, dst)
            resolved[name] = {"source": os.path.relpath(path, PROJECT), "sha256": sha256_file(path),
                              "resolved": dst}
        else:
            resolved[name] = {"source": None, "note": "no --adjustments; engine defaults"}

    manifest = {
        "kind": "rapidraw-baseline-render",
        "created": now_iso(),
        "engine": engine_info(engine),
        "environment": environment_info(),
        "pinned_env": {"XDG_DATA_HOME": env["XDG_DATA_HOME"], "XDG_CONFIG_HOME": env["XDG_CONFIG_HOME"],
                       "XDG_CACHE_HOME": env["XDG_CACHE_HOME"],
                       "removed_vars": sorted(set(os.environ) - set(env))},
        "command": "RapidRAW export <raw> --output <out.tiff> --format tiff --tiff-bit-depth 16 [--adjustments <json>]",
        "corpus_sha256": sha256_file(CORPUS_JSON),
        "adjustment_sets": resolved,
        "renders": [],
    }
    lut = os.path.join(ADJ_DIR, "baseline-warm-teal-9.cube")
    if os.path.isfile(lut):
        manifest["lut_sha256"] = sha256_file(lut)

    # 3. Warm-up (untimed, discarded): the fresh XDG_CACHE_HOME means an empty
    #    Mesa shader cache, so the first export of each set pays ~10-25 s of
    #    shader compilation. Render the smallest raw once per set first.
    if not args.no_warmup:
        small = min(files, key=lambda e: e["size_bytes"])
        wdir = os.path.join(envroot, "warmup")
        os.makedirs(wdir, exist_ok=True)
        for name in sets:
            cmd = [engine, "export", os.path.join(args.raw_dir, small["file"]), "--output",
                   os.path.join(wdir, name + ".tiff"), "--format", "tiff", "--tiff-bit-depth", "16"]
            if sets[name]:
                cmd += ["--adjustments", resolved[name]["resolved"]]
            t0 = time.time()
            with open(os.path.join(wdir, name + ".log"), "w") as lf:
                subprocess.run(cmd, stdout=lf, stderr=subprocess.STDOUT, env=env,
                               stdin=subprocess.DEVNULL, timeout=args.timeout)
            print(f"warm-up {name:24s} {small['id']} {time.time() - t0:5.1f}s", flush=True)
        manifest["warmup"] = {"image": small["id"], "note": "one untimed render per set, discarded"}

    total = len(files) * len(sets)
    n = 0
    t_all = time.time()
    failures = 0
    for name in sets:
        sdir = os.path.join(out, name)
        os.makedirs(sdir, exist_ok=True)
        for e in files:
            n += 1
            raw = os.path.join(args.raw_dir, e["file"])
            tif = os.path.join(sdir, e["id"] + ".tiff")
            log = os.path.join(sdir, e["id"] + ".log")
            if os.path.exists(tif):
                os.remove(tif)
            cmd = [engine, "export", raw, "--output", tif, "--format", "tiff", "--tiff-bit-depth", "16"]
            if sets[name]:
                cmd += ["--adjustments", resolved[name]["resolved"]]
            t0 = time.time()
            with open(log, "w") as lf:  # never pipe the engine through head (see STAGE-0)
                try:
                    rc = subprocess.run(cmd, stdout=lf, stderr=subprocess.STDOUT, env=env,
                                        stdin=subprocess.DEVNULL, timeout=args.timeout).returncode
                except subprocess.TimeoutExpired:
                    rc = "timeout"
            wall = time.time() - t0
            text = open(log, errors="replace").read()
            rec = {"set": name, "id": e["id"], "exit": rc, "wall_s": round(wall, 2),
                   "gpu_s": parse_gpu_seconds(text), "output": os.path.relpath(tif, out)}
            ok = (rc == 0 and os.path.isfile(tif) and os.path.getsize(tif) > 0
                  and "Headless export completed successfully" in text)
            if ok:
                a, info = read_tiff(tif)
                rec.update(width=info["width"], height=info["height"], icc=info["icc_desc"],
                           file_sha256=sha256_file(tif), pixel_sha256=pixel_sha256(a))
                del a
            else:
                failures += 1
                rec["error"] = "export failed (see log)"
            rec["ok"] = ok
            manifest["renders"].append(rec)
            print(f"[{n:3d}/{total}] {name:24s} {e['id']:30s} {'ok ' if ok else 'FAIL'} "
                  f"wall {wall:6.1f}s gpu {rec['gpu_s'] if rec['gpu_s'] is not None else '-'}", flush=True)
            save_json(os.path.join(out, "manifest.json"), manifest)  # incremental
    manifest["total_wall_s"] = round(time.time() - t_all, 1)
    # Record which wgpu backend/adapter lines the engine logged.
    applog = os.path.join(envroot, "data", "io.github.CyberTimon.RapidRAW", "logs", "app.log")
    if os.path.isfile(applog):
        lines = [l.strip() for l in open(applog, errors="replace")
                 if re.search(r"backend|adapter|cooperative|GPU Driver crash|OpenGL", l, re.I)]
        manifest["engine_gpu_log_sample"] = sorted(set(re.sub(r"^\S+ \S+ ", "", l) for l in lines))[:20]
    save_json(os.path.join(out, "manifest.json"), manifest)
    print(f"done: {total - failures}/{total} ok, {manifest['total_wall_s']} s total -> {out}/manifest.json")
    sys.exit(1 if failures else 0)


# --------------------------------------------------------------------------- compare

def compare_pair(ref_path, test_path, threshold, de_long_side):
    a, ia = read_tiff(ref_path)
    b, ib = read_tiff(test_path)
    r = {"ref_icc": ia["icc_desc"], "test_icc": ib["icc_desc"],
         "ref_size": [ia["width"], ia["height"]], "test_size": [ib["width"], ib["height"]],
         "ref_pixel_sha256": pixel_sha256(a), "test_pixel_sha256": pixel_sha256(b)}
    if a.shape != b.shape:
        r["error"] = "size mismatch"
        return r
    if r["ref_pixel_sha256"] == r["test_pixel_sha256"]:
        r.update(identical=True, max_abs=0, mean_abs=0.0, psnr_db=float("inf"),
                 pct_over_threshold=0.0, de00_mean=0.0, de00_p99=0.0, de00_max=0.0)
        return r
    h = a.shape[0]
    step = 256
    max_abs = 0
    sum_abs = 0.0
    sum_sq = 0.0
    over = 0
    for y in range(0, h, step):
        d = np.abs(a[y:y + step].astype(np.int32) - b[y:y + step].astype(np.int32))
        max_abs = max(max_abs, int(d.max()))
        sum_abs += float(d.sum(dtype=np.float64))
        sum_sq += float((d.astype(np.float64) ** 2).sum())
        over += int((d.max(axis=2) > threshold).sum())
    nvals = a.size
    mse = sum_sq / nvals
    lab_a = linear_to_lab(downsample_linear(a, de_long_side))
    lab_b = linear_to_lab(downsample_linear(b, de_long_side))
    de = ciede2000(lab_a, lab_b)
    r.update(identical=False, max_abs=max_abs, mean_abs=sum_abs / nvals,
             psnr_db=(10 * np.log10(65535.0 ** 2 / mse)) if mse > 0 else float("inf"),
             pct_over_threshold=100.0 * over / (a.shape[0] * a.shape[1]),
             de00_mean=float(de.mean()), de00_p99=float(np.percentile(de, 99)), de00_max=float(de.max()))
    return r


def _compare_job(job):
    key, refp, testp, thr, ls = job
    try:
        return key, compare_pair(refp, testp, thr, ls)
    except Exception as e:  # noqa: BLE001
        return key, {"error": f"{type(e).__name__}: {e}"}


def judge(r, tol):
    """Return list of tolerance violations for one image."""
    if "error" in r:
        return [r["error"]]
    if r.get("identical"):
        return []
    v = []
    if r["max_abs"] > tol["max_abs"]:
        v.append(f"max_abs {r['max_abs']} > {tol['max_abs']}")
    if r["mean_abs"] > tol["mean_abs"]:
        v.append(f"mean_abs {r['mean_abs']:.3f} > {tol['mean_abs']}")
    if r["psnr_db"] < tol["psnr_db_min"]:
        v.append(f"PSNR {r['psnr_db']:.1f} < {tol['psnr_db_min']}")
    if r["pct_over_threshold"] > tol["pct_over_threshold"]:
        v.append(f"pct>thr {r['pct_over_threshold']:.4f}% > {tol['pct_over_threshold']}%")
    if r["de00_mean"] > tol["de00_mean"]:
        v.append(f"dE00 mean {r['de00_mean']:.4f} > {tol['de00_mean']}")
    if r["de00_p99"] > tol["de00_p99"]:
        v.append(f"dE00 p99 {r['de00_p99']:.3f} > {tol['de00_p99']}")
    return v


def load_tolerances(path, profile=None):
    t = load_json(path)
    name = profile or t["default_profile"]
    if name not in t["profiles"]:
        sys.exit(f"unknown tolerance profile {name!r}; have {list(t['profiles'])}")
    return name, t["profiles"][name], t


def env_signature(man):
    if not man:
        return None
    e = man.get("environment", {})
    return {"hostname": e.get("hostname"), "kernel": e.get("kernel"), "gpus": e.get("gpus"),
            "packages": e.get("packages"), "vulkan": e.get("vulkan")}


def cmd_compare(args):
    profile, tol, tolf = load_tolerances(args.tolerances, args.profile)
    thr = tolf["pixel_threshold_codes"]
    ls = tolf["de00_downsample_long_side"]
    ref, test = os.path.abspath(args.refdir), os.path.abspath(args.testdir)
    rm = os.path.join(ref, "manifest.json")
    tm = os.path.join(test, "manifest.json")
    ref_man = load_json(rm) if os.path.isfile(rm) else None
    test_man = load_json(tm) if os.path.isfile(tm) else None

    sets = sorted(d for d in os.listdir(ref) if os.path.isdir(os.path.join(ref, d)) and not d.startswith("_"))
    if args.sets:
        sets = [s for s in sets if s in args.sets.split(",")]
    jobs, missing = [], []
    for s in sets:
        for fn in sorted(os.listdir(os.path.join(ref, s))):
            if not fn.endswith(".tiff"):
                continue
            iid = fn[:-5]
            if args.only and iid not in args.only.split(","):
                continue
            tp = os.path.join(test, s, fn)
            if not os.path.isfile(tp):
                missing.append(f"{s}/{iid}")
                continue
            jobs.append(((s, iid), os.path.join(ref, s, fn), tp, thr, ls))
    if not jobs and not missing:
        sys.exit("nothing to compare")

    results = {}
    if args.jobs > 1:
        from multiprocessing import Pool
        with Pool(args.jobs) as pool:
            for key, r in pool.imap_unordered(_compare_job, jobs):
                results[key] = r
                print(f"  compared {key[0]}/{key[1]}", file=sys.stderr, flush=True)
    else:
        for j in jobs:
            key, r = _compare_job(j)
            results[key] = r
            print(f"  compared {key[0]}/{key[1]}", file=sys.stderr, flush=True)

    # Is refdir really the committed reference?
    committed = load_json(REF_MANIFEST) if os.path.isfile(REF_MANIFEST) else None
    ref_check = None
    if committed:
        want = {(x["set"], x["id"]): x.get("pixel_sha256") for x in committed["renders"] if x.get("ok")}
        mism = [f"{k[0]}/{k[1]}" for k, r in results.items()
                if k in want and r.get("ref_pixel_sha256") and r["ref_pixel_sha256"] != want[k]]
        ref_check = {"matches_committed_reference": not mism, "mismatched": mism}

    rows, failed = [], 0
    for key in sorted(results):
        r = results[key]
        v = judge(r, tol)
        r["violations"] = v
        r["pass"] = not v
        failed += bool(v)
        rows.append({"set": key[0], "id": key[1], **r})
    for m in missing:
        rows.append({"set": m.split("/")[0], "id": m.split("/")[1], "pass": False,
                     "violations": ["missing in testdir"]})
        failed += 1

    def agg(field, fn):
        vals = [x[field] for x in rows if field in x and x[field] is not None and np.isfinite(x[field])]
        return fn(vals) if vals else None

    summary = {
        "kind": "rapidraw-baseline-compare",
        "created": now_iso(),
        "refdir": ref, "testdir": test,
        "ref_engine": ref_man and {k: ref_man["engine"].get(k) for k in ("path", "sha256", "build_info", "package")},
        "test_engine": test_man and {k: test_man["engine"].get(k) for k in ("path", "sha256", "build_info", "package")},
        "tolerances_file": os.path.relpath(os.path.abspath(args.tolerances), PROJECT),
        "profile": profile, "tolerances": tol, "pixel_threshold_codes": thr,
        "de00_downsample_long_side": ls,
        "same_environment": (env_signature(ref_man) == env_signature(test_man)) if ref_man and test_man else None,
        "reference_check": ref_check,
        "n_compared": len(results), "n_missing": len(missing), "n_failed": failed,
        "n_identical": sum(1 for x in rows if x.get("identical")),
        "worst": {"max_abs": agg("max_abs", max), "mean_abs": agg("mean_abs", max),
                  "psnr_db_min": agg("psnr_db", min), "pct_over_threshold": agg("pct_over_threshold", max),
                  "de00_mean": agg("de00_mean", max), "de00_p99": agg("de00_p99", max),
                  "de00_max": agg("de00_max", max)},
        "pass": failed == 0,
        "images": rows,
    }
    if args.json:
        js = json.loads(json.dumps(summary, default=float).replace("Infinity", "null"))
        save_json(args.json, js)

    # human summary
    print(f"\nRapidRAW baseline compare\n  ref : {ref}\n  test: {test}")
    if ref_man and test_man:
        print(f"  ref engine  sha256 {ref_man['engine']['sha256'][:16]}  {ref_man['engine']['path']}")
        print(f"  test engine sha256 {test_man['engine']['sha256'][:16]}  {test_man['engine']['path']}")
    print(f"  tolerance profile: {profile}")
    if summary["same_environment"] is False:
        print("  NOTE: ref and test were rendered in different environments (GPU/driver/kernel/host); "
              "differences may be environmental. See README 'Tolerances'.")
    if ref_check is not None and not ref_check["matches_committed_reference"]:
        print(f"  WARNING: refdir does not match reference-manifest.json for {len(ref_check['mismatched'])} images")
    print(f"\n  {'set':24s} {'image':30s} {'maxabs':>7s} {'meanabs':>8s} {'PSNR':>6s} {'%>thr':>8s} "
          f"{'dE mean':>8s} {'dE p99':>7s}  result")
    for x in rows:
        if "max_abs" in x:
            ps = "inf" if not np.isfinite(x["psnr_db"]) else f"{x['psnr_db']:.1f}"
            print(f"  {x['set']:24s} {x['id']:30s} {x['max_abs']:7d} {x['mean_abs']:8.3f} {ps:>6s} "
                  f"{x['pct_over_threshold']:8.4f} {x['de00_mean']:8.4f} {x['de00_p99']:7.3f}  "
                  f"{'identical' if x.get('identical') else ('pass' if x['pass'] else 'FAIL')}")
        else:
            print(f"  {x['set']:24s} {x['id']:30s} {'':58s}  FAIL")
        for v in x.get("violations", []):
            print(f"      - {v}")
    print(f"\n  {len(rows)} images: {summary['n_identical']} identical, "
          f"{len(rows) - failed - summary['n_identical']} within tolerance, {failed} FAILED")
    print(f"  overall: {'PASS' if failed == 0 else 'FAIL'}")
    sys.exit(0 if failed == 0 else 1)


# --------------------------------------------------------------------------- hashes / check

def _hash_side(path):
    """Load pixel hashes from a render dir (its manifest.json) or a hash file
    written by `hashes`. Returns (images {(set, id): rec}, failed [keys], meta)."""
    if os.path.isdir(path):
        m = load_json(os.path.join(path, "manifest.json"))
        images, failed = {}, []
        for r in m["renders"]:
            key = (r["set"], r["id"])
            if r.get("ok"):
                images[key] = {"width": r["width"], "height": r["height"], "pixel_sha256": r["pixel_sha256"],
                               "output": os.path.join(path, r["output"])}
            else:
                failed.append(key)
        return images, failed, {"vulkan": m.get("environment", {}).get("vulkan"),
                                "corpus_sha256": m.get("corpus_sha256")}
    h = load_json(path)
    if h.get("kind") != "rapidraw-ci-reference":
        sys.exit(f"{path} is neither a render directory nor a hash file from `baseline.py hashes`")
    images = {(x["set"], x["id"]): x for x in h["images"]}
    return images, [], {"vulkan": h.get("environment", {}).get("vulkan"), "corpus_sha256": h.get("corpus_sha256")}


def cmd_hashes(args):
    m = load_json(os.path.join(args.renderdir, "manifest.json"))
    bad = [f"{r['set']}/{r['id']}" for r in m["renders"] if not r.get("ok")]
    if bad:
        sys.exit(f"refusing: {len(bad)} renders failed in {args.renderdir}: {bad[:5]}")
    env = m.get("environment", {})
    out = {
        "_comment": "Per-image pixel hashes for the CI regression check (.github/workflows/regression.yml), "
                    "recorded on a GitHub Actions runner with Mesa lavapipe. pixel_sha256 is the SHA-256 of the "
                    "decoded 16-bit RGB pixels (little-endian), so TIFF container changes don't matter. Updating "
                    "this file is a rendering decision that needs a maintainer's sign-off (see GOVERNANCE).",
        "kind": "rapidraw-ci-reference",
        "created": now_iso(),
        "recorded_by": args.by,
        "source": args.source,
        "environment": {k: env.get(k) for k in ("kernel", "cpu_count", "vulkan", "debian_packages")},
        "engine_sha256": m["engine"]["sha256"],
        "corpus_sha256": m.get("corpus_sha256"),
        "adjustment_sets": {k: v.get("sha256") for k, v in sorted(m["adjustment_sets"].items())},
        "lut_sha256": m.get("lut_sha256"),
        "images": [{"set": r["set"], "id": r["id"], "width": r["width"], "height": r["height"],
                    "pixel_sha256": r["pixel_sha256"]}
                   for r in sorted(m["renders"], key=lambda r: (r["set"], r["id"]))],
    }
    save_json(args.out, out)
    print(f"wrote {len(out['images'])} hashes -> {args.out}")


def cmd_check(args):
    ref, ref_failed, ref_meta = _hash_side(args.ref)
    test, test_failed, test_meta = _hash_side(args.test)
    rows = []
    for key in sorted(set(ref) | set(test) | set(ref_failed) | set(test_failed)):
        r, t = ref.get(key), test.get(key)
        row = {"set": key[0], "id": key[1]}
        if key in ref_failed:
            row["problem"] = "render failed in ref"
        elif key in test_failed:
            row["problem"] = "render failed in test"
        elif t is None:
            row["problem"] = "missing in test"
        elif r is None:
            row["problem"] = "not in ref"
        else:
            row.update(ref_size=[r["width"], r["height"]], test_size=[t["width"], t["height"]],
                       ref_pixel_sha256=r["pixel_sha256"], test_pixel_sha256=t["pixel_sha256"])
            if row["ref_size"] != row["test_size"]:
                row["problem"] = f"size {r['width']}x{r['height']} -> {t['width']}x{t['height']}"
            elif r["pixel_sha256"] != t["pixel_sha256"]:
                row["problem"] = "pixels differ"
        for side, recs in (("ref_output", ref), ("test_output", test)):
            if key in recs and recs[key].get("output"):
                row[side] = recs[key]["output"]
        rows.append(row)
    bad = [x for x in rows if "problem" in x]
    notes = []
    if ref_meta["vulkan"] and test_meta["vulkan"] and ref_meta["vulkan"] != test_meta["vulkan"]:
        notes.append(f"Vulkan driver differs: ref {ref_meta['vulkan']} vs test {test_meta['vulkan']}")
    if ref_meta["corpus_sha256"] and test_meta["corpus_sha256"] and ref_meta["corpus_sha256"] != test_meta["corpus_sha256"]:
        notes.append("corpus.json differs between ref and test")
    summary = {"kind": "rapidraw-baseline-check", "created": now_iso(),
               "ref": os.path.abspath(args.ref), "test": os.path.abspath(args.test),
               "n_images": len(rows), "n_identical": len(rows) - len(bad), "n_failed": len(bad),
               "notes": notes, "pass": not bad, "images": rows}
    if args.json:
        save_json(args.json, summary)
    if args.copy_differing and bad:
        os.makedirs(args.copy_differing, exist_ok=True)
        for x in bad:
            for side in ("ref_output", "test_output"):
                src = x.get(side)
                if not src:
                    continue
                stem = f"{x['set']}__{x['id']}__{side.split('_')[0]}"
                if os.path.isfile(src):
                    shutil.copy2(src, os.path.join(args.copy_differing, stem + ".tiff"))
                log = os.path.splitext(src)[0] + ".log"
                if os.path.isfile(log):
                    shutil.copy2(log, os.path.join(args.copy_differing, stem + ".log"))

    print(f"\nRapidRAW pixel-hash check\n  ref : {args.ref}\n  test: {args.test}")
    for n in notes:
        print(f"  NOTE: {n}")
    print()
    for x in rows:
        print(f"  {x['set']:24s} {x['id']:30s} {x.get('problem', 'identical')}")
    print(f"\n  {len(rows)} images: {len(rows) - len(bad)} identical, {len(bad)} FAILED")
    print(f"  overall: {'PASS' if not bad else 'FAIL'}")
    sys.exit(0 if not bad else 1)


# --------------------------------------------------------------------------- init / bless

def _reference_from(refdir):
    m = load_json(os.path.join(refdir, "manifest.json"))
    bad = [f"{r['set']}/{r['id']}" for r in m["renders"] if not r.get("ok")]
    if bad:
        sys.exit(f"refusing: {len(bad)} renders failed in {refdir}: {bad[:5]}")
    m = dict(m)
    m["kind"] = "rapidraw-baseline-reference"
    m["reference_dir"] = os.path.relpath(os.path.abspath(refdir), PROJECT)
    return m


def _append_log(text):
    new = not os.path.exists(BLESS_LOG)
    with open(BLESS_LOG, "a") as f:
        if new:
            f.write("# Baseline reference log\n\nEvery change to `reference-manifest.json` is recorded here. "
                    "Only a maintainer may bless a new reference (see README.md).\n")
        f.write(text)


def cmd_init(args):
    if os.path.exists(REF_MANIFEST):
        sys.exit("reference-manifest.json already exists. Replacing a reference is `bless`, which only a maintainer may run.")
    m = _reference_from(args.refdir)
    m["status"] = "initial reference, created on request; awaiting a maintainer's review"
    m["recorded_by"] = args.by
    save_json(REF_MANIFEST, m)
    _append_log(f"\n## {now_iso()}: initial reference\n\n- By: {args.by}\n- Why: {args.why}\n"
                f"- Engine: `{m['engine']['path']}` sha256 `{m['engine']['sha256']}`\n"
                f"- Build: {m['engine'].get('build_info', '-')}\n- Renders: {len(m['renders'])} in "
                f"`{m['reference_dir']}`\n- Status: not blessed; a maintainer reviews before it is committed.\n")
    print(f"wrote {REF_MANIFEST}")


def cmd_bless(args):
    if not args.i_am_a_maintainer:
        sys.exit("refusing: replacing the reference needs a maintainer's explicit approval. "
                 "Agents must never bless; a maintainer runs this himself with --i-am-a-maintainer from a terminal.")
    if not sys.stdin.isatty():
        sys.exit("refusing: bless must be run interactively from a maintainer's terminal (stdin is not a TTY).")
    if not args.why or len(args.why.strip()) < 10:
        sys.exit("refusing: give a real reason with --why (which intended change makes the new renders correct).")
    if not args.compare_json or not os.path.isfile(args.compare_json):
        sys.exit("refusing: pass --compare-json from `compare <current-ref> <new-dir> --json ...` "
                 "so the log records what changed.")
    cmp_ = load_json(args.compare_json)
    m = _reference_from(args.newdir)
    print(f"New reference: {args.newdir}\n  engine {m['engine']['path']}\n  {m['engine'].get('build_info', '')}")
    print(f"Compare vs old: {cmp_['n_failed']} images out of tolerance, worst dE00 mean "
          f"{cmp_['worst']['de00_mean']}")
    ans = input("Type 'bless' to replace the reference: ")
    if ans.strip() != "bless":
        sys.exit("not blessed")
    old = load_json(REF_MANIFEST) if os.path.exists(REF_MANIFEST) else None
    m["status"] = "blessed"
    m["blessed_by"] = args.by or getpass.getuser()
    m["blessed_at"] = now_iso()
    m["bless_reason"] = args.why
    save_json(REF_MANIFEST, m)
    _append_log(f"\n## {m['blessed_at']}: bless\n\n- By: {m['blessed_by']} (--i-am-a-maintainer, interactive)\n"
                f"- Why: {args.why}\n- Old engine: {old and old['engine']['sha256']}\n"
                f"- New engine: `{m['engine']['path']}` sha256 `{m['engine']['sha256']}`\n"
                f"- Build: {m['engine'].get('build_info', '-')}\n- New renders: `{m['reference_dir']}`\n"
                f"- Compare: {cmp_['n_failed']}/{cmp_['n_compared']} out of tolerance; worst "
                f"dE00 mean {cmp_['worst']['de00_mean']}, max_abs {cmp_['worst']['max_abs']} "
                f"(`{os.path.relpath(os.path.abspath(args.compare_json), PROJECT)}`)\n")
    print(f"blessed; wrote {REF_MANIFEST} and appended {BLESS_LOG}. Commit both.")


# --------------------------------------------------------------------------- main

def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    f = sub.add_parser("fetch", help="download/verify corpus raws")
    f.add_argument("--raw-dir", default=DEFAULT_RAW_DIR)
    f.add_argument("--only", help="comma-separated corpus ids")
    f.set_defaults(fn=cmd_fetch)

    r = sub.add_parser("render", help="render corpus x adjustment sets")
    r.add_argument("--engine", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--raw-dir", default=DEFAULT_RAW_DIR)
    r.add_argument("--sets", help="comma-separated adjustment set names (default: all)")
    r.add_argument("--only", help="comma-separated corpus ids (default: all)")
    r.add_argument("--timeout", type=int, default=600)
    r.add_argument("--skip-hash", action="store_true", help="skip raw sha256 verification")
    r.add_argument("--force", action="store_true", help="overwrite an existing render dir")
    r.add_argument("--adj-dir", help="use adjustment sets from this dir instead of baseline/adjustments")
    r.add_argument("--no-warmup", action="store_true", help="skip the untimed shader warm-up renders")
    r.set_defaults(fn=cmd_render)

    c = sub.add_parser("compare", help="compare two render dirs")
    c.add_argument("refdir")
    c.add_argument("testdir")
    c.add_argument("--tolerances", default=TOL_JSON)
    c.add_argument("--profile", help="tolerance profile from tolerances.json (default: its default_profile)")
    c.add_argument("--json", help="write full results JSON here")
    c.add_argument("--sets")
    c.add_argument("--only")
    c.add_argument("--jobs", type=int, default=4)
    c.set_defaults(fn=cmd_compare)

    hs = sub.add_parser("hashes", help="write per-image pixel hashes of a render dir (CI reference format)")
    hs.add_argument("renderdir")
    hs.add_argument("--out", required=True, help=f"output JSON (the committed one is {os.path.relpath(CI_REFERENCE, PROJECT)})")
    hs.add_argument("--by", required=True, help="who recorded it (e.g. 'GitHub Actions, workflow_dispatch record')")
    hs.add_argument("--source", help="where it was recorded (e.g. a workflow run URL and commit)")
    hs.set_defaults(fn=cmd_hashes)

    ck = sub.add_parser("check", help="compare pixel hashes and dimensions only")
    ck.add_argument("ref", help="render dir or hash file (e.g. ci-reference.json)")
    ck.add_argument("test", help="render dir or hash file")
    ck.add_argument("--json", help="write the per-image results here")
    ck.add_argument("--copy-differing", metavar="DIR", help="copy the TIFFs and logs of differing renders here")
    ck.set_defaults(fn=cmd_check)

    i = sub.add_parser("init", help="record the first reference manifest (fails if one exists)")
    i.add_argument("refdir")
    i.add_argument("--by", required=True, help="who ran it (e.g. 'Claude, at a maintainer's request')")
    i.add_argument("--why", required=True)
    i.set_defaults(fn=cmd_init)

    b = sub.add_parser("bless", help="replace the reference (a maintainer only)")
    b.add_argument("newdir")
    b.add_argument("--i-am-a-maintainer", action="store_true")
    b.add_argument("--why")
    b.add_argument("--by")
    b.add_argument("--compare-json")
    b.set_defaults(fn=cmd_bless)

    a = p.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
