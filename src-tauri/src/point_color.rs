//! Point Color GPU recipe and bounded, separable OKLCh shifts.
//!
//! Original implementation from the RapidRoom #160 / Redlamp design. A smooth
//! trapezoid has |w'| <= 1.5 / fade_width. Limiting each active swatch's
//! displacement to fade_width / (2 * active_count) therefore leaves every
//! same-axis mapping slope >= 0.25, including overlapping swatches.
use bytemuck::{Pod, Zeroable};
use serde_json::Value;

pub const MAX_POINTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, serde::Serialize, serde::Deserialize)]
pub struct GpuPointColor {
    pub color: [f32; 4],
    pub range: [f32; 4],
    pub shift: [f32; 4],
}

fn number(value: &Value, key: &str, default: f32, min: f32, max: f32) -> f32 {
    value[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .map_or(default, |n| n.clamp(f64::from(min), f64::from(max)) as f32)
}

pub fn parse(value: &Value) -> [GpuPointColor; MAX_POINTS] {
    let mut points = [GpuPointColor::default(); MAX_POINTS];
    if let Some(items) = value.as_array() {
        for (point, item) in points.iter_mut().zip(items.iter()) {
            let color = &item["color"];
            if !["lightness", "chroma", "hue"]
                .iter()
                .all(|key| color[key].as_f64().is_some_and(f64::is_finite))
            {
                continue;
            }
            point.color = [
                number(color, "lightness", 0.5, 0.0, 4.0),
                number(color, "chroma", 0.1, 0.0, 2.0),
                number(color, "hue", 0.0, 0.0, 360.0),
                0.0,
            ];
            point.range = [
                number(item, "hueRange", 40.0, 0.1, 180.0),
                number(item, "chromaRange", 0.1, 0.001, 0.5),
                number(item, "lightnessRange", 0.25, 0.001, 1.0),
                number(item, "smoothness", 50.0, 10.0, 100.0) / 100.0,
            ];
            point.shift = [
                number(item, "hueShift", 0.0, -100.0, 100.0) / 100.0,
                number(item, "saturationShift", 0.0, -100.0, 100.0) / 100.0,
                number(item, "luminanceShift", 0.0, -100.0, 100.0) / 100.0,
                0.0,
            ];
            point.shift[3] = if point.shift[..3].iter().any(|shift| *shift != 0.0) {
                1.0
            } else {
                0.0
            };
        }
    }
    points[0].color[3] = points.iter().map(|point| point.shift[3]).sum();
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn weight(distance: f32, width: f32, smoothness: f32) -> f32 {
        let fade = width * smoothness;
        let t = ((distance.abs() - (width - fade)) / fade).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }

    fn shifted(x: f32, centers: &[f32], width: f32, smoothness: f32, amount: f32) -> f32 {
        let limit = width * smoothness / (2.0 * centers.len() as f32);
        x + centers
            .iter()
            .map(|center| amount * limit * weight(x - center, width, smoothness))
            .sum::<f32>()
    }

    #[test]
    fn defaults_and_invalid_points_are_exact_noops() {
        assert!(parse(&Value::Null).iter().all(|p| p.shift == [0.0; 4]));
        let points = parse(&json!([{"color":{"lightness":0.5,"chroma":0.1,"hue":30}}]));
        assert_eq!(points[0].shift, [0.0; 4]);
        let invalid = parse(&json!([{"hueShift":100}]));
        assert_eq!(invalid[0].shift, [0.0; 4]);
    }

    #[test]
    fn recipes_are_bounded_and_never_exceed_eight_swatches() {
        let point =
            json!({"color":{"lightness":0.5,"chroma":0.1,"hue":30},"hueShift":999,"smoothness":0});
        let parsed = parse(&Value::Array(vec![point; 12]));
        assert_eq!(parsed.len(), 8);
        assert!(
            parsed
                .iter()
                .all(|p| p.shift[0] == 1.0 && p.shift[3] == 1.0 && p.range[3] == 0.1)
        );
    }

    #[test]
    fn signed_shifts_keep_order_even_with_eight_overlapping_ranges() {
        for width in [0.001, 0.1, 1.0, 40.0, 180.0] {
            for smoothness in [0.1, 0.5, 1.0] {
                for amount in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                    for centers in [
                        vec![0.0],
                        vec![0.0; 8],
                        vec![-0.4 * width, -0.2 * width, 0.2 * width, 0.4 * width],
                    ] {
                        let step = width / 1000.0;
                        let mut last = shifted(-2.0 * width, &centers, width, smoothness, amount);
                        for index in 1..=4000 {
                            let x = -2.0 * width + index as f32 * step;
                            let next = shifted(x, &centers, width, smoothness, amount);
                            assert!(
                                next - last >= 0.249 * step,
                                "slope bound: {width} {smoothness} {amount} {index}"
                            );
                            last = next;
                        }
                    }
                }
            }
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub lightness: f32,
    pub chroma: f32,
    pub hue: f32,
}

pub fn average_sample(
    image: &image::DynamicImage,
    x: f32,
    y: f32,
    radius: f32,
) -> Result<Sample, String> {
    let pixels = image.to_rgba16();
    let (width, height) = pixels.dimensions();
    let cx = x * width as f32;
    let cy = y * height as f32;
    let radius = (radius * width as f32).max(1.0);
    let mut lab = [0.0_f64; 3];
    let mut count = 0_u32;
    let x0 = (cx - radius).max(0.0) as u32;
    let y0 = (cy - radius).max(0.0) as u32;
    let x1 = ((cx + radius).ceil() as u32).min(width);
    let y1 = ((cy + radius).ceil() as u32).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            if (px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2) > radius * radius {
                continue;
            }
            let p = pixels.get_pixel(px, py);
            lab[0] += f64::from(p[0]) / f64::from(u16::MAX) * 4.0;
            lab[1] += f64::from(p[1]) / f64::from(u16::MAX) * 2.0 - 1.0;
            lab[2] += f64::from(p[2]) / f64::from(u16::MAX) * 2.0 - 1.0;
            count += 1;
        }
    }
    if count == 0 {
        return Err("Point Color sample is outside the image".into());
    }
    for value in &mut lab {
        *value /= f64::from(count);
    }
    Ok(Sample {
        lightness: lab[0] as f32,
        chroma: lab[1].hypot(lab[2]) as f32,
        hue: lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0) as f32,
    })
}
