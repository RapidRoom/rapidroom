"""Chosen-look workflow guards: cropping alone must not satisfy grading."""

import copy
import unittest

from mcp_skill import assert_chosen_variant


class ChosenLookTests(unittest.TestCase):
    def setUp(self):
        self.calls = [
            {"name": "mcp__rapidroom__render_compare", "input": {"variants": [
                {"name": "Neutral"}, {"name": "Warm", "changes": {"temperature": 20, "hsl": {"reds": {"saturation": -5}}}},
                {"name": "Muted", "changes": {"vibrance": -10}}]}},
            {"name": "mcp__rapidroom__update_adjustments", "input": {"expectedRevision": "crop-revision", "changes": {"temperature": 20, "hsl": {"reds": {"saturation": -5}}}}},
        ]
        self.final = {"adjustments": {"temperature": 20, "hsl": {"reds": {"saturation": -5, "hue": 0}}, "crop": {"x":500,"y":100,"width":1800,"height":2250}}}

    def test_chosen_tone_patch_is_applied_with_nested_merge_and_retained(self):
        self.assertEqual(assert_chosen_variant(self.calls, self.final)["temperature"], 20)

    def test_crop_only_edit_is_not_a_chosen_look(self):
        self.calls[1]["input"]["changes"] = {"crop": self.final["adjustments"]["crop"]}
        self.final["adjustments"]["temperature"] = 0
        with self.assertRaisesRegex(RuntimeError, "guarded mutation"):
            assert_chosen_variant(self.calls, self.final)

    def test_attempted_look_that_did_not_persist_fails(self):
        self.final["adjustments"]["hsl"]["reds"]["saturation"] = 0
        with self.assertRaisesRegex(RuntimeError, "Final state"):
            assert_chosen_variant(self.calls, self.final)

    def test_unguarded_or_ambiguous_choice_and_empty_patch_fail(self):
        for kind in ("unguarded", "duplicate", "empty"):
            calls = copy.deepcopy(self.calls)
            if kind == "unguarded":
                calls[1]["input"].pop("expectedRevision")
            elif kind == "duplicate":
                calls[0]["input"]["variants"].append({"name": "Warm", "changes": {"temperature": 10}})
            else:
                calls[0]["input"]["variants"][1]["changes"] = {}
            with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                assert_chosen_variant(calls, self.final)


if __name__ == "__main__":
    unittest.main()
