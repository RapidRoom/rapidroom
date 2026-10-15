"""Meaningful fixture/metric controls; no production GPU/estimator validation."""

import math
import unittest

from fixtures import decode, encode, fixture, from_ycc, metrics_in_both_domains, residual_metrics, ycc


class FixtureControls(unittest.TestCase):
    def test_transfer_and_ycc_round_trip_including_float_headroom(self):
        for value in (-0.01, 0.0, 0.001, 0.04, 0.5, 1.0, 1.2):
            self.assertAlmostEqual(decode(encode(value)), value, places=6)
        rgb = (0.2, 0.5, 0.8)
        for actual, expected in zip(from_ycc(*ycc(rgb)), rgb):
            self.assertAlmostEqual(actual, expected, places=12)

    def test_constant_bias_is_not_random_noise(self):
        reference = [(0.3, 0.3, 0.3)] * 100
        metric = residual_metrics([(0.4, 0.4, 0.4)] * 100, reference, 'linear')['y_cb_cr'][0]
        self.assertAlmostEqual(metric['mean'], 0.1)
        self.assertAlmostEqual(metric['mse'], 0.01)
        self.assertAlmostEqual(metric['variance'], 0.0)
        self.assertAlmostEqual(metric['mad_gaussian_scale'], 0.0)

    def test_clipped_excursions_can_have_zero_mad_but_positive_variance(self):
        clean = [(0.0, 0.0, 0.0)] * 100
        output = [(0.0, 0.0, 0.0)] * 90 + [(0.1, 0.1, 0.1)] * 10
        result = residual_metrics(output, clean, 'encoded')['y_cb_cr'][0]
        self.assertEqual(result['mad_gaussian_scale'], 0)
        self.assertGreater(result['variance'], 0)
        self.assertAlmostEqual(result['mse'], result['variance'] + result['mean'] ** 2)

    def test_matched_linear_encoded_pairs_have_same_encoded_reference(self):
        a = metrics_in_both_domains(fixture(domain='encoded'))
        b = metrics_in_both_domains(fixture(domain='linear'))
        for first, second in zip(a['encoded']['y_cb_cr'], b['encoded']['y_cb_cr']):
            self.assertAlmostEqual(first['variance'], second['variance'], places=12)
        self.assertNotAlmostEqual(a['encoded']['y_cb_cr'][0]['variance'],
                                  a['linear']['y_cb_cr'][0]['variance'], places=6)

    def test_white_noise_reference_matches_injected_component_variance(self):
        pair = fixture(sigma=0.02)
        result = residual_metrics(pair['noisy'], pair['clean'], 'encoded')['y_cb_cr'][0]
        expected = 0.02**2 * sum(v * v for v in (0.2126, 0.7152, 0.0722))
        self.assertLess(abs(result['variance'] / expected - 1), 0.05)

    def test_quantized_noise_is_unresolved_instead_of_proven_absent(self):
        pair = fixture(sigma=1e-8, brightness=128 / 255, bits=8)
        self.assertEqual(pair['clean'], pair['noisy'])
        self.assertGreater(pair['construction']['sigma_encoded_rgb'], 0)

    def test_known_detail_corruption_is_detected(self):
        for scene in ('texture-1-8', 'line-2-1', 'stars', 'led'):
            pair = fixture(scene, sigma=0)
            self.assertEqual(pair['clean'], pair['noisy'])
            # A deliberately destructive constant output must fail clean-detail
            # metrics even though it has no visible random variation itself.
            result = residual_metrics([(0.5, 0.5, 0.5)] * len(pair['clean']),
                                      pair['clean'], 'encoded')
            self.assertGreater(sum(c['mse'] for c in result['y_cb_cr']), 0)

    def test_correlation_preserves_blocks_and_seed_changes_data(self):
        pair = fixture(correlation=4)
        self.assertEqual(pair['noisy'][0], pair['noisy'][3])
        self.assertNotEqual(pair['noisy'][0], pair['noisy'][4])
        self.assertNotEqual(pair['noisy'], fixture(correlation=4, seed=619)['noisy'])

    def test_invalid_metrics_fail_closed(self):
        for value in (math.nan, math.inf):
            with self.assertRaises(ValueError):
                residual_metrics([(value, 0, 0)], [(0, 0, 0)], 'linear')
        with self.assertRaises(ValueError):
            residual_metrics([], [], 'encoded')
        with self.assertRaises(ValueError):
            fixture(domain='unknown')


if __name__ == '__main__':
    unittest.main()
