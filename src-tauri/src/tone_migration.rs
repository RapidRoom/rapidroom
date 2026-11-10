//! Opt-in one-slider J40 ramp matching. Never called while loading a recipe.
use serde_json::{Value, json};

const KINDS: [&str; 4] = ["highlights", "shadows", "whites", "blacks"];
const FITS: [[f64; 7]; 4] = [
    [5.3936, 6.0134, 0.9184, 1.0243, -0.1437, 5.2500, 4.5634],
    [5.5057, 3.8280, 1.2969, 1.0059, 0.0437, 0.8285, 10.9305],
    [3.9336, 1.8512, 1.9202, 0.6126, 0.0024, 2.8656, 4.0217],
    [2.6903, 6.8315, 1.0891, 1.6427, 0.0448, 0.9347, 9.9743],
];
fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn encode(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let t = if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
    .powf(1.0 / 1.1);
    (0.25 * t + 0.75 * t * t * (3.0 - 2.0 * t)) * 255.0
}
fn fitted(kind: usize, value: f64, x: f64) -> f64 {
    if value == 0.0 {
        return x;
    }
    let [ap, an, gp, gn, t0, p, q] = FITS[kind];
    let t = x.max(0.0).powf(0.4545);
    let (a, g) = if value >= 0.0 { (ap, gp) } else { (-an, gn) };
    (t + a * (value / 100.0).abs().powf(g) * (t - t0).max(0.0).powf(p) / (1.0 + t).powf(q))
        .max(0.0)
        .powf(1.0 / 0.4545)
}
fn legacy(kind: usize, value: f64, x: f64) -> f64 {
    if value == 0.0 {
        return x;
    }
    if kind == 0 {
        let a = value / 120.0;
        let d = (x - 0.06).max(0.0);
        let target = if a < 0.0 {
            x + d / (1.0 + (-a) * 2.2 * d / (1.0 + d * 0.35)) - d
        } else {
            x + d * (a * 0.70 / (1.0 + d * 0.25))
        };
        return x + (target - x) * smooth(0.06, 0.28, x);
    }
    let safe = x.max(1e-4);
    let t = safe.powf(0.4545);
    if kind == 2 {
        let a = value / 40.0;
        let u = (t - 0.4).max(0.0);
        let new = if a > 0.0 {
            t + a * 0.35 * u * u / (1.0 + u * u)
        } else if u > 0.0 {
            0.4 + u / (1.0 + (-a) * 0.3 * u)
        } else {
            t
        };
        return x * (t + (new - t) * smooth(0.1, 0.9, t)).powf(2.2) / safe;
    }
    let lift = if kind == 1 {
        value / 120.0 * t * (1.0 - t).max(0.0).powf(4.5)
    } else {
        value / 40.0 * t * (1.0 - t).max(0.0).powf(12.0)
    };
    let curved = (t + lift).max(0.0);
    let stretch = 1.0 + lift.max(0.0) * (1.0 - smooth(0.1, 0.85, t)) * 4.4;
    x * (0.2 + (curved - 0.2) * stretch).max(0.0).powf(2.2) / safe
}
fn migrate_controls(
    controls: &mut Value,
    prefix: &str,
    residuals: &mut Value,
) -> Result<(), String> {
    for (kind, key) in KINDS.iter().enumerate() {
        let Some(old) = controls.get(*key) else {
            continue;
        };
        let old = old
            .as_f64()
            .filter(|n| n.is_finite() && (-100.0..=100.0).contains(n))
            .ok_or_else(|| format!("{prefix}{key} must be between -100 and 100"))?;
        let mut best = (0.0, f64::INFINITY);
        for step in -400..=400 {
            let candidate = f64::from(step) * 0.25;
            if old * candidate < 0.0 || (old == 0.0 && candidate != 0.0) {
                continue;
            }
            let error = (0..22)
                .map(|i| {
                    let x = 0.18 * 2.0_f64.powf(-6.0 + f64::from(i) / 3.0);
                    (encode(fitted(kind, candidate, x)) - encode(legacy(kind, old, x))).powi(2)
                })
                .sum::<f64>()
                / 22.0;
            if error < best.1 {
                best = (candidate, error);
            }
        }
        controls[*key] = json!(best.0);
        residuals[format!("{prefix}{key}")] =
            json!({"before": old, "after": best.0, "rampRms": best.1.sqrt()});
    }
    Ok(())
}

#[tauri::command]
pub fn migrate_tone_sliders(mut adjustments: Value) -> Result<Value, String> {
    if !adjustments.is_object() {
        return Err("adjustments must be an object".into());
    }
    let mut residuals = json!({});
    if crate::image_processing::tone_slider_version(&adjustments) == 0 {
        migrate_controls(&mut adjustments, "", &mut residuals)?;
        if let Some(masks) = adjustments["masks"].as_array_mut() {
            for (index, mask) in masks.iter_mut().enumerate() {
                if mask["adjustments"].is_object() {
                    migrate_controls(
                        &mut mask["adjustments"],
                        &format!("masks.{index}."),
                        &mut residuals,
                    )?;
                }
            }
        }
        adjustments["toneSliderVersion"] = json!(1);
    }
    Ok(json!({"adjustments": adjustments, "residuals": residuals}))
}

/// Writes a separate output, so the source sidecar or saved preset remains available.
pub fn run_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err("usage: RapidRAW migrate-tone INPUT.json OUTPUT.json".into());
    }
    let mut document: Value =
        serde_json::from_slice(&std::fs::read(&args[0]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let nested = document.get("adjustments").is_some();
    let result = migrate_tone_sliders(if nested {
        document["adjustments"].clone()
    } else {
        document.clone()
    })?;
    if nested {
        document["adjustments"] = result["adjustments"].clone();
    } else {
        document = result["adjustments"].clone();
    }
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    output
        .write_all(
            serde_json::to_string_pretty(&document)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    println!("{}", result["residuals"]);
    Ok(())
}
