#!/usr/bin/env python3
"""CC0-1.0 original synthetic progressive JPEG fixture (Pillow)."""
from pathlib import Path
from PIL import Image
image = Image.new('RGB', (31, 23))
image.putdata([((x * 17) % 256, (y * 23) % 256, ((x + y) * 31) % 256)
               for y in range(23) for x in range(31)])
image.save(Path(__file__).with_name('progressive.jpg'), quality=87, progressive=True)
