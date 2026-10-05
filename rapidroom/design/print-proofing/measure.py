"""Measurements behind the print soft-proofing design note (README.md, issue #122).

    python3 measure.py <printer profile.icc>

Calls LittleCMS 2 (the system liblcms2.so.2) through ctypes. Measures:
  1. how the printer gamut and sRGB overlap,
  2. how much the rendering intent, black point compensation and paper white change a proof,
  3. how close a proof transform baked into an N^3 LUT and sampled tetrahedrally
     (what a GPU pass would do) gets to LittleCMS itself, including a gamut flag,
  4. single-thread throughput of LittleCMS proofing and output transforms,
  5. whether the tificc command-line tool matches the library (the end-to-end reference).
Needs numpy; step 5 also needs tificc (liblcms2-utils) and tifffile.
"""

import ctypes as C
import logging
import os
import shutil
import subprocess
import sys
import tempfile
import time

import numpy as np

try:
    import tifffile

    logging.getLogger("tifffile").setLevel(logging.ERROR)
except ImportError:
    tifffile = None

lcms = C.CDLL("liblcms2.so.2")
lcms.cmsOpenProfileFromFile.restype = C.c_void_p
lcms.cmsOpenProfileFromFile.argtypes = [C.c_char_p, C.c_char_p]
lcms.cmsCreate_sRGBProfile.restype = C.c_void_p
lcms.cmsCreateLab4Profile.restype = C.c_void_p
lcms.cmsCreateLab4Profile.argtypes = [C.c_void_p]
lcms.cmsCreateTransform.restype = C.c_void_p
lcms.cmsCreateTransform.argtypes = [C.c_void_p, C.c_uint32, C.c_void_p, C.c_uint32, C.c_uint32, C.c_uint32]
lcms.cmsCreateProofingTransform.restype = C.c_void_p
lcms.cmsCreateProofingTransform.argtypes = [
    C.c_void_p, C.c_uint32, C.c_void_p, C.c_uint32, C.c_void_p, C.c_uint32, C.c_uint32, C.c_uint32,
]
lcms.cmsDoTransform.argtypes = [C.c_void_p, C.c_void_p, C.c_void_p, C.c_uint32]
lcms.cmsSetAlarmCodes.argtypes = [C.POINTER(C.c_uint16 * 16)]
lcms.cmsGetColorSpace.restype = C.c_uint32
lcms.cmsGetColorSpace.argtypes = [C.c_void_p]

TYPE_RGB_8 = 0x40019
TYPE_RGB_16 = 0x4001A
TYPE_RGB_DBL = 0x440018
TYPE_Lab_DBL = 0x4A0018
TYPE_CMYK_DBL = 0x460020
TYPE_CMYK_16 = 0x60022
SIG_CMYK = 0x434D594B
PERCEPTUAL, RELATIVE, ABSOLUTE = 0, 1, 3
F_BPC = 0x2000
F_SOFTPROOF = 0x4000
F_GAMUTCHECK = 0x1000
F_NOOPT = 0x0100
F_NOCACHE = 0x0040

SRGB = lcms.cmsCreate_sRGBProfile()
LAB = lcms.cmsCreateLab4Profile(None)


def run(xf, src, out_fmt_dtype, out_ch):
    src = np.ascontiguousarray(src)
    n = src.shape[0]
    out = np.zeros((n, out_ch), dtype=out_fmt_dtype)
    lcms.cmsDoTransform(xf, src.ctypes.data, out.ctypes.data, n)
    return out


def srgb_to_lab(rgb01):
    xf = lcms.cmsCreateTransform(SRGB, TYPE_RGB_DBL, LAB, TYPE_Lab_DBL, RELATIVE, F_NOOPT)
    return run(xf, rgb01.astype(np.float64), np.float64, 3)


def de2000(lab1, lab2):
    L1, a1, b1 = lab1.T
    L2, a2, b2 = lab2.T
    C1 = np.hypot(a1, b1)
    C2 = np.hypot(a2, b2)
    Cb = (C1 + C2) / 2
    G = 0.5 * (1 - np.sqrt(Cb**7 / (Cb**7 + 25.0**7)))
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
    dHp = 2 * np.sqrt(C1p * C2p) * np.sin(np.radians(dhp) / 2)
    Lbp = (L1 + L2) / 2
    Cbp = (C1p + C2p) / 2
    hsum = h1p + h2p
    hbp = np.where(np.abs(h1p - h2p) > 180,
                   np.where(hsum < 360, (hsum + 360) / 2, (hsum - 360) / 2), hsum / 2)
    hbp = np.where(C1p * C2p == 0, hsum, hbp)
    T = (1 - 0.17 * np.cos(np.radians(hbp - 30)) + 0.24 * np.cos(np.radians(2 * hbp))
         + 0.32 * np.cos(np.radians(3 * hbp + 6)) - 0.20 * np.cos(np.radians(4 * hbp - 63)))
    dtheta = 30 * np.exp(-(((hbp - 275) / 25) ** 2))
    Rc = 2 * np.sqrt(Cbp**7 / (Cbp**7 + 25.0**7))
    Sl = 1 + 0.015 * (Lbp - 50) ** 2 / np.sqrt(20 + (Lbp - 50) ** 2)
    Sc = 1 + 0.045 * Cbp
    Sh = 1 + 0.015 * Cbp * T
    Rt = -np.sin(np.radians(2 * dtheta)) * Rc
    return np.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2 + Rt * (dCp / Sc) * (dHp / Sh))


def tetra(lut, rgb01):
    """Tetrahedral interpolation of an N^3 LUT indexed lut[r, g, b] (same scheme as shader.wgsl)."""
    n = lut.shape[0]
    p = np.clip(rgb01, 0, 1) * (n - 1)
    i0 = np.minimum(np.floor(p).astype(int), n - 2)
    f = p - i0
    r, g, b = i0.T
    fr, fg, fb = f.T

    def at(dr, dg, db):
        return lut[r + dr, g + dg, b + db]

    c000, c111 = at(0, 0, 0), at(1, 1, 1)
    out = np.zeros((rgb01.shape[0], lut.shape[3]))
    conds = [
        (fr > fg) & (fg > fb),
        (fr > fg) & (fg <= fb) & (fr > fb),
        (fr > fg) & (fg <= fb) & (fr <= fb),
        (fr <= fg) & (fb > fg),
        (fr <= fg) & (fb <= fg) & (fb > fr),
        (fr <= fg) & (fb <= fg) & (fb <= fr),
    ]
    for k, m in enumerate(conds):
        if not m.any():
            continue
        R, G, B = fr[m][:, None], fg[m][:, None], fb[m][:, None]
        a0, a1 = c000[m], c111[m]
        if k == 0:
            c100, c110 = at(1, 0, 0)[m], at(1, 1, 0)[m]
            out[m] = (1 - R) * a0 + (R - G) * c100 + (G - B) * c110 + B * a1
        elif k == 1:
            c100, c101 = at(1, 0, 0)[m], at(1, 0, 1)[m]
            out[m] = (1 - R) * a0 + (R - B) * c100 + (B - G) * c101 + G * a1
        elif k == 2:
            c001, c101 = at(0, 0, 1)[m], at(1, 0, 1)[m]
            out[m] = (1 - B) * a0 + (B - R) * c001 + (R - G) * c101 + G * a1
        elif k == 3:
            c001, c011 = at(0, 0, 1)[m], at(0, 1, 1)[m]
            out[m] = (1 - B) * a0 + (B - G) * c001 + (G - R) * c011 + R * a1
        elif k == 4:
            c010, c011 = at(0, 1, 0)[m], at(0, 1, 1)[m]
            out[m] = (1 - G) * a0 + (G - B) * c010 + (B - R) * c011 + R * a1
        else:
            c010, c110 = at(0, 1, 0)[m], at(1, 1, 0)[m]
            out[m] = (1 - G) * a0 + (G - R) * c010 + (R - B) * c110 + B * a1
    return out


def stats(name, de):
    print(f"  {name:44s} mean {de.mean():.3f}  p99 {np.percentile(de, 99):.3f}  max {de.max():.3f}")


def main(path):
    printer = lcms.cmsOpenProfileFromFile(path.encode(), b"r")
    assert printer, path
    rng = np.random.default_rng(122)
    samples = rng.random((200_000, 3))

    print(f"profile: {path}")

    # 1. Gamut overlap.
    alarm = (C.c_uint16 * 16)(*([12345, 54321, 11111] + [0] * 13))
    lcms.cmsSetAlarmCodes(C.byref(alarm))
    grid = np.stack(np.meshgrid(*[np.linspace(0, 1, 33)] * 3, indexing="ij"), -1).reshape(-1, 3)
    g16 = np.round(grid * 65535).astype(np.uint16)
    xf = lcms.cmsCreateProofingTransform(
        SRGB, TYPE_RGB_16, SRGB, TYPE_RGB_16, printer, RELATIVE, RELATIVE,
        F_SOFTPROOF | F_GAMUTCHECK | F_NOCACHE,
    )
    out = run(xf, g16, np.uint16, 3)
    flagged = np.all(out == np.array([12345, 54321, 11111], dtype=np.uint16), axis=1)
    print(f"1. sRGB 33^3 grid colours flagged out of printer gamut: {flagged.mean() * 100:.1f}%")

    if lcms.cmsGetColorSpace(printer) == SIG_CMYK:
        device = np.stack(np.meshgrid(*[np.linspace(0, 100, 11)] * 4, indexing="ij"), -1).reshape(-1, 4)
        device_fmt, grid_name = TYPE_CMYK_DBL, "11^4 CMYK"
    else:
        device = grid
        device_fmt, grid_name = TYPE_RGB_DBL, "33^3 RGB"
    to_srgb = lcms.cmsCreateTransform(printer, device_fmt, SRGB, TYPE_RGB_DBL, RELATIVE, F_NOOPT)
    rgb = run(to_srgb, device.astype(np.float64), np.float64, 3)
    outside = np.any((rgb < -0.002) | (rgb > 1.002), axis=1)
    print(f"   printer {grid_name} grid colours outside sRGB (unbounded float): {outside.mean() * 100:.1f}%")

    # 2. Perceptual vs relative colorimetric through the printer.
    s16 = np.round(samples * 65535).astype(np.uint16)

    def proof(intent, proof_intent, flags, fmt=TYPE_RGB_16, src=s16, dtype=np.uint16):
        xf = lcms.cmsCreateProofingTransform(
            SRGB, fmt, SRGB, fmt, printer, intent, proof_intent, F_SOFTPROOF | flags,
        )
        return run(xf, src, dtype, 3)

    rel = proof(RELATIVE, RELATIVE, F_BPC) / 65535.0
    per = proof(PERCEPTUAL, RELATIVE, F_BPC) / 65535.0
    rel_nobpc = proof(RELATIVE, RELATIVE, 0) / 65535.0
    paper = proof(RELATIVE, ABSOLUTE, F_BPC) / 65535.0
    lab_src = srgb_to_lab(samples)
    lab_rel, lab_per = srgb_to_lab(rel), srgb_to_lab(per)
    print("2. dE2000 on 200k random sRGB colours:")
    stats("original vs proof (relative + BPC)", de2000(lab_src, lab_rel))
    stats("proof perceptual vs relative (both BPC)", de2000(lab_per, lab_rel))
    stats("proof relative: BPC on vs off", de2000(lab_rel, srgb_to_lab(rel_nobpc)))
    stats("proof relative vs paper-white simulation", de2000(lab_rel, srgb_to_lab(paper)))

    # 3. Baked LUT accuracy. Reference is LittleCMS 16-bit, unoptimised.
    print("3. Baked RGBA LUT (tetrahedral) vs LittleCMS 16-bit unoptimised proof, dE2000:")
    ref = proof(RELATIVE, RELATIVE, F_BPC | F_NOOPT) / 65535.0
    lab_ref = srgb_to_lab(ref)
    for n in (17, 33, 65):
        nodes = np.stack(np.meshgrid(*[np.linspace(0, 1, n)] * 3, indexing="ij"), -1).reshape(-1, 3)
        node_out = proof(RELATIVE, RELATIVE, F_BPC | F_NOOPT, src=np.round(nodes * 65535).astype(np.uint16)) / 65535.0
        lut = node_out.reshape(n, n, n, 3)
        got = tetra(lut, samples)
        de = de2000(lab_ref, srgb_to_lab(got))
        code = np.abs(np.round(got * 255) - np.round(ref * 255)).max()
        stats(f"{n}^3 float LUT (max 8-bit code diff {code:.0f})", de)
        if n == 33:
            got16 = tetra(lut.astype(np.float16).astype(np.float64), samples)
            stats("33^3 LUT stored as float16", de2000(lab_ref, srgb_to_lab(got16)))
            got8 = tetra(np.round(lut * 255) / 255, samples)
            stats("33^3 LUT stored as 8-bit", de2000(lab_ref, srgb_to_lab(got8)))
    lcms8 = proof(RELATIVE, RELATIVE, F_BPC, fmt=TYPE_RGB_8, src=np.round(samples * 255).astype(np.uint8), dtype=np.uint8) / 255.0
    stats("LittleCMS default 8-bit path (for scale)", de2000(lab_ref, srgb_to_lab(lcms8)))

    # Gamut flag carried in the LUT alpha channel, compared with LittleCMS's own gamut check.
    alarm_v = np.array([12345, 54321, 11111], dtype=np.uint16)
    xf = lcms.cmsCreateProofingTransform(
        SRGB, TYPE_RGB_16, SRGB, TYPE_RGB_16, printer, RELATIVE, RELATIVE,
        F_SOFTPROOF | F_GAMUTCHECK | F_NOCACHE | F_NOOPT,
    )
    ref_flag = np.all(run(xf, s16, np.uint16, 3) == alarm_v, axis=1)
    print(f"   gamut flag in the LUT alpha channel vs LittleCMS ({ref_flag.mean() * 100:.1f}% of samples flagged by LittleCMS):")
    for n in (33, 65):
        nodes = np.stack(np.meshgrid(*[np.linspace(0, 1, n)] * 3, indexing="ij"), -1).reshape(-1, 3)
        node_flag = np.all(run(xf, np.round(nodes * 65535).astype(np.uint16), np.uint16, 3) == alarm_v, axis=1)
        value = tetra(node_flag.astype(float).reshape(n, n, n, 1), samples)[:, 0]
        for threshold in (0.0, 0.5):
            got = value > threshold + 1e-9
            print(f"  {n}^3, flag > {threshold}: disagree {(got != ref_flag).mean() * 100:.2f}% "
                  f"(false warnings {(got & ~ref_flag).mean() * 100:.2f}%, missed {(~got & ref_flag).mean() * 100:.2f}%)")

    # 4. Throughput, one thread.
    print("4. LittleCMS proofing transform throughput (one thread):")
    for mp, fmt, dt, label in ((2, TYPE_RGB_8, np.uint8, "8-bit"), (24, TYPE_RGB_8, np.uint8, "8-bit"), (24, TYPE_RGB_16, np.uint16, "16-bit")):
        npx = mp * 1_000_000
        maxv = 255 if dt == np.uint8 else 65535
        buf = rng.integers(0, maxv + 1, (npx, 3), dtype=dt)
        for gamut in (0, F_GAMUTCHECK):
            xf = lcms.cmsCreateProofingTransform(
                SRGB, fmt, SRGB, fmt, printer, RELATIVE, RELATIVE, F_SOFTPROOF | F_BPC | gamut | F_NOCACHE,
            )
            t0 = time.perf_counter()
            run(xf, buf, dt, 3)
            dt_s = time.perf_counter() - t0
            print(f"  {mp:2d} MP {label}{' + gamut check' if gamut else '':15s} {dt_s * 1000:7.0f} ms  ({npx / dt_s / 1e6:.0f} MP/s)")
    dev_fmt, dev_ch = (TYPE_CMYK_16, 4) if lcms.cmsGetColorSpace(printer) == SIG_CMYK else (TYPE_RGB_16, 3)
    xf = lcms.cmsCreateTransform(SRGB, TYPE_RGB_16, printer, dev_fmt, RELATIVE, F_BPC)
    buf = rng.integers(0, 65536, (24_000_000, 3), dtype=np.uint16)
    t0 = time.perf_counter()
    run(xf, buf, np.uint16, dev_ch)
    print(f"  24 MP 16-bit sRGB -> printer device values      {(time.perf_counter() - t0) * 1000:7.0f} ms")

    # 5. The tificc command-line tool as an end-to-end reference.
    if shutil.which("tificc") is None or tifffile is None:
        print("5. skipped: needs tificc (liblcms2-utils) and the tifffile module")
        return
    print("5. tificc vs the library transform on a random 16-bit TIFF, max difference in 16-bit codes:")
    img = rng.integers(0, 65536, (256, 256, 3), dtype=np.uint16)
    with tempfile.TemporaryDirectory() as tmp:
        src, dst = os.path.join(tmp, "in.tif"), os.path.join(tmp, "out.tif")
        tifffile.imwrite(src, img, photometric="rgb")
        for flags, (intent, proof_intent, bpc) in (
            (["-t1", "-b", "-m1"], (RELATIVE, RELATIVE, F_BPC)),
            (["-t1", "-b", "-m3"], (RELATIVE, ABSOLUTE, F_BPC)),
            (["-t0", "-m1"], (PERCEPTUAL, RELATIVE, 0)),
        ):
            subprocess.run(["tificc", *flags, "-p" + path, "-w16", src, dst], check=True, capture_output=True)
            lib = proof(intent, proof_intent, bpc, src=img.reshape(-1, 3)).reshape(img.shape)
            diff = np.abs(tifffile.imread(dst).astype(int) - lib.astype(int)).max()
            print(f"  soft proof   tificc {' '.join(flags):14s} {diff}")
        subprocess.run(["tificc", "-t1", "-b", "-o" + path, "-w16", src, dst], check=True, capture_output=True)
        lib = run(xf, img.reshape(-1, 3), np.uint16, dev_ch)
        diff = np.abs(tifffile.imread(dst).reshape(-1, dev_ch).astype(int) - lib.astype(int)).max()
        print(f"  to printer   tificc -t1 -b -o<profile>  {diff}")


if __name__ == "__main__":
    main(sys.argv[1])
