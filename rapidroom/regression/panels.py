#!/usr/bin/env python3
"""Before/after evidence for failed pixel comparisons; never modifies a reference."""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

import baseline


def render_panels(summary, out):
    out.mkdir(parents=True, exist_ok=True)
    panels = []
    for row in summary["images"]:
        if "problem" not in row:
            continue
        pair = []
        for side in ("ref_output", "test_output"):
            source = row.get(side)
            if not source or not Path(source).is_file():
                raise ValueError(f"Missing {side} for {row['set']}/{row['id']}")
            pixels, _ = baseline.read_tiff(source)
            image = Image.fromarray((pixels >> 8).astype(np.uint8))
            image.thumbnail((800, 600))
            pair.append(image)
        panel = Image.new("RGB", (1600, 630), "#202020")
        draw = ImageDraw.Draw(panel)
        for index, image in enumerate(pair):
            panel.paste(image, (index * 800, 30))
            draw.text((index * 800 + 8, 8), "Checked base (before)" if index == 0 else "Candidate (after)", fill="white")
        name = f"{row['set']}__{row['id']}.png"
        if Path(name).name != name:
            raise ValueError("Invalid panel filename")
        panel.save(out / name)
        panels.append(name)
    (out / "panels.json").write_text(json.dumps({"panels": panels}, indent=2) + "\n")
    return panels


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("summary", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    render_panels(json.loads(args.summary.read_text()), args.out)
