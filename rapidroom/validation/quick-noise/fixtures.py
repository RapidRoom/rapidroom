"""Paired synthetic inputs and reference metrics, never an app noise estimator.

AGPL-3.0. Adapted fixture families, YCbCr conversion and centered residual
metrics from RustRunner/RapidRAW-MKII@555f7c6ddd5b6b5d219f87e3552a334968e9ddd9,
src-tauri/src/gpu_processing/denoise_tests/qualification.rs (RustRunner).
Counter-based Gaussian generation, clipping/quantization and star/LED cases
are original RapidRoom test support. See README.md for scope and provenance.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics


def encode(value):
    return 12.92 * value if value <= 0.0031308 else 1.055 * value ** (1 / 2.4) - 0.055


def decode(value):
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def ycc(rgb):
    r, g, b = rgb
    y = 0.2126 * r + 0.7152 * g + 0.0722 * b
    return y, 0.565 * (b - y), 0.713 * (r - y)


def from_ycc(y, cb, cr):
    r, b = y + cr / 0.713, y + cb / 0.565
    return r, (y - 0.2126 * r - 0.0722 * b) / 0.7152, b


def gaussian(seed, x, y, channel):
    # Open-interval uniforms from independently addressed counters. No RNG state
    # leaks between fixture order, clean/noisy generation, or component channels.
    raw = hashlib.sha256(f'{seed}:{x}:{y}:{channel}'.encode('ascii')).digest()
    u, v = ((int.from_bytes(raw[i:i + 8], 'big') + 0.5) / 2**64 for i in (0, 8))
    return math.sqrt(-2 * math.log(u)) * math.cos(2 * math.pi * v)


def fixture(scene='flat', domain='encoded', seed=173, width=128, height=128,
            brightness=128 / 255, sigma=0.02, correlation=1, clip=False, bits=0):
    """Return clean/noisy RGB pairs and measured residuals in both domains.

    Noise is injected into encoded RGB before the optional endpoint clipping
    and integer quantization, identically applied to both pair members. Linear
    fixtures decode that result; they are developed RGB, not CFA/sensor noise.
    Coarse shared Gaussian blocks intentionally add spatial correlation.
    """
    if domain not in ('linear', 'encoded') or width < 8 or height < 8:
        raise ValueError('Unsupported domain or dimensions')
    if bits not in (0, 8, 16) or correlation not in (1, 2, 4, 8):
        raise ValueError('Unsupported quantization or correlation')
    if not math.isfinite(brightness) or not math.isfinite(sigma) or sigma < 0:
        raise ValueError('Nonfinite input or negative sigma')
    clean, noisy = [], []
    for y in range(height):
        for x in range(width):
            value = [brightness, 0.0, 0.0]
            if scene == 'edge':
                value[0] += (-10 if x < width // 2 else 10) / 255
            elif scene.startswith('texture-'):
                _, channel, period = scene.split('-')
                value[int(channel)] += 6 / 255 * math.sin(x * 2 * math.pi / int(period))
            elif scene.startswith('line-'):
                _, channel, line_width = scene.split('-')
                if width // 2 <= x < width // 2 + int(line_width):
                    value[int(channel)] += 12 / 255
            elif scene.startswith('gradient-'):
                value[int(scene[-1])] += 12 / 255 * (x / (width - 1) - 0.5)
            elif scene in ('stars', 'led'):
                if (x, y) in ((width // 3, height // 3), (width // 2, height // 2)):
                    value = [0.9, 0.0, 0.0] if scene == 'stars' else [0.4, 0.15, -0.08]
            elif scene != 'flat':
                raise ValueError('Unknown scene')
            base = from_ycc(*value)
            perturbation = [sigma * gaussian(seed, x // correlation, y // correlation, c)
                            for c in range(3)]

            def store(rgb):
                if clip or bits:
                    rgb = [max(0.0, min(1.0, v)) for v in rgb]
                if bits:
                    top = 2**bits - 1
                    rgb = [round(v * top) / top for v in rgb]
                return tuple(decode(v) for v in rgb) if domain == 'linear' else tuple(rgb)

            clean.append(store(base))
            noisy.append(store([v + n for v, n in zip(base, perturbation)]))
    return {'width': width, 'height': height, 'domain': domain, 'clean': clean, 'noisy': noisy,
            'construction': {'scene': scene, 'seed': seed, 'sigma_encoded_rgb': sigma,
                             'brightness_encoded_y': brightness, 'correlation_block': correlation,
                             'clip': clip or bool(bits), 'bits': bits}}


def residual_metrics(output, clean, domain):
    """Reference-only ground truth: centered variance != MSE != Gaussian MAD.

    Values use normalized domain units. There is no fitted plane or estimator
    qualification here: ground truth is available because the input is paired.
    """
    if domain not in ('linear', 'encoded') or len(output) != len(clean) or not clean:
        raise ValueError('Invalid metric domain or pair length')
    errors = [[], [], []]
    for actual, reference in zip(output, clean):
        if len(actual) != 3 or len(reference) != 3 or not all(
                math.isfinite(v) for v in (*actual, *reference)):
            raise ValueError('Metrics require finite RGB triples')
        for component, a, b in zip(errors, ycc(actual), ycc(reference)):
            component.append(a - b)
    result = []
    for values in errors:
        mean, center = statistics.fmean(values), statistics.median(values)
        result.append({'mean': mean, 'variance': statistics.pvariance(values),
                       'mse': statistics.fmean(v * v for v in values),
                       'mad_gaussian_scale': statistics.median(abs(v - center) for v in values)
                       / 0.6744897501960817})
    return {'domain': domain, 'count': len(clean), 'y_cb_cr': result}


def metrics_in_both_domains(pair):
    result = {}
    for domain in ('encoded', 'linear'):
        transform = (lambda v: v) if domain == pair['domain'] else (
            encode if domain == 'encoded' else decode)
        result[domain] = residual_metrics(
            [tuple(transform(v) for v in pixel) for pixel in pair['noisy']],
            [tuple(transform(v) for v in pixel) for pixel in pair['clean']], domain)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    # These seeds are construction/control seeds. A later implementation must
    # reserve separate, untouched release holdouts after fixing its method.
    cases = []
    scenes = ['flat', 'edge', 'stars', 'led']
    scenes += [f'texture-{c}-{p}' for c in range(3) for p in (4, 8, 16)]
    scenes += [f'line-{c}-{w}' for c in (1, 2) for w in (1, 2, 4)]
    scenes += ['gradient-1', 'gradient-2']
    for domain in ('encoded', 'linear'):
        for scene in scenes:
            pair = fixture(scene, domain, brightness=0.02 if scene in ('stars', 'led') else 128 / 255)
            cases.append({'construction': pair['construction'], 'source_domain': domain,
                          'metrics': metrics_in_both_domains(pair)})
        for brightness, bits, correlation in ((0.0, 0, 1), (0.02, 8, 1), (0.5, 16, 4)):
            pair = fixture(domain=domain, brightness=brightness, bits=bits,
                           correlation=correlation, clip=True)
            cases.append({'construction': pair['construction'], 'source_domain': domain,
                          'metrics': metrics_in_both_domains(pair)})
    with args.out.open('x') as stream:
        json.dump({'schema_version': 1, 'scope': 'fixture references only; no shader/estimator run',
                   'cases': cases}, stream, indent=2, allow_nan=False)
        stream.write('\n')


if __name__ == '__main__':
    main()
