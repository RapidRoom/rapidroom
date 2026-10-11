"""Regressions for the real native palette screenshot comparator."""
import unittest
from PIL import Image, ImageDraw
from palette import _photo_only, _require_visible_photo


class PalettePhotoChecks(unittest.TestCase):
    def test_exclusion_edges_cannot_make_a_solid_preview_valid(self):
        image, visible = _photo_only(Image.new('RGB', (768,512), 220),
          (336,146,1104,658), (310,8,1130,342))
        with self.assertRaisesRegex(RuntimeError, 'blank or untextured'):
            _require_visible_photo(image, visible)

    def test_dialog_covering_photo_cannot_validate_preview(self):
        image, visible = _photo_only(Image.new('RGB', (32,32), 220),
          (0,0,32,32), (-100,-100,100,100))
        with self.assertRaisesRegex(RuntimeError, 'insufficient unobscured'):
            _require_visible_photo(image, visible)

    def test_texture_hidden_behind_dialog_cannot_validate_blank_visible_photo(self):
        original = Image.new('RGB', (100,100), 220)
        ImageDraw.Draw(original).rectangle((0,0,49,49), fill=20)
        image, visible = _photo_only(original, (0,0,100,100), (0,0,50,50))
        with self.assertRaisesRegex(RuntimeError, 'blank or untextured'):
            _require_visible_photo(image, visible)

    def test_real_texture_in_visible_photo_is_valid(self):
        original = Image.new('RGB', (100,100), 220)
        ImageDraw.Draw(original).rectangle((50,50,99,99), fill=20)
        image, visible = _photo_only(original, (0,0,100,100), (0,0,25,25))
        _require_visible_photo(image, visible)


if __name__ == '__main__':
    unittest.main()
