"""Decode Lightroom's HDR DNG (DNG 1.7, LinearRaw, float16 JPEG XL tiles) with libjxl via ctypes,
convert camera-native RGB to linear sRGB (AsShotNeutral + ForwardMatrix2 + Bradford D50->D65),
and write a 2x2-downsampled f32 dump matching the harness format.

usage: lr_dng.py <dng> <out_dir>
"""
import ctypes
import json
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np

lib = ctypes.CDLL("libjxl.so.0.11")
lib.JxlDecoderCreate.restype = ctypes.c_void_p
lib.JxlDecoderCreate.argtypes = [ctypes.c_void_p]
lib.JxlDecoderDestroy.argtypes = [ctypes.c_void_p]
lib.JxlDecoderSubscribeEvents.argtypes = [ctypes.c_void_p, ctypes.c_int]
lib.JxlDecoderSetInput.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]
lib.JxlDecoderCloseInput.argtypes = [ctypes.c_void_p]
lib.JxlDecoderProcessInput.argtypes = [ctypes.c_void_p]


class PixelFormat(ctypes.Structure):
    _fields_ = [("num_channels", ctypes.c_uint32), ("data_type", ctypes.c_int),
                ("endianness", ctypes.c_int), ("align", ctypes.c_size_t)]


class BasicInfo(ctypes.Structure):
    _fields_ = [("have_container", ctypes.c_int), ("xsize", ctypes.c_uint32), ("ysize", ctypes.c_uint32),
                ("bits_per_sample", ctypes.c_uint32), ("exponent_bits_per_sample", ctypes.c_uint32),
                ("intensity_target", ctypes.c_float), ("min_nits", ctypes.c_float),
                ("relative_to_max_display", ctypes.c_int), ("linear_below", ctypes.c_float),
                ("uses_original_profile", ctypes.c_int), ("have_preview", ctypes.c_int),
                ("have_animation", ctypes.c_int), ("orientation", ctypes.c_int),
                ("num_color_channels", ctypes.c_uint32), ("num_extra_channels", ctypes.c_uint32),
                ("alpha_bits", ctypes.c_uint32), ("alpha_exponent_bits", ctypes.c_uint32),
                ("alpha_premultiplied", ctypes.c_int), ("preview", ctypes.c_uint32 * 2),
                ("animation", ctypes.c_uint32 * 4), ("intrinsic_xsize", ctypes.c_uint32),
                ("intrinsic_ysize", ctypes.c_uint32), ("padding", ctypes.c_uint8 * 100)]


lib.JxlDecoderGetBasicInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(BasicInfo)]
lib.JxlDecoderImageOutBufferSize.argtypes = [ctypes.c_void_p, ctypes.POINTER(PixelFormat), ctypes.POINTER(ctypes.c_size_t)]
lib.JxlDecoderSetImageOutBuffer.argtypes = [ctypes.c_void_p, ctypes.POINTER(PixelFormat), ctypes.c_void_p, ctypes.c_size_t]

JXL_DEC_SUCCESS, JXL_DEC_ERROR, JXL_DEC_NEED_MORE_INPUT = 0, 1, 2
JXL_DEC_NEED_IMAGE_OUT_BUFFER = 5
JXL_DEC_BASIC_INFO, JXL_DEC_FULL_IMAGE = 0x40, 0x1000
INFO = {}


def decode_tile(data):
    dec = lib.JxlDecoderCreate(None)
    try:
        assert lib.JxlDecoderSubscribeEvents(dec, JXL_DEC_BASIC_INFO | JXL_DEC_FULL_IMAGE) == 0
        buf = ctypes.create_string_buffer(data, len(data))
        lib.JxlDecoderSetInput(dec, buf, len(data))
        lib.JxlDecoderCloseInput(dec)
        fmt = PixelFormat(3, 0, 0, 0)  # 3 channels, JXL_TYPE_FLOAT, native endian
        info = BasicInfo()
        out = None
        while True:
            st = lib.JxlDecoderProcessInput(dec)
            if st == JXL_DEC_BASIC_INFO:
                assert lib.JxlDecoderGetBasicInfo(dec, ctypes.byref(info)) == 0
                INFO.setdefault("bits", (info.bits_per_sample, info.exponent_bits_per_sample, info.uses_original_profile))
            elif st == JXL_DEC_NEED_IMAGE_OUT_BUFFER:
                size = ctypes.c_size_t()
                assert lib.JxlDecoderImageOutBufferSize(dec, ctypes.byref(fmt), ctypes.byref(size)) == 0
                out = np.empty(size.value // 4, dtype=np.float32)
                assert lib.JxlDecoderSetImageOutBuffer(dec, ctypes.byref(fmt), out.ctypes.data, size.value) == 0
            elif st == JXL_DEC_FULL_IMAGE:
                pass
            elif st == JXL_DEC_SUCCESS:
                return out.reshape(info.ysize, info.xsize, 3)
            else:
                raise RuntimeError(f"libjxl status {st}")
    finally:
        lib.JxlDecoderDestroy(dec)


def read_tiff(b):
    u16 = lambda o: struct.unpack_from("<H", b, o)[0]
    u32 = lambda o: struct.unpack_from("<I", b, o)[0]

    def ifd(off):
        tags = {}
        for i in range(u16(off)):
            e = off + 2 + 12 * i
            tags[u16(e)] = (u16(e + 2), u32(e + 4), e + 8)
        return tags

    def vals(tags, tag):
        ty, n, pos = tags[tag]
        size = 2 if ty == 3 else 4
        base = u32(pos) if size * n > 4 else pos
        return [u16(base + 2 * i) if size == 2 else u32(base + 4 * i) for i in range(n)]

    ifd0 = ifd(u32(4))
    for off in vals(ifd0, 330):
        t = ifd(off)
        if vals(t, 254)[0] == 0 and vals(t, 277)[0] == 3:
            return t, vals
    raise RuntimeError("no full-resolution SubIFD")


def main(dng, out_dir):
    b = Path(dng).read_bytes()
    t, vals = read_tiff(b)
    w, h = vals(t, 256)[0], vals(t, 257)[0]
    tw, th = vals(t, 322)[0], vals(t, 323)[0]
    across = -(-w // tw)
    cam = np.zeros((h, w, 3), dtype=np.float32)
    for i, (off, n) in enumerate(zip(vals(t, 324), vals(t, 325))):
        tile = decode_tile(b[off:off + n])
        x0, y0 = (i % across) * tw, (i // across) * th
        hh, ww = min(tile.shape[0], h - y0), min(tile.shape[1], w - x0)
        cam[y0:y0 + hh, x0:x0 + ww] = tile[:hh, :ww]
    meta = json.loads(subprocess.check_output(
        ["exiftool", "-j", "-n", "-IFD0:AsShotNeutral", "-IFD0:ForwardMatrix2", "-IFD0:BaselineExposure", dng]))[0]
    neutral = np.array([float(x) for x in str(meta["AsShotNeutral"]).split()], dtype=np.float32)
    fm = np.array([float(x) for x in str(meta["ForwardMatrix2"]).split()], dtype=np.float32).reshape(3, 3)
    bradford_d50_d65 = np.array([[0.9555766, -0.0230393, 0.0631636],
                                 [-0.0282895, 1.0099416, 0.0210077],
                                 [0.0122982, -0.0204830, 1.3299098]], dtype=np.float32)
    xyz_to_srgb = np.array([[3.2404542, -1.5371385, -0.4985314],
                            [-0.9692660, 1.8760108, 0.0415560],
                            [0.0556434, -0.2040259, 1.0572252]], dtype=np.float32)
    m = xyz_to_srgb @ bradford_d50_d65 @ fm
    lin = (cam / neutral[None, None, :]) @ m.T
    half = lin[: h // 2 * 2, : w // 2 * 2].reshape(h // 2, 2, w // 2, 2, 3).mean(axis=(1, 3)).astype("<f4")
    out = Path(out_dir)
    half.tofile(out / "lr-linear.f32")
    (out / "lr-linear.json").write_text(json.dumps({"width": w // 2, "height": h // 2, "channels": 3}))
    print(json.dumps({"dng": Path(dng).name, "size": [w, h], "tile": [tw, th], "jxl_bits": INFO.get("bits"),
                      "camera_min": float(cam.min()), "camera_max": float(cam.max()),
                      "camera_p50": float(np.median(cam)), "baseline_exposure": meta.get("BaselineExposure"),
                      "neutral": neutral.tolist()}))


main(sys.argv[1], sys.argv[2])
