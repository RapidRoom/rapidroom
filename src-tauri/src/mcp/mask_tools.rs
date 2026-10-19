use image::GenericImageView;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::{adjustments, masks, preview, tools, ui};
use crate::AppState;

pub(super) fn definitions() -> Vec<Value> {
    let point = json!({"type":"object","additionalProperties":false,"properties":{
        "x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]});
    let mut generated = json!({"imagePath":{"type":"string"},"expectedRevision":{"type":"string"},
        "maskId":masks::identifier_schema(),"subMaskId":masks::identifier_schema(),
        "kind":{"type":"string","enum":["subject","sky","foreground"]},
        "name":{"type":"string","minLength":1,"maxLength":128},
        "coordinateSpace":{"type":"string","enum":["view","mask"],"default":"view"},
        "point":point,"box":{"type":"object","additionalProperties":false,"properties":{
            "x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0}},"required":["x","y","width","height"]},
        "verbose":{"type":"boolean","default":false}});
    generated["label"] = json!({"type":"string","minLength":1,"maxLength":160});
    let (_, defs) = adjustments::compact_schema();
    let mut patch = defs["mask"].clone();
    patch["required"] = json!(["id"]);
    vec![
        json!({"name":"generate_ai_mask","description":"Run the same local SAM subject, U2Net foreground or sky model as the GUI and store its PNG as one sub-mask. Uses existing verified GUI model downloads if missing; no cloud inference. Subject accepts a point or box (otherwise the full visible frame); sky/foreground segment the whole image. View coordinates are fractions of the cropped, corrected view; mask coordinates are pre-crop pixels on the same corrected/oriented canvas. Preserves sibling masks/components, refuses a changed edit, and commits one undoable step. May take several minutes on first use.","inputSchema":{"type":"object","additionalProperties":false,"properties":generated,"required":["imagePath","maskId","subMaskId","kind"]}}),
        json!({"name":"render_mask","description":"Read a grayscale matte (white=affected, black=unaffected) for one existing mask using the exact edit geometry/crop, opacity, inversion and component composition. Invisible masks render black. Does not change history, edit, or selection.","annotations":{"readOnlyHint":true,"destructiveHint":false},"inputSchema":{"type":"object","additionalProperties":false,"properties":{"imagePath":{"type":"string"},"maskId":masks::identifier_schema(),"maxDimension":{"type":"integer","minimum":128,"maximum":2048,"default":1024}},"required":["imagePath","maskId"]}}),
        json!({"name":"convert_coordinates","description":"Convert points between view fractions (after crop) and mask pixels (before crop on the corrected/oriented canvas). Lens/perspective warps already precede BOTH spaces: no second lens warp is applied. This is not a conversion to uncorrected sensor coordinates. Read-only; use the returned editRevision when placing a mask.","annotations":{"readOnlyHint":true,"destructiveHint":false},"inputSchema":{"type":"object","additionalProperties":false,"properties":{"imagePath":{"type":"string"},"from":{"type":"string","enum":["view","mask"]},"points":{"type":"array","minItems":1,"maxItems":256,"items":point}},"required":["imagePath","from","points"]}}),
        json!({"name":"upsert_masks","description":"Atomically create/deep-update 1–64 distinct mask IDs, preserving siblings. Same rules as upsert_mask; all entries validate before one history/autosave step. Duplicate IDs are refused.","inputSchema":{"type":"object","additionalProperties":false,"$defs":adjustments::referenced_definitions(&json!({"mask":patch}),&defs),"properties":{"imagePath":{"type":"string"},"expectedRevision":{"type":"string"},"masks":{"type":"array","minItems":1,"maxItems":64,"items":patch},"label":{"type":"string","minLength":1,"maxLength":160},"verbose":{"type":"boolean","default":false}},"required":["imagePath","masks"]}}),
    ]
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x: f64,
    y: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoxRegion {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

// Geometry warp, fine rotation and flips retain canvas dimensions. Only coarse
// orientation swaps the axes. Match apply_crop's rounding and clipping rules.
fn view_rect(width: u32, height: u32, edit: &Value) -> [f64; 4] {
    let (w, h) = if edit["orientationSteps"].as_u64().unwrap_or(0) % 2 == 1 {
        (height, width)
    } else {
        (width, height)
    };
    let c = &edit["crop"];
    let vals = ["x", "y", "width", "height"].map(|k| c[k].as_f64().unwrap_or(0.0).round().max(0.0));
    let [x, y, cw, ch] = vals;
    if x < f64::from(w) && y < f64::from(h) && cw > 0.0 && ch > 0.0 {
        // The renderer offsets mask coordinates by the original crop's f32
        // offset, even where apply_crop rounds the sampling rectangle.
        [
            c["x"].as_f64().unwrap_or(0.0) as f32 as f64,
            c["y"].as_f64().unwrap_or(0.0) as f32 as f64,
            cw.min(f64::from(w) - x),
            ch.min(f64::from(h) - y),
        ]
    } else {
        [0.0, 0.0, f64::from(w), f64::from(h)]
    }
}
fn convert(point: Point, rect: [f64; 4], from: &str) -> Result<Point, String> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err("Coordinates must be finite".into());
    }
    let [x, y, w, h] = rect;
    match from {
        "view" if (0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y) => Ok(Point {
            x: x + point.x * w,
            y: y + point.y * h,
        }),
        "view" => Err("View coordinates must be fractions from 0 to 1".into()),
        "mask" => Ok(Point {
            x: (point.x - x) / w,
            y: (point.y - y) / h,
        }),
        _ => Err("coordinate space must be view or mask".into()),
    }
}
fn label(arguments: &Value, default: String) -> Result<String, String> {
    match arguments.get("label") {
        None => Ok(default),
        Some(value) => value
            .as_str()
            .filter(|s| {
                !s.trim().is_empty() && s.chars().count() <= 160 && !s.chars().any(char::is_control)
            })
            .map(|s| format!("AI: {s}"))
            .ok_or("label must be 1–160 printable characters".into()),
    }
}
fn id(arguments: &Value, key: &str) -> Result<String, String> {
    let id = arguments[key]
        .as_str()
        .ok_or(format!("{key} is required"))?;
    masks::validate_id(id)?;
    Ok(id.into())
}
fn current_rect(app: &AppHandle, path: &str, edit: &Value) -> Result<[f64; 4], String> {
    let state = app.state::<AppState>();
    let image = state.original_image.lock().unwrap();
    let image = image.as_ref().ok_or("No loaded image")?;
    let (source, _) = crate::file_management::parse_virtual_path(path);
    if source != std::path::Path::new(&image.path) {
        return Err("Loaded image changed".into());
    }
    let (w, h) = image.image.dimensions();
    if w == 0 || h == 0 {
        return Err("Empty loaded image".into());
    }
    Ok(view_rect(w, h, edit))
}

pub(super) async fn call(app: &AppHandle, name: &str, args: &Value) -> Result<Value, String> {
    let path = tools::required_image_path(args)?;
    ui::require_active_session(app, Some(&path))?;
    tools::check_expected_revision(app, &path, args.get("expectedRevision")).await?;
    let before = tools::get_image_state(app, args)?;
    match name {
        "convert_coordinates" => {
            let points: Vec<Point> =
                serde_json::from_value(args["points"].clone()).map_err(|e| e.to_string())?;
            if points.is_empty() || points.len() > 256 {
                return Err("Provide 1–256 points".into());
            }
            let from = args["from"].as_str().ok_or("from is required")?;
            let rect = current_rect(app, &path, &before["adjustments"])?;
            let points = points
                .into_iter()
                .map(|p| convert(p, rect, from).map(|p| json!({"x":p.x,"y":p.y})))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(
                json!({"imagePath":path,"editRevision":before["editRevision"],"coordinateSpace":if from=="view" {"mask"} else {"view"},"points":points,"viewRect":rect}),
            )
        }
        "render_mask" => {
            let mask_id = id(args, "maskId")?;
            let bound = args
                .get("maxDimension")
                .map_or(Some(1024), Value::as_u64)
                .filter(|n| (128..=2048).contains(n))
                .ok_or("maxDimension must be 128–2048")? as u32;
            let image = crate::generate_mask_pixels_for_path(
                path.clone(),
                before["adjustments"].clone(),
                mask_id.clone(),
                bound,
                app.clone(),
            )
            .await?;
            tools::ensure_read_state(app, args, &before).await?;
            let (w, h) = image.dimensions();
            Ok(
                json!({"imagePath":path,"editRevision":before["editRevision"],"maskId":mask_id,"width":w,"height":h,"content":super::measure::content(&[image])?}),
            )
        }
        "upsert_masks" => {
            let changed = batch(&before["adjustments"]["masks"], &args["masks"])?;
            let names: Vec<_> = args["masks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["name"].as_str().or(m["id"].as_str()).unwrap_or("Mask"))
                .collect();
            let summary = names.iter().take(3).copied().collect::<Vec<_>>().join(", ");
            let extra = if names.len() > 3 {
                format!(" (+{} more)", names.len() - 3)
            } else {
                String::new()
            };
            let title = label(args, format!("AI: Masks: {summary}{extra}"))?;
            commit(app, args, &before, changed, title).await
        }
        "generate_ai_mask" => generate(app, args, &before, &path).await,
        _ => Err("Unknown mask tool".into()),
    }
}
fn batch(current: &Value, patches: &Value) -> Result<Value, String> {
    let entries = patches
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .ok_or("masks must contain 1–64 entries")?;
    let mut ids = std::collections::HashSet::new();
    let mut next = current.clone();
    for patch in entries {
        let id = patch["id"].as_str().ok_or("mask id is required")?;
        if !ids.insert(id) {
            return Err(format!("Duplicate mask id: {id}"));
        }
        next = masks::change_masks(&next, Some(patch), None)?;
    }
    Ok(next)
}
async fn commit(
    app: &AppHandle,
    args: &Value,
    before: &Value,
    changed: Value,
    title: String,
) -> Result<Value, String> {
    let verbose = preview::option(args, "verbose")?;
    let mut next = before["adjustments"].clone();
    next["masks"] = changed;
    let response=ui::request_ui(app,"apply-adjustments",json!({"path":before["imagePath"],"adjustments":next,"baseAdjustments":before["adjustments"],"label":title})).await?;
    Ok(tools::mutation_response(before, response, verbose))
}
async fn generate(
    app: &AppHandle,
    args: &Value,
    before: &Value,
    path: &str,
) -> Result<Value, String> {
    let mask_id = id(args, "maskId")?;
    let sub_id = id(args, "subMaskId")?;
    let kind = args["kind"]
        .as_str()
        .filter(|k| ["subject", "sky", "foreground"].contains(k))
        .ok_or("kind must be subject, sky or foreground")?;
    let title = label(args, format!("AI: Generate {kind} mask"))?;
    preview::option(args, "verbose")?;
    let rect = current_rect(app, path, &before["adjustments"])?;
    let space = args
        .get("coordinateSpace")
        .map_or(Some("view"), Value::as_str)
        .filter(|s| ["view", "mask"].contains(s))
        .ok_or("coordinateSpace must be view or mask")?;
    if args.get("point").is_some() && args.get("box").is_some() {
        return Err("Use point or box, not both".into());
    }
    if kind != "subject" && (args.get("point").is_some() || args.get("box").is_some()) {
        return Err(
            "Only subject accepts point or box; sky/foreground segment the full image".into(),
        );
    }
    let (start, end) = if let Some(p) = args.get("point") {
        let p: Point = serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
        let p = if space == "view" {
            convert(p, rect, space)?
        } else {
            convert(convert(p, rect, "mask")?, rect, "view")?
        };
        ((p.x, p.y), (p.x, p.y))
    } else if let Some(b) = args.get("box") {
        let b: BoxRegion = serde_json::from_value(b.clone()).map_err(|e| e.to_string())?;
        if !b.width.is_finite() || !b.height.is_finite() || b.width <= 0.0 || b.height <= 0.0 {
            return Err("Box must have positive finite dimensions".into());
        }
        let p = Point { x: b.x, y: b.y };
        let q = Point {
            x: b.x + b.width,
            y: b.y + b.height,
        };
        let (p, q) = if space == "view" {
            (convert(p, rect, space)?, convert(q, rect, space)?)
        } else {
            (
                convert(convert(p, rect, "mask")?, rect, "view")?,
                convert(convert(q, rect, "mask")?, rect, "view")?,
            )
        };
        ((p.x, p.y), (q.x, q.y))
    } else {
        ((rect[0], rect[1]), (rect[0] + rect[2], rect[1] + rect[3]))
    };
    let name = args.get("name").map_or(Ok(format!("AI {kind}")), |n| {
        n.as_str()
            .filter(|s| {
                !s.trim().is_empty() && s.chars().count() <= 128 && !s.chars().any(char::is_control)
            })
            .map(str::to_owned)
            .ok_or_else(|| "name must be 1–128 printable characters".to_string())
    })?;
    if before["adjustments"]["masks"]
        .as_array()
        .is_some_and(|masks| {
            masks.iter().any(|m| {
                m["id"] != mask_id
                    && m["subMasks"]
                        .as_array()
                        .is_some_and(|subs| subs.iter().any(|s| s["id"] == sub_id))
            })
        })
    {
        return Err("subMaskId is already used by another mask".into());
    }
    // Validate IDs, container limit and defaults before expensive inference.
    let skeleton = json!({"id":mask_id,"subMasks":[{"id":sub_id,"type":format!("ai-{kind}"),"parameters":if kind=="subject" {json!({"startX":start.0,"startY":start.1,"endX":end.0,"endY":end.1})} else {json!({})}}]});
    masks::change_masks(&before["adjustments"]["masks"], Some(&skeleton), None)?;
    let state = app.state::<AppState>();
    let source_image = state
        .original_image
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("No loaded image")?
        .image
        .clone();
    let started = std::time::Instant::now();
    // Initialize before rechecking the session, since first use can download.
    tokio::time::timeout(
        std::time::Duration::from_secs(480),
        crate::ai_processing::get_or_init_ai_models(app, &state.ai_state, &state.ai_init_lock),
    )
    .await
    .map_err(|_| "Model initialization timed out; no edit was applied")?
    .map_err(|e| e.to_string())?;
    tools::ensure_read_state(app, args, before).await?;
    current_rect(app, path, &before["adjustments"])?;
    if !state
        .original_image
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|image| std::sync::Arc::ptr_eq(&image.image, &source_image))
    {
        return Err(
            "Loaded source changed during model initialization; no edit was applied".into(),
        );
    }
    let mut edit = before["adjustments"].clone();
    let (source, _) = crate::file_management::parse_virtual_path(path);
    // SAM's embedding key hashes the concrete geometry fields. Resolve the
    // source-bound camera policy first, so Auto/Manual cannot reuse embeddings
    // made from a different warp with the same unresolved recipe fields.
    crate::camera_lens::resolve_snapshot(&mut edit, &source.to_string_lossy(), &state);
    let rot = edit["rotation"].as_f64().unwrap_or(0.0) as f32;
    let fh = edit["flipHorizontal"].as_bool().unwrap_or(false);
    let fv = edit["flipVertical"].as_bool().unwrap_or(false);
    let steps = edit["orientationSteps"].as_u64().unwrap_or(0) as u8;
    let task = Some(format!("mcp-{}", uuid::Uuid::new_v4()));
    let parameters = match kind {
        "subject" => serde_json::to_value(
            crate::ai_commands::generate_ai_subject_mask(
                edit,
                path.into(),
                start,
                end,
                rot,
                fh,
                fv,
                steps,
                Some(false),
                task,
                state,
                app.clone(),
            )
            .await?,
        )
        .map_err(|e| e.to_string())?,
        "sky" => serde_json::to_value(
            crate::ai_commands::generate_ai_sky_mask(
                edit,
                rot,
                fh,
                fv,
                steps,
                task,
                state,
                app.clone(),
            )
            .await?,
        )
        .map_err(|e| e.to_string())?,
        _ => serde_json::to_value(
            crate::ai_commands::generate_ai_foreground_mask(
                edit,
                rot,
                fh,
                fv,
                steps,
                task,
                state,
                app.clone(),
            )
            .await?,
        )
        .map_err(|e| e.to_string())?,
    };
    if started.elapsed() > std::time::Duration::from_secs(540) {
        return Err("Model timed out; no edit was applied".into());
    }
    tools::ensure_read_state(app, args, before).await?;
    ui::require_active_session(app, Some(path))?;
    if !app
        .state::<AppState>()
        .original_image
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|image| std::sync::Arc::ptr_eq(&image.image, &source_image))
    {
        return Err("Loaded source changed during inference; no edit was applied".into());
    }
    let current = before["adjustments"]["masks"]
        .as_array()
        .ok_or("masks must be an array")?;
    let mut patch = current
        .iter()
        .find(|m| m["id"] == mask_id)
        .cloned()
        .unwrap_or_else(|| json!({"id":mask_id,"name":name,"subMasks":[]}));
    if args.get("name").is_some() {
        patch["name"] = json!(name);
    }
    let subs = patch["subMasks"]
        .as_array_mut()
        .ok_or("subMasks must be an array")?;
    let generated = json!({"id":sub_id,"type":format!("ai-{kind}"),"visible":true,"invert":false,"opacity":100,"mode":"additive","parameters":parameters});
    if let Some(old) = subs.iter_mut().find(|s| s["id"] == sub_id) {
        let old_params = old["parameters"].clone();
        let mut replacement = generated;
        for field in ["visible", "invert", "opacity", "mode"] {
            if let Some(value) = old.get(field) {
                replacement[field] = value.clone();
            }
        }
        for field in ["grow", "feather"] {
            if let Some(value) = old_params.get(field) {
                replacement["parameters"][field] = value.clone();
            }
        }
        *old = replacement;
    } else {
        subs.push(generated);
    }
    let next = masks::change_masks(&before["adjustments"]["masks"], Some(&patch), None)?;
    let mut response = commit(app, args, before, next, title).await?;
    response["maskId"] = json!(mask_id);
    response["subMaskId"] = json!(sub_id);
    response["kind"] = json!(kind);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converted_view_point_lands_in_the_rendered_matte_after_geometry() {
        let edit = json!({"orientationSteps":1,"rotation":10,"flipHorizontal":true,
            "transformDistortion":20,"transformScale":100,
            "crop":{"x":10,"y":20,"width":40,"height":60,"unit":"px"}});
        let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            128,
            96,
            image::Rgb([90, 120, 150]),
        ));
        let (rendered, offset) = crate::adjustment_utils::apply_all_transformations(&source, &edit);
        assert_eq!(rendered.dimensions(), (40, 60));
        let rect = view_rect(128, 96, &edit);
        let point = convert(Point { x: 0.25, y: 0.5 }, rect, "view").unwrap();
        let mask:crate::mask_generation::MaskDefinition=serde_json::from_value(json!({"id":"test","name":"test","visible":true,"invert":false,"opacity":100,"adjustments":{},"subMasks":[{
            "id":"radial","type":"radial","visible":true,"parameters":{"centerX":point.x,"centerY":point.y,"radiusX":5,"radiusY":5,"rotation":0,"feather":0}}]})).unwrap();
        let matte = crate::mask_generation::generate_mask_bitmap(
            &mask,
            rendered.width(),
            rendered.height(),
            1.0,
            offset,
            None,
        )
        .unwrap();
        assert_eq!(matte.get_pixel(10, 30).0, [255]);
        assert_eq!(matte.get_pixel(30, 10).0, [0]);
    }

    #[test]
    fn batch_is_atomic_and_preserves_siblings() {
        let old = json!([{"id":"old","subMasks":[],"adjustments":{"exposure":1}}]);
        let before = old.clone();
        assert!(batch(&old,&json!([{"id":"old","adjustments":{"exposure":2}},{"id":"bad","subMasks":[{"id":"x","type":"invalid","parameters":{}}]}])).is_err());
        assert_eq!(old, before);
        assert!(batch(&old, &json!([{"id":"old"},{"id":"old"}])).is_err());
        let next = batch(
            &old,
            &json!([{"id":"new","subMasks":[]},{"id":"old","adjustments":{"contrast":4}}]),
        )
        .unwrap();
        assert_eq!(next[0]["adjustments"], json!({"exposure":1,"contrast":4}));
        assert_eq!(next[1]["id"], "new");
    }
    #[test]
    fn view_points_match_mask_crop_pixels_with_rotation_and_lens_warp() {
        let edit = json!({"orientationSteps":1,"rotation":12,"flipHorizontal":true,"lensDistortionEnabled":true,"crop":{"x":100,"y":200,"width":800,"height":1200}});
        let rect = view_rect(6000, 4000, &edit);
        assert_eq!(rect, [100., 200., 800., 1200.]);
        let mask = convert(Point { x: 0.25, y: 0.5 }, rect, "view").unwrap();
        assert_eq!((mask.x, mask.y), (300., 800.));
        let view = convert(mask, rect, "mask").unwrap();
        assert_eq!((view.x, view.y), (0.25, 0.5));
        assert!(convert(Point { x: 1.1, y: 0. }, rect, "view").is_err());
        assert_eq!(
            view_rect(6000, 4000, &json!({"orientationSteps":1})),
            [0., 0., 4000., 6000.]
        );
    }
}
