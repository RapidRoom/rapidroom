"""Measure RapidRoom's current HDR merge against Lightroom's merged DNG (#87, step 1).

Inputs: the harness dumps (2x2 box-downsampled linear RGB f32) per bracket.
All coordinates and shifts are reported in FULL-resolution pixels.
"""
import json
import math
import re
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(sys.argv[1])
CROPS = Path(sys.argv[2])
CROPS.mkdir(parents=True, exist_ok=True)
LOG = (ROOT / "harness.log").read_text()


def load(dirpath, name):
    shape = json.loads((dirpath / f"{name}.json").read_text())
    data = np.fromfile(dirpath / f"{name}.f32", dtype="<f4")
    return data.reshape(shape["height"], shape["width"], 3)


def lum(img):
    return img[..., 0] * 0.2126 + img[..., 1] * 0.7152 + img[..., 2] * 0.0722


def box(a, r):
    """Mean filter with radius r (window 2r+1), edge-normalised."""
    pad = np.pad(a, r, mode="edge").astype(np.float64)
    c = pad.cumsum(0).cumsum(1)
    c = np.pad(c, ((1, 0), (1, 0)))
    k = 2 * r + 1
    s = c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]
    return (s / (k * k)).astype(np.float32)


def frame_meta(first):
    rows = []
    for line in LOG.splitlines():
        m = re.match(r"(A7C\d+): (\d+)x(\d+) iso (\d+) exposure ([\d.e-]+)", line)
        if m:
            rows.append((m.group(1), int(m.group(4)), float(m.group(5))))
    start = [i for i, r in enumerate(rows) if r[0] == first][0]
    return rows[start:start + 3]


def phase_shift(a, b):
    """Sub-pixel translation of b relative to a (dy, dx) by phase correlation."""
    win = np.outer(np.hanning(a.shape[0]), np.hanning(a.shape[1]))
    fa = np.fft.fft2((a - a.mean()) * win)
    fb = np.fft.fft2((b - b.mean()) * win)
    r = fa * np.conj(fb)
    r /= np.abs(r) + 1e-12
    corr = np.fft.ifft2(r).real
    peak = np.unravel_index(np.argmax(corr), corr.shape)
    out = []
    for axis, p in enumerate(peak):
        n = corr.shape[axis]
        idx = [peak[0], peak[1]]
        def val(k):
            j = list(idx)
            j[axis] = (p + k) % n
            return corr[tuple(j)]
        c0, cm, cp = val(0), val(-1), val(1)
        den = cm - 2 * c0 + cp
        frac = 0.5 * (cm - cp) / den if den != 0 else 0.0
        s = p + frac
        if s > n / 2:
            s -= n
        out.append(s)
    return -out[0], -out[1], corr.max()


def tile_shifts(a, b, valid, tile=256):
    """Median and p90 of per-tile shift magnitudes between two log-luminance images."""
    ha = a - box(a, 6)
    hb = b - box(b, 6)
    mags = []
    for y in range(0, a.shape[0] - tile, tile):
        for x in range(0, a.shape[1] - tile, tile):
            v = valid[y:y + tile, x:x + tile]
            if v.mean() < 0.9:
                continue
            ta, tb = ha[y:y + tile, x:x + tile], hb[y:y + tile, x:x + tile]
            if ta.std() < 0.02:
                continue
            dy, dx, peak = phase_shift(ta, tb)
            if peak < 0.05:
                continue
            mags.append(2 * math.hypot(dy, dx))  # full-res px
    if not mags:
        return None
    m = np.array(mags)
    return {"tiles": len(m), "median_px": round(float(np.median(m)), 2), "p90_px": round(float(np.percentile(m, 90)), 2)}


def clip_mask(img):
    # The loader scales raw white to 1.0; highlight reconstruction can extend above it.
    return (img >= 0.9).any(axis=-1)


def tile_vectors(a, b, valid, tile=256):
    """Per-tile (centre_y, centre_x, dy, dx) in half-res px of b relative to a."""
    ha = a - box(a, 6)
    hb = b - box(b, 6)
    rows = []
    for y in range(0, a.shape[0] - tile, tile // 2):
        for x in range(0, a.shape[1] - tile, tile // 2):
            if valid[y:y + tile, x:x + tile].mean() < 0.8:
                continue
            ta, tb = ha[y:y + tile, x:x + tile], hb[y:y + tile, x:x + tile]
            if ta.std() < 0.02:
                continue
            dy, dx, peak = phase_shift(ta, tb)
            if peak > 0.05:
                rows.append((y + tile / 2, x + tile / 2, dy, dx))
    return np.array(rows)


def fit_affine(v):
    """Robust least-squares d(p) = A p + t from tile vectors; returns params and inlier residual."""
    keep = np.ones(len(v), bool)
    for _ in range(4):
        X = np.c_[v[keep, 0], v[keep, 1], np.ones(keep.sum())]
        py, *_ = np.linalg.lstsq(X, v[keep, 2], rcond=None)
        px, *_ = np.linalg.lstsq(X, v[keep, 3], rcond=None)
        Xa = np.c_[v[:, 0], v[:, 1], np.ones(len(v))]
        res = np.hypot(Xa @ py - v[:, 2], Xa @ px - v[:, 3])
        keep = res < max(1.0, 2.5 * np.median(res[keep]))
    return py, px, float(np.median(res[keep])), int(keep.sum())


def warp(img, py, px, sign):
    h, w = img.shape[:2]
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    sy = yy + sign * (py[0] * yy + py[1] * xx + py[2])
    sx = xx + sign * (px[0] * yy + px[1] * xx + px[2])
    y0 = np.clip(np.floor(sy).astype(np.int32), 0, h - 2)
    x0 = np.clip(np.floor(sx).astype(np.int32), 0, w - 2)
    fy = np.clip(sy - y0, 0, 1)[..., None]
    fx = np.clip(sx - x0, 0, 1)[..., None]
    out = (img[y0, x0] * (1 - fy) * (1 - fx) + img[y0 + 1, x0] * fy * (1 - fx)
           + img[y0, x0 + 1] * (1 - fy) * fx + img[y0 + 1, x0 + 1] * fy * fx)
    inside = (sy >= 0) & (sy <= h - 1) & (sx >= 0) & (sx <= w - 1)
    return out.astype(np.float32), inside


def scale_to(ref_log, out_log, mask):
    return float(np.median(ref_log[mask] - out_log[mask]))


def display(img_log_ref_scaled, gain):
    v = np.exp(img_log_ref_scaled)
    return v


def to_srgb8(lin):
    lin = np.clip(lin, 0, 1)
    s = np.where(lin <= 0.0031308, 12.92 * lin, 1.055 * np.power(lin, 1 / 2.4) - 0.055)
    return (np.clip(s, 0, 1) * 255 + 0.5).astype(np.uint8)


results = {}
EPS = 1e-6
for d in sorted(p for p in ROOT.iterdir() if p.is_dir()):
    first = d.name
    meta = frame_meta(first)
    names = [m[0] for m in meta]
    ref_i = 1
    # Exposure normalisation to the reference (middle) frame.
    t = [m[2] * m[1] for m in meta]
    k = [t[ref_i] / ti for ti in t]
    inputs = [load(d, f"{n}-input") for n in names]
    aligned = [load(d, f"{n}-aligned") for n in names]
    rr = load(d, "rr-merged-linear")
    rr_out = load(d, "rr-output-srgb")
    lr = load(d, "lr-linear")
    clipped = [clip_mask(a) for a in aligned]
    norm = [a * ki for a, ki in zip(aligned, k)]
    lums = [np.log(np.maximum(lum(n), EPS)) for n in norm]
    noise_floor = [np.log(0.004 * ki) for ki in k]  # raw value 0.004 in each frame
    valid = [(~c) & (l > nf) for c, l, nf in zip(clipped, lums, noise_floor)]
    r = {"frames": [{"name": n, "iso": m[1], "exposure_s": m[2], "ev_vs_ref": round(math.log2(t[i] / t[ref_i]), 2),
                     "clipped_pct": round(100 * float(clipped[i].mean()), 2)} for i, (n, m) in enumerate(zip(names, meta))]}

    # Which input frame did Lightroom keep as its geometric base? (tile shift vs unaligned inputs)
    lr_log0 = np.log(np.maximum(lum(lr), EPS))
    base = {}
    for j in range(3):
        raw_log = np.log(np.maximum(lum(inputs[j] * k[j]), EPS))
        ts = tile_shifts(raw_log, lr_log0, valid[ref_i])
        base[names[j]] = ts["median_px"] if ts else None
    r["lightroom_geometry_vs_unaligned_inputs_median_px"] = base
    # Register Lightroom's result to RapidRoom's reference frame (affine from tile shifts).
    v = tile_vectors(lums[ref_i], lr_log0, valid[ref_i])
    py, px, resid, n = fit_affine(v)
    best = None
    for sign in (1, -1):
        cand, inside = warp(lr, py, px, sign)
        ts = tile_shifts(lums[ref_i], np.log(np.maximum(lum(cand), EPS)), valid[ref_i] & inside)
        if ts and (best is None or ts["median_px"] < best[2]["median_px"]):
            best = (cand, inside, ts)
    lr, lr_inside, after = best
    r["lightroom_registration"] = {"tiles": n, "fit_residual_px": round(2 * resid, 2), "after": after,
                                   "translation_px": [round(2 * float(py[2]), 1), round(2 * float(px[2]), 1)]}

    # 1. Alignment residual vs the reference frame, before and after RapidRoom's alignment.
    r["alignment"] = {}
    for i in (0, 2):
        both = valid[i] & valid[ref_i]
        raw_log = [np.log(np.maximum(lum(inputs[j] * k[j]), EPS)) for j in (i, ref_i)]
        r["alignment"][names[i]] = {
            "before": tile_shifts(raw_log[1], raw_log[0], both),
            "after_rapidroom": tile_shifts(lums[ref_i], lums[i], both),
        }
    lr_log = np.log(np.maximum(lum(lr), EPS))
    rr_log = np.log(np.maximum(lum(rr), EPS))
    for j in range(3):
        valid[j] &= lr_inside
    static_valid = valid[ref_i]
    r["alignment"]["rapidroom_merge_vs_registered_lightroom"] = tile_shifts(lr_log, rr_log, static_valid)

    # 2. Motion and ghosting. Motion: frames disagree by > 0.35 EV where both are valid.
    tau = 0.25
    diff = np.zeros(lums[0].shape, dtype=bool)
    for i in (0, 2):
        both = valid[i] & valid[ref_i]
        diff |= both & (np.abs(lums[i] - lums[ref_i]) > tau)
    motion = box(diff.astype(np.float32), 3) > 0.5
    static = valid[ref_i] & ~(box(diff.astype(np.float32), 8) > 0.05)
    lr_s = lr_log + scale_to(lums[ref_i], lr_log, static)
    rr_s = rr_log + scale_to(lums[ref_i], rr_log, static)
    r["motion_area_pct"] = round(100 * float(motion.mean()), 3)

    def single_frame(out_s):
        cons = [motion & valid[j] & (np.abs(out_s - lums[j]) < tau / 2) for j in range(3)]
        any_c = np.zeros_like(motion)
        for c in cons:
            any_c |= c
        n = max(1, int(motion.sum()))
        return [float(c.sum()) / n for c in cons], float(any_c.sum()) / n

    lr_per, lr_any = single_frame(lr_s)
    rr_per, rr_any = single_frame(rr_s)
    # Areas Lightroom takes from exactly one frame.
    lr_cons = [motion & valid[j] & (np.abs(lr_s - lums[j]) < tau / 2) for j in range(3)]
    count = sum(c.astype(np.int32) for c in lr_cons)
    single = motion & (count == 1)
    inter = []
    for i in (0, 2):
        m2 = single & valid[i] & valid[ref_i]
        if m2.any():
            inter.append(float(np.median(np.abs(lums[i] - lums[ref_i])[m2])) / math.log(2))
    r["ghosting"] = {
        "motion_pixels_matching_some_single_frame_pct": {"lightroom": round(100 * lr_any, 1), "rapidroom": round(100 * rr_any, 1)},
        "lightroom_source_frame_pct": {names[j]: round(100 * lr_per[j], 1) for j in range(3)},
        "lightroom_single_frame_area_pct_of_image": round(100 * float(single.mean()), 3),
        "inter_frame_difference_in_those_areas_ev_median": round(float(np.median(inter)), 2) if inter else None,
        "rapidroom_minus_lightroom_in_those_areas_ev_mean_abs": round(float(np.mean(np.abs(rr_s - lr_s)[single])) / math.log(2), 2) if single.any() else None,
        "rapidroom_minus_lightroom_static_ev_mean_abs": round(float(np.mean(np.abs(rr_s - lr_s)[static])) / math.log(2), 3),
    }

    # 3. Highlights: clipped in the longest frame, unclipped in the reference (truth = reference).
    long_i = 2
    hl = clipped[long_i] & ~clipped[ref_i] & lr_inside & (lums[ref_i] > noise_floor[ref_i])
    r["highlights"] = {"long_clipped_ref_valid_pct": round(100 * float(hl.mean()), 2)}
    if hl.sum() > 1000:
        truth = lums[ref_i]
        r["highlights"]["vs_reference_ev_median"] = {
            "rapidroom": round(float(np.median((rr_s - truth)[hl])) / math.log(2), 2),
            "lightroom": round(float(np.median((lr_s - truth)[hl])) / math.log(2), 2),
        }
        def detail(x):
            return float(np.std((x - box(x, 3))[hl]))
        r["highlights"]["local_detail_std_log"] = {"reference_frame": round(detail(truth), 4), "rapidroom": round(detail(rr_s), 4), "lightroom": round(detail(lr_s), 4)}
    out8 = np.clip(rr_out, 0, 1)
    r["rapidroom_output"] = {
        "pct_any_channel_ge_254_of_255": round(100 * float((out8 >= 254 / 255).any(-1).mean()), 3),
        "pct_all_channels_le_1_of_255": round(100 * float((out8 <= 1 / 255).all(-1).mean()), 3),
        "median_srgb": round(float(np.median(lum(out8))), 3),
        "p99_9_srgb": round(float(np.percentile(lum(out8), 99.9)), 3),
    }

    # 4. Shadow noise: flat static pixels in the darkest 10% of Lightroom's result.
    dark = static & (lr_s < np.percentile(lr_s[static], 10))
    flat = box(np.abs(lr_s - box(lr_s, 2)), 4) < np.percentile(np.abs(lr_s - box(lr_s, 2))[dark], 50)
    sn = dark & flat
    def noise(x):
        return float(np.std((x - box(x, 2))[sn]))
    r["shadow_noise_std_log"] = {
        "longest_frame_alone": round(noise(lums[2]), 4),
        "reference_frame_alone": round(noise(lums[ref_i]), 4),
        "rapidroom": round(noise(rr_s), 4),
        "lightroom": round(noise(lr_s), 4),
        "pixels": int(sn.sum()),
    }

    # Crops of the region with the most motion (512x384 full-res = 256x192 half-res).
    ch, cw = 288, 384
    dens = box(motion.astype(np.float32), 64)
    dens[:ch // 2, :] = dens[-ch // 2:, :] = 0
    dens[:, :cw // 2] = dens[:, -cw // 2:] = 0
    cy, cx = np.unravel_index(np.argmax(dens), dens.shape)
    y0, x0 = int(cy - ch // 2), int(cx - cw // 2)
    r["crop_full_res"] = {"x": 2 * x0, "y": 2 * y0, "width": 2 * cw, "height": 2 * ch}
    if d.name in ("A7C00449", "A7C00428"):
        # Same display transform for every panel: reference exposure, +1.5 EV, sRGB.
        gain = 2 ** 0.0 * 0.18 / max(1e-4, float(np.median(np.exp(lums[ref_i][static]))))
        panels = []
        for label, img_lin in [
            (names[0], norm[0]), (names[1], norm[1]), (names[2], norm[2]),
        ]:
            panels.append((label, img_lin[y0:y0 + ch, x0:x0 + cw] * gain))
        rr_lin = rr * float(np.exp(scale_to(lums[ref_i], rr_log, static)))
        lr_lin = lr * float(np.exp(scale_to(lums[ref_i], lr_log, static)))
        panels.append(("RapidRoom merge", rr_lin[y0:y0 + ch, x0:x0 + cw] * gain))
        panels.append(("Lightroom merge", lr_lin[y0:y0 + ch, x0:x0 + cw] * gain))
        for label, p in panels:
            Image.fromarray(to_srgb8(p)).save(CROPS / f"{d.name}-{label.replace(' ', '-')}.png")
        Image.fromarray(np.ascontiguousarray(rr_out[y0:y0 + ch, x0:x0 + cw].clip(0, 1) * 255).astype(np.uint8)).save(
            CROPS / f"{d.name}-RapidRoom-as-saved.png")
    results[d.name] = r

print(json.dumps(results, indent=1))
