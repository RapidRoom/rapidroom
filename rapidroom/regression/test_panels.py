import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import numpy as np
from PIL import Image

import panels


class PanelTests(unittest.TestCase):
    def test_before_and_after_are_not_swapped(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = root / "before.tiff", root / "after.tiff"
            before.touch()
            after.touch()
            def read(path):
                color = [65535, 0, 0] if Path(path) == before else [0, 0, 65535]
                return np.array([[color]], dtype=np.uint16), {}
            row = {"set": "neutral", "id": "sample", "problem": "pixels differ", "ref_output": str(before), "test_output": str(after)}
            with patch.object(panels.baseline, "read_tiff", side_effect=read):
                names = panels.render_panels({"images": [row]}, root / "panels")
            with Image.open(root / "panels" / names[0]) as image:
                self.assertEqual(image.getpixel((0, 30)), (255, 0, 0))
                self.assertEqual(image.getpixel((800, 30)), (0, 0, 255))

    def test_missing_before_evidence_fails(self):
        with tempfile.TemporaryDirectory() as folder:
            row = {"set": "neutral", "id": "sample", "problem": "pixels differ"}
            with self.assertRaisesRegex(ValueError, "Missing ref_output"):
                panels.render_panels({"images": [row]}, Path(folder))

    def test_identical_rows_do_not_make_false_diff_panels(self):
        with tempfile.TemporaryDirectory() as folder:
            self.assertEqual(panels.render_panels({"images": [{"set": "neutral", "id": "sample"}]}, Path(folder)), [])
