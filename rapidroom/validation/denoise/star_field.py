#!/usr/bin/env python3
"""Measure fixed original-defined star sites in aligned, linear RGB .npy images.

The sites JSON is [{"x": int, "y": int, "faint": bool}, ...]. Choose it from
an independent reference/original, never re-detect stars on the denoised result.
Input arrays must share crop, white balance, exposure and colour space. Images,
site coordinates and paths are not included in the aggregate output report.
"""
import argparse
import json
from pathlib import Path

import numpy as np


def validate_image(image):
    if image.ndim != 3 or image.shape[2] != 3 or min(image.shape[:2]) < 13:
        raise ValueError("expected an HxWx3 RGB image at least 13 pixels per side")
    if not np.isfinite(image).all():
        raise ValueError("image contains nonfinite samples")


def photometry(image, x, y):
    patch = image[y - 6:y + 7, x - 6:x + 7].astype(np.float64)
    dy, dx = np.mgrid[-6:7, -6:7]
    r2 = dx * dx + dy * dy
    sky = (r2 >= 25) & (r2 <= 36)
    core = r2 <= 16
    background = np.median(patch[sky], axis=0)
    signal = patch - background
    luminance = signal @ np.array([0.2126, 0.7152, 0.0722])
    flux_rgb = signal[core].sum(axis=0)
    flux = float(flux_rgb @ np.array([0.2126, 0.7152, 0.0722]))
    positive = np.maximum(luminance, 0) * core
    total = float(positive.sum())
    if total <= 0:
        spread = None
    else:
        cx, cy = (float((positive * axis).sum() / total) for axis in (dx, dy))
        spread = float(np.sqrt((positive * ((dx - cx)**2 + (dy - cy)**2)).sum() / total))
    return {
        "peak": float(luminance[core].max()), "flux": flux, "spread": spread,
        "colour": None if flux <= 0 else ((flux_rgb - flux_rgb[1]) / flux).tolist(),
    }


def compare(original, candidate, sites):
    validate_image(original)
    validate_image(candidate)
    if original.shape != candidate.shape:
        raise ValueError("images differ in shape; registration is required before comparison")
    if not sites:
        raise ValueError("at least one fixed original-defined star is required")
    rows = []
    seen = set()
    h, w = original.shape[:2]
    for site in sites:
        x, y, faint = site["x"], site["y"], site["faint"]
        if type(x) is not int or type(y) is not int or type(faint) is not bool:
            raise ValueError("site x/y must be integers and faint must be boolean")
        if not (6 <= x < w - 6 and 6 <= y < h - 6) or (x, y) in seen:
            raise ValueError("site has no full sky aperture or duplicates another site")
        seen.add((x, y))
        before, after = (photometry(image, x, y) for image in (original, candidate))
        if before["flux"] <= 0 or before["peak"] <= 0:
            raise ValueError("original star has no positive aperture flux/peak")
        rows.append({
            "faint": faint,
            "peak_ratio": after["peak"] / before["peak"],
            "flux_ratio": after["flux"] / before["flux"],
            "spread_ratio": None if before["spread"] in (None, 0) or after["spread"] is None
                else after["spread"] / before["spread"],
            "colour_delta": None if after["colour"] is None else
                float(np.linalg.norm(np.array(after["colour"]) - before["colour"])),
        })
    faint = [row for row in rows if row["faint"]]
    def median(key):
        values = [row[key] for row in rows if row[key] is not None]
        return float(np.median(values)) if values else None
    return {
        "protocol": "fixed-sites-linear-aperture-v1",
        "stars": len(rows), "faint_stars": len(faint),
        "faint_survival_fraction": None if not faint else
            sum(row["flux_ratio"] >= 0.5 and row["peak_ratio"] >= 0.5 for row in faint) / len(faint),
        "median_peak_ratio": median("peak_ratio"), "median_flux_ratio": median("flux_ratio"),
        "median_spread_ratio": median("spread_ratio"), "median_aperture_colour_delta": median("colour_delta"),
        "candidate_clipped_fraction_at_one": float(np.mean(np.any(candidate >= 1, axis=2))),
    }


def acceptance_failures(report, min_faint_survival=None, min_median_peak_ratio=None,
                        min_median_flux_ratio=None, max_median_spread_ratio=None,
                        max_median_colour_delta=None):
    requirements = [
        ("faint_survival_fraction", min_faint_survival, "at least", lambda a, b: a >= b),
        ("median_peak_ratio", min_median_peak_ratio, "at least", lambda a, b: a >= b),
        ("median_flux_ratio", min_median_flux_ratio, "at least", lambda a, b: a >= b),
        ("median_spread_ratio", max_median_spread_ratio, "at most", lambda a, b: a <= b),
        ("median_aperture_colour_delta", max_median_colour_delta, "at most", lambda a, b: a <= b),
    ]
    failures = []
    for metric, limit, direction, accepted in requirements:
        if limit is None:
            continue
        actual = report[metric]
        if actual is None or not accepted(actual, limit):
            failures.append(f"{metric}={actual} (required {direction} {limit})")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--sites", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--min-faint-survival", type=float)
    parser.add_argument("--min-median-peak-ratio", type=float)
    parser.add_argument("--min-median-flux-ratio", type=float)
    parser.add_argument("--max-median-spread-ratio", type=float)
    parser.add_argument("--max-median-colour-delta", type=float)
    args = parser.parse_args()
    try:
        report = compare(np.load(args.original, allow_pickle=False),
                         np.load(args.candidate, allow_pickle=False), json.loads(args.sites.read_text()))
    except (ValueError, KeyError, OSError) as error:
        parser.exit(2, f"{error}\n")
    failures = acceptance_failures(
        report, args.min_faint_survival, args.min_median_peak_ratio,
        args.min_median_flux_ratio, args.max_median_spread_ratio,
        args.max_median_colour_delta)
    if any(value is not None for value in (
            args.min_faint_survival, args.min_median_peak_ratio,
            args.min_median_flux_ratio, args.max_median_spread_ratio,
            args.max_median_colour_delta)):
        report["acceptance"] = {"pass": not failures, "failures": failures}
    args.out.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(json.dumps(report, allow_nan=False))
    if failures:
        parser.exit(1, "star-field acceptance failed\n")


if __name__ == "__main__":
    main()
