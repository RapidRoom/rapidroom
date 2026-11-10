import unittest
import numpy as np
from star_field import acceptance_failures, compare


class StarFieldTests(unittest.TestCase):
    def setUp(self):
        self.sites = [{"x": 20, "y": 20, "faint": True}, {"x": 44, "y": 44, "faint": False}]
        y, x = np.mgrid[:65, :65]
        level = np.full((65, 65), 0.002)
        for site, amplitude in zip(self.sites, [0.01, 0.2]):
            level += amplitude * np.exp(-((x-site['x'])**2 + (y-site['y'])**2) / 2)
        self.image = np.repeat(level[:, :, None], 3, axis=2)

    def test_identity_preserves_faint_flux_peak_spread_and_colour(self):
        result = compare(self.image, self.image, self.sites)
        self.assertEqual(result['faint_survival_fraction'], 1)
        for metric in ['median_peak_ratio', 'median_flux_ratio', 'median_spread_ratio']:
            self.assertAlmostEqual(result[metric], 1)
        self.assertEqual(result['median_aperture_colour_delta'], 0)

    def test_reduced_signal_detects_lost_faint_star(self):
        candidate = 0.002 + (self.image - 0.002) * 0.25
        result = compare(self.image, candidate, self.sites)
        self.assertEqual(result['faint_survival_fraction'], 0)
        self.assertAlmostEqual(result['median_flux_ratio'], 0.25)
        self.assertAlmostEqual(result['median_peak_ratio'], 0.25)

    def test_spread_and_colour_changes_are_measured(self):
        y, x = np.mgrid[:65, :65]
        level = np.full((65, 65), 0.002)
        for site, amplitude in zip(self.sites, [0.01, 0.2]):
            level += amplitude * np.exp(-((x-site['x'])**2 + (y-site['y'])**2) / 8)
        candidate = np.repeat(level[:, :, None], 3, axis=2)
        candidate[:, :, 0] *= 1.2
        result = compare(self.image, candidate, self.sites)
        self.assertGreater(result['median_spread_ratio'], 1.5)
        self.assertGreater(result['median_aperture_colour_delta'], 0.1)

    def test_acceptance_thresholds_pass_fail_and_reject_missing_measurements(self):
        result = compare(self.image, self.image, self.sites)
        thresholds = {
            'min_faint_survival': 1,
            'min_median_peak_ratio': 0.5,
            'min_median_flux_ratio': 0.5,
            'max_median_spread_ratio': 1.5,
            'max_median_colour_delta': 0.05,
        }
        self.assertEqual(acceptance_failures(result, **thresholds), [])
        candidate = 0.002 + (self.image - 0.002) * 0.25
        reduced = compare(self.image, candidate, self.sites)
        failures = acceptance_failures(reduced, **thresholds)
        self.assertEqual(len(failures), 3)
        self.assertTrue(any('faint_survival_fraction' in failure for failure in failures))
        self.assertTrue(any('median_peak_ratio' in failure for failure in failures))
        self.assertTrue(any('median_flux_ratio' in failure for failure in failures))
        self.assertEqual(acceptance_failures(result), [])
        self.assertIn('None', acceptance_failures(
            {'faint_survival_fraction': None}, min_faint_survival=0.5)[0])
        self.assertIn('None', acceptance_failures(
            {'median_spread_ratio': None}, max_median_spread_ratio=1.5)[0])

    def test_no_wraparound_duplicate_or_shape_mismatch(self):
        for sites in [[{'x': 2, 'y': 20, 'faint': True}], self.sites * 2]:
            with self.assertRaises(ValueError):
                compare(self.image, self.image, sites)
        with self.assertRaises(ValueError):
            compare(self.image, self.image[:-1], self.sites)
        bad = self.image.copy(); bad[0, 0, 0] = np.nan
        with self.assertRaises(ValueError):
            compare(self.image, bad, self.sites)


if __name__ == '__main__':
    unittest.main()
