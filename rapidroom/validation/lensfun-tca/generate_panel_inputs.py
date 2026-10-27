#!/usr/bin/env python3
"""Generate synthetic optical RGB using independent Lensfun reverse coordinates."""
import argparse
import ctypes as C
import fcntl
import hashlib
import json
from pathlib import Path
import shutil

import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
out = args.out.resolve()
assert 'samples' in out.parts
out.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parent
# The numeric reference helper binds the installed public C API.
import generate_reference as reference
lib = reference.lib
P, F = C.c_void_p, C.c_float
fixture = root / 'fixture.xml'
db = lib.lf_db_new()
assert lib.lf_db_load_file(db, str(fixture).encode()) == 0
lens = lib.lf_db_get_lenses(db)[0]
w, h = 1200, 800


def ideal(x, y, channel):
    dx = (x - 20 + 30) % 60 - 30
    dy = (y - 20 + 30) % 60 - 30
    line = np.maximum(np.exp(-dx*dx / 1.3), np.exp(-dy*dy / 1.3))
    lx, ly = (x - 43 + 40) % 80 - 40, (y - 47 + 40) % 80 - 40
    led = np.exp(-(lx*lx + ly*ly) / 9)
    return np.clip(.025 + .68*line + [.25, .16, .06][channel]*led, 0, 1)


y, x = np.mgrid[0:h, 0:w]
ideal_image = np.stack([ideal(x, y, c) for c in range(3)], axis=2).astype('<f4')
records = []
for focal in [20, 70, 200]:
    with open('/tmp/rapidroom-gpu.lock', 'a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if shutil.disk_usage('/home').free < 25*1024**3:
            raise RuntimeError('At least 25 GiB free required')
        modifier = lib.lf_modifier_new(lens, 1, w, h)
        assert lib.lf_modifier_initialize(modifier, lens, 3, focal, 4, 10, 1, 1, 1, 1) & 1
        field = np.empty((h, w, 3), dtype='<f4')
        coords = (F * (w*6))()
        for row in range(h):
            assert lib.lf_modifier_apply_subpixel_distortion(modifier, 0, row, w, 1, coords)
            uv = np.ctypeslib.as_array(coords).reshape(w, 6)
            for channel in range(3):
                field[row, :, channel] = ideal(uv[:, channel*2], uv[:, channel*2+1], channel)
        lib.lf_modifier_destroy(modifier)
        filename = out / f'{focal}mm-linear-rgb.f32'
        filename.write_bytes(field.tobytes())
        records.append(dict(focal=focal, file=filename.name, sha256=hashlib.sha256(filename.read_bytes()).hexdigest()))
        print(filename, flush=True)
lib.lf_db_destroy(db)
ideal_file = out / 'ideal-linear-rgb.f32'
ideal_file.write_bytes(ideal_image.tobytes())
(out / 'inputs.json').write_text(json.dumps(dict(synthetic=True, real_lens=False, width=w, height=h,
    encoding='little-endian float32 interleaved linear RGB', library=reference.version,
    fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
    ideal_file=ideal_file.name, ideal_sha256=hashlib.sha256(ideal_file.read_bytes()).hexdigest(), records=records), indent=2)+'\n')
