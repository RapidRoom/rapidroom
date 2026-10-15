#!/usr/bin/env python3
"""CC0-1.0: authored grayscale edges; generate with libjxl cjxl 0.12.0."""
from pathlib import Path
import subprocess
import tempfile
root = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory() as directory:
    pgm = Path(directory) / 'edges.pgm'
    pixels = bytes(255 if (x > 31 or (x - 16) ** 2 + (y - 32) ** 2 < 100) else 0
                   for y in range(64) for x in range(64))
    pgm.write_bytes(b'P5\n64 64\n255\n' + pixels)
    subprocess.run(['cjxl', str(pgm), str(root / 'lossy-edges.jxl'),
                    '--distance=1', '--effort=3', '--num_threads=1'], check=True)
