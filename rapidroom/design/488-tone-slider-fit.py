#!/usr/bin/env python3
"""Check the proposed #488 tone-slider law against a J40 comparison JSON.

The JSON is the study's comparison file. It must contain
photos["rr-j40-grey-ramp"].engines.{rapidroom,lightroom}.ramp_steps.
Pass that path as the only argument. The script prints the ramp ratios and
the migration search. It does not render and it does not change a sidecar.

Coefficients are the design values in 488-tone-sliders.md. Rounding is four
decimal places, which is what the note publishes.
"""

import json
import sys
from pathlib import Path

import numpy as np

# Ap, An, g_pos, g_neg, t0, p, q. Slider s = value/100.
# delta_t = sign_amplitude * |s|^g * max(t - t0, 0)^p / (1 + t)^q
FITS = {
    "highlights": (5.3936, 6.0134, 0.9184, 1.0243, -0.1437, 5.2500, 4.5634),
    "shadows": (5.5057, 3.8280, 1.2969, 1.0059, 0.0437, 0.8285, 10.9305),
    "whites": (3.9336, 1.8512, 1.9202, 0.6126, 0.0024, 2.8656, 4.0217),
    "blacks": (2.6903, 6.8315, 1.0891, 1.6427, 0.0448, 0.9347, 9.9743),
}
HOLD_LINEAR = 0.36
SLIDERS = (-100, -50, 50, 100)


def smoothstep(edge0, edge1, value):
    t = np.clip((value - edge0) / (edge1 - edge0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def encode_luma(linear):
    """Raw default output: sRGB, power 1/1.1, then 0.25 identity + 0.75 smoothstep."""
    channel = np.clip(np.asarray(linear, dtype=np.float64), 0.0, 1.0)
    encoded = np.where(
        channel <= 0.0031308,
        channel * 12.92,
        1.055 * np.power(channel, 1.0 / 2.4) - 0.055,
    )
    encoded = np.power(np.clip(encoded, 0.0, None), 1.0 / 1.1)
    contrast = encoded * encoded * (3.0 - 2.0 * encoded)
    return (0.25 * encoded + 0.75 * contrast) * 255.0


def proposed_linear(kind, slider, linear):
    amplitude_pos, amplitude_neg, gamma_pos, gamma_neg, t0, power, roll = FITS[kind]
    amount = slider / 100.0
    gamma = gamma_pos if amount >= 0.0 else gamma_neg
    amplitude = amplitude_pos if amount >= 0.0 else -amplitude_neg
    perceptual = np.power(np.maximum(linear, 0.0), 0.4545)
    window = np.power(np.maximum(perceptual - t0, 0.0), power) / np.power(1.0 + perceptual, roll)
    delta = amplitude * (abs(amount) ** gamma) * window
    updated = np.power(np.maximum(perceptual + delta, 0.0), 1.0 / 0.4545)
    # Above the measured ramp, hold the gain at the top measured linear value.
    if np.ndim(linear) == 0:
        if linear > HOLD_LINEAR:
            return proposed_linear(kind, slider, HOLD_LINEAR) * (linear / HOLD_LINEAR)
        return float(updated)
    updated = np.where(
        linear > HOLD_LINEAR,
        proposed_linear(kind, slider, HOLD_LINEAR) * (linear / HOLD_LINEAR),
        updated,
    )
    return updated


def current_linear(kind, slider, linear):
    """Flat-field current law. UI values are divided by the production scales."""
    rgb = np.array(linear, dtype=np.float64)
    if kind == "whites":
        amount = slider / 40.0
        luma = np.maximum(rgb, 1e-4)
        perceptual = np.power(luma, 0.4545)
        above = np.maximum(perceptual - 0.4, 0.0)
        if amount > 0.0:
            updated = perceptual + amount * 0.35 * (above * above) / (1.0 + above * above)
        else:
            updated = np.where(
                above > 0.0,
                0.4 + above / (1.0 + (-amount) * 0.3 * above),
                perceptual,
            )
        ramp = smoothstep(0.1, 0.9, perceptual)
        updated = perceptual * (1.0 - ramp) + updated * ramp
        return rgb * np.power(np.maximum(updated, 0.0), 2.2) / luma
    if kind == "highlights":
        amount = slider / 120.0
        pivot = 0.06
        delta = np.maximum(rgb - pivot, 0.0)
        if amount < 0.0:
            strength = (-amount) * 2.2
            compressed = delta / (1.0 + strength * (delta / (1.0 + delta * 0.35)))
            target = rgb + (compressed - delta)
        else:
            boost = amount * 0.70
            boosted = delta * (1.0 + boost / (1.0 + delta * 0.25))
            target = rgb + (boosted - delta)
        blend = smoothstep(pivot, pivot + 0.22, rgb)
        return np.where(rgb <= pivot, rgb, rgb * (1.0 - blend) + target * blend)
    shadows = slider / 120.0 if kind == "shadows" else 0.0
    blacks = slider / 40.0 if kind == "blacks" else 0.0
    safe = np.maximum(rgb, 1e-4)
    perceptual = np.power(safe, 0.4545)
    shadow_lift = shadows * perceptual * np.power(np.maximum(1.0 - perceptual, 0.0), 4.5)
    black_lift = blacks * perceptual * np.power(np.maximum(1.0 - perceptual, 0.0), 12.0)
    lift = np.maximum(shadow_lift + black_lift, 0.0)
    curved = np.maximum(perceptual + shadow_lift + black_lift, 0.0)
    stretch = 1.0 + lift * (1.0 - smoothstep(0.1, 0.85, perceptual)) * 4.4
    final = np.maximum(0.2 + (curved - 0.2) * stretch, 0.0)
    return rgb * (np.power(final, 2.2) / safe)


def strength(proposed_delta, lightroom_delta):
    mask = (
        (np.abs(proposed_delta) >= 0.25)
        & (np.abs(lightroom_delta) >= 0.25)
        & (np.sign(proposed_delta) == np.sign(lightroom_delta))
    )
    if int(mask.sum()) < 3:
        return None
    denominator = float(np.dot(proposed_delta[mask], lightroom_delta[mask]))
    if denominator == 0.0:
        return None
    return 100.0 * float(np.dot(proposed_delta[mask], proposed_delta[mask])) / denominator


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {Path(sys.argv[0]).name} comparison.json")
    comparison = json.loads(Path(sys.argv[1]).read_text())
    ramp = comparison["photos"]["rr-j40-grey-ramp"]["engines"]
    linear = np.array([step["input_linear"] for step in ramp["rapidroom"]["ramp_steps"]["blacks_0"]])
    baseline = encode_luma(linear)
    print(f"{'slider':22} {'old mean':>8} {'new mean':>8} {'old pix':>8} {'new pix':>8} {'new rms':>8}")
    for kind in ("highlights", "shadows", "whites", "blacks"):
        for slider in SLIDERS:
            lightroom = np.array(
                [step["delta_luma"] for step in ramp["lightroom"]["ramp_steps"][f"{kind}_{slider}"]]
            )
            current = encode_luma(current_linear(kind, slider, linear)) - baseline
            proposed = encode_luma(proposed_linear(kind, slider, linear)) - baseline
            old_mean = 100.0 * current.sum() / lightroom.sum()
            new_mean = 100.0 * proposed.sum() / lightroom.sum()
            print(
                f"{kind + ' ' + str(slider):22} {old_mean:8.1f} {new_mean:8.1f} "
                f"{strength(current, lightroom):8.1f} {strength(proposed, lightroom):8.1f} "
                f"{float(np.sqrt(np.mean((proposed - lightroom) ** 2))):8.2f}"
            )
    print("\nmigration: old slider -> new slider, ramp RMS of the encoded delta")
    grid = np.linspace(-100.0, 100.0, 801)
    for kind in ("highlights", "shadows", "whites", "blacks"):
        cached = {
            round(float(value), 3): encode_luma(proposed_linear(kind, float(value), linear)) - baseline
            for value in grid
        }
        for old in (-100, -50, 50, 100):
            target = encode_luma(current_linear(kind, old, linear)) - baseline
            best_value, best_error = 0.0, 1e9
            for value, delta in cached.items():
                if old > 0 and value < 0 or old < 0 and value > 0:
                    continue
                error = float(np.mean((delta - target) ** 2))
                if error < best_error:
                    best_value, best_error = value, error
            print(f"{kind:12} {old:4} -> {best_value:7.2f}  rms {best_error ** 0.5:5.2f}")


if __name__ == "__main__":
    main()
