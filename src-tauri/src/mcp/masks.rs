use serde_json::{Map, Value, json};

use super::adjustments;

pub(super) const LOCAL_ADJUSTMENTS: &[&str] = &[
    "blacks",
    "brightness",
    "clarity",
    "colorGrading",
    "colorNoiseReduction",
    "contrast",
    "curves",
    "pointCurves",
    "parametricCurve",
    "curveMode",
    "dehaze",
    "exposure",
    "flareAmount",
    "glowAmount",
    "halationAmount",
    "highlights",
    "hsl",
    "hue",
    "lumaNoiseReduction",
    "saturation",
    "sectionVisibility",
    "activeTools",
    "shadows",
    "sharpness",
    "sharpnessThreshold",
    "structure",
    "temperature",
    "tint",
    "vibrance",
    "whites",
];
const TYPES: &[&str] = &[
    "all",
    "radial",
    "linear",
    "color",
    "luminance",
    "brush",
    "flow",
    "ai-subject",
    "ai-foreground",
    "ai-sky",
    "ai-depth",
    "quick-eraser",
    "clone",
    "heal",
    "liquify",
    "retouch",
];

fn number(min: f64, max: f64, description: &str) -> Value {
    json!({"type":"number","minimum":min,"maximum":max,"description":description})
}
fn coordinate() -> Value {
    number(
        -1e9,
        1e9,
        "Original-image pixels before crop; may lie outside the image.",
    )
}
pub(super) fn identifier_schema() -> Value {
    json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_:.-]+$",
        "description":"Opaque ID: 1–128 ASCII letters, digits, underscore, colon, dot or hyphen."})
}
pub(super) fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty() || !crate::assistant_text::valid_opaque_id(value) {
        return Err(
            "Mask id must contain 1–128 ASCII letters, digits, underscore, colon, dot or hyphen"
                .into(),
        );
    }
    Ok(())
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}

pub(super) fn local_schema(global_properties: &Map<String, Value>) -> Value {
    let mut properties: Map<_, _> = LOCAL_ADJUSTMENTS
        .iter()
        .map(|key| (key.to_string(), global_properties[*key].clone()))
        .collect();
    properties.insert(
        "sectionVisibility".into(),
        json!({"type":"object","additionalProperties":{"type":"boolean"}}),
    );
    let mut schema = object(Value::Object(properties), &[]);
    schema["description"] = json!(
        "Sparse local adjustments; omitted nested defaults are filled on load/create. Updates preserve omitted existing values. Parametric split1 < split2 < split3 after defaults 25/50/75 are filled."
    );
    schema
}

pub(super) fn crop_schema() -> Value {
    let mut schema = object(
        json!({
            "x":number(0.0,1e9,"Crop left in original-image pixels."),
            "y":number(0.0,1e9,"Crop top in original-image pixels."),
            "width":{"type":"number","exclusiveMinimum":0,"maximum":1e9},
            "height":{"type":"number","exclusiveMinimum":0,"maximum":1e9},
            "unit":{"type":"string","enum":["px"],"default":"px"}
        }),
        &["x", "y", "width", "height"],
    );
    schema["type"] = json!(["object", "null"]);
    schema["description"] = json!(
        "Original-image pixel rectangle, or null to clear. unit is px; set aspectRatio separately to width divided by height (0.75 for 3:4)."
    );
    schema
}

fn stroke_schema(flow: bool) -> Value {
    let mut schema = object(
        json!({
            "tool":{"type":"string","enum":["brush","eraser"]},
            "brushSize":number(0.0,1e9,"Brush diameter in original-image pixels."),
            "feather":number(0.0,1.0,"0–1 feather, not percent; default 0.5."),
            "points":{"type":"array","items":object(json!({"x":coordinate(),"y":coordinate()}), &["x","y"])}
        }),
        &["tool", "brushSize", "points"],
    );
    if flow {
        schema["properties"]["flow"] = number(0.0, 100.0, "Flow percent; default 10.");
    }
    schema
}

fn parameters_schema(kind: &str) -> Value {
    let mut p = Map::new();
    let mut required = vec![];
    let coordinates: &[&str] = match kind {
        "radial" => &["centerX", "centerY", "radiusX", "radiusY"],
        "linear" => &["startX", "startY", "endX", "endY"],
        "color" | "luminance" => &["targetX", "targetY"],
        "ai-subject" | "quick-eraser" => &["startX", "startY", "endX", "endY"],
        _ => &[],
    };
    for key in coordinates {
        let value = if key.starts_with("radius") {
            number(0.0, 1e9, "Radius in original-image pixels.")
        } else {
            coordinate()
        };
        p.insert(key.to_string(), value);
        required.push(*key);
    }
    match kind {
        "radial" => {
            p.insert("rotation".into(), number(-180.0, 180.0, "Degrees."));
            p.insert(
                "feather".into(),
                number(0.0, 1.0, "0–1, not the 0–100 panel percentage."),
            );
            required.extend(["rotation", "feather"]);
        }
        "linear" => {
            p.insert(
                "range".into(),
                number(
                    0.0,
                    1e9,
                    "Feather half-width on each side of the line, in original pixels; default 50. At the line: 50%. Left-to-right affects above; top-to-bottom affects right. Reverse endpoints to swap sides.",
                ),
            );
        }
        "color" | "luminance" => {
            p.insert(
                "tolerance".into(),
                number(1.0, 100.0, "Panel tolerance; default 20."),
            );
        }
        "brush" | "flow" | "clone" | "heal" | "liquify" | "retouch" => {
            p.insert(
                "lines".into(),
                json!({"type":"array","items":stroke_schema(kind=="flow")}),
            );
            if kind == "flow" {
                p.insert("flow".into(), number(0.0, 100.0, "Panel flow percent."));
            }
        }
        _ => {}
    }
    if matches!(
        kind,
        "color"
            | "luminance"
            | "ai-subject"
            | "ai-foreground"
            | "ai-sky"
            | "ai-depth"
            | "quick-eraser"
    ) {
        p.insert("grow".into(), number(-100.0, 100.0, "Panel grow amount."));
        p.insert(
            "feather".into(),
            number(
                0.0,
                100.0,
                "Panel feather percent (radial and brush use 0–1 instead).",
            ),
        );
        p.insert(
            "rotation".into(),
            number(-180.0, 180.0, "Capture geometry rotation in degrees."),
        );
        for key in ["flipHorizontal", "flipVertical"] {
            p.insert(key.into(), json!({"type":"boolean"}));
        }
        p.insert(
            "orientationSteps".into(),
            json!({"type":"integer","minimum":0,"maximum":3}),
        );
    }
    if kind.starts_with("ai-") || kind == "quick-eraser" {
        // AI capture transforms are optional in the renderer, including explicit null.
        for key in [
            "rotation",
            "flipHorizontal",
            "flipVertical",
            "orientationSteps",
        ] {
            let field = p.get_mut(key).expect("capture transform field");
            field["type"] = json!([field["type"], "null"]);
        }
        p.insert("maskDataBase64".into(),json!({"type":["string","null"],"description":"Existing generated mask PNG bytes in Base64. This edit does not run a model or make a cloud request."}));
    }
    if kind == "ai-depth" {
        for key in ["minDepth", "maxDepth", "minFade", "maxFade"] {
            p.insert(key.into(), number(0.0, 100.0, "Depth/fade percent."));
        }
    }
    // The GUI retains this flag while drawing an unfinished local mask.
    p.insert("isInitialDraw".into(), json!({"type":"boolean"}));
    let mut schema = object(Value::Object(p), &required);
    if kind == "linear" {
        schema["description"] = json!(
            "Directed boundary through start→end, not a ramp between endpoints. Signed perpendicular distance d gives clamp(0.5 - d/(2*max(range,0.01)),0,1); zero-length line is empty."
        );
    }
    schema
}

// Tool schemas are independent JSON Schema documents. Share repeated component
// fields within each document; external refs cannot be assumed to resolve in a client.
pub(super) fn compact_components(full: &Value, definitions: &mut Value) -> Value {
    let variants = full["oneOf"].as_array().expect("static sub-mask variants");
    let mut common = variants[0].clone();
    common["properties"]["type"] = json!({"type":"string"});
    common["properties"]["parameters"] = json!({"type":"object"});
    definitions["subMaskCommon"] = common;
    let mut shared: std::collections::BTreeMap<String, (String, Value, usize)> =
        std::collections::BTreeMap::new();
    for variant in variants {
        for value in variant["properties"]["parameters"]["properties"]
            .as_object()
            .expect("static parameters")
            .values()
        {
            let key = value.to_string();
            if key.len() >= 80 {
                let n = shared.len();
                shared
                    .entry(key)
                    .or_insert_with(|| (format!("maskParameter{n}"), value.clone(), 0))
                    .2 += 1;
            }
        }
    }
    let compact = variants
        .iter()
        .map(|variant| {
            let mut parameters = variant["properties"]["parameters"].clone();
            for value in parameters["properties"]
                .as_object_mut()
                .expect("static parameters")
                .values_mut()
            {
                if let Some((name, schema, _)) = shared
                    .get(&value.to_string())
                    .filter(|(_, _, count)| *count > 1)
                {
                    definitions[name.as_str()] = schema.clone();
                    *value = json!({"$ref":format!("#/$defs/{name}")});
                }
            }
            json!({"properties":{"type":variant["properties"]["type"],"parameters":parameters}})
        })
        .collect::<Vec<_>>();
    json!({"allOf":[{"$ref":"#/$defs/subMaskCommon"},{"oneOf":compact}]})
}

fn sub_mask_schema(kind: &str) -> Value {
    object(
        json!({
            "id":identifier_schema(),"name":{"type":"string"},
            "type":{"type":"string","enum":[kind]},
            "visible":{"type":"boolean","default":true},
            "invert":{"type":"boolean","default":false},
            "opacity":number(0.0,100.0,"Opacity percent; default 100."),
            "mode":{"type":"string","enum":["additive","subtractive","intersect"],"default":"additive"},
            "parameters":parameters_schema(kind)
        }),
        &["id", "type", "parameters"],
    )
}

pub(super) fn mask_schema(local_adjustments: Value, partial: bool) -> Value {
    let variants: Vec<_> = TYPES.iter().map(|kind| sub_mask_schema(kind)).collect();
    let mut schema = object(
        json!({
            "id":identifier_schema(),"name":{"type":"string"},
            "visible":{"type":"boolean","default":true},
            "invert":{"type":"boolean","default":false},
            "opacity":number(0.0,100.0,"Container opacity percent; default 100."),
            "adjustments":local_adjustments,
            "subMasks":{"type":"array","items":{"oneOf":variants},"description":"Replaces this container's components, not sibling containers. Components compose in array order."}
        }),
        if partial {
            &["id"]
        } else {
            &["id", "subMasks"]
        },
    );
    schema["description"] = json!(
        "Mask container in original-image coordinates. Full masks arrays replace the entire array; upsert_mask edits one id and preserves siblings."
    );
    schema
}

// Validates this module's own acyclic schemas. No untrusted schemas/references are evaluated.
fn validate(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let types: Vec<&str> = match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !types.is_empty()
        && !types.iter().any(|t| match *t {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        })
    {
        return Err(format!("{path} must have type {}", types.join(" or ")));
    }
    if let Some(choices) = schema["enum"].as_array()
        && !choices.contains(value)
    {
        return Err(format!("{path} must be one of {choices:?}"));
    }
    if let Some(number) = value.as_f64() {
        for (key, valid) in [
            (
                "minimum",
                schema["minimum"].as_f64().is_none_or(|min| number >= min),
            ),
            (
                "maximum",
                schema["maximum"].as_f64().is_none_or(|max| number <= max),
            ),
            (
                "exclusiveMinimum",
                schema["exclusiveMinimum"]
                    .as_f64()
                    .is_none_or(|min| number > min),
            ),
        ] {
            if !valid {
                return Err(format!("{path} violates {key}: {}", schema[key]));
            }
        }
    }
    if let Some(text) = value.as_str() {
        let len = text.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| len < min)
            || schema["maxLength"].as_u64().is_some_and(|max| len > max)
        {
            return Err(format!("{path} has invalid string length"));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key} is required"));
                }
            }
        }
        for (key, child) in object {
            if let Some(rule) = schema["properties"].get(key) {
                validate(child, rule, &format!("{path}.{key}"))?;
            } else if schema["additionalProperties"].is_object() {
                validate(
                    child,
                    &schema["additionalProperties"],
                    &format!("{path}.{key}"),
                )?;
            } else if schema["additionalProperties"] == false {
                return Err(format!("unsupported field {path}.{key}"));
            }
        }
    }
    if let Some(array) = value.as_array() {
        let len = array.len() as u64;
        if schema["minItems"].as_u64().is_some_and(|min| len < min)
            || schema["maxItems"].as_u64().is_some_and(|max| len > max)
        {
            return Err(format!("{path} has invalid array length"));
        }
    }
    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (index, child) in array.iter().enumerate() {
            let path = format!("{path}[{index}]");
            if let Some(variants) = items["oneOf"].as_array() {
                let kind = child["type"]
                    .as_str()
                    .ok_or(format!("{path}.type is required"))?;
                let variant = variants
                    .iter()
                    .find(|v| v["properties"]["type"]["enum"][0] == kind)
                    .ok_or(format!("unsupported {path}.type: {kind}"))?;
                validate(child, variant, &path)?;
            } else {
                validate(child, items, &path)?;
            }
        }
    }
    Ok(())
}

pub(super) fn validate_crop(value: &Value) -> Result<(), String> {
    validate(value, &crop_schema(), "crop")
}
fn local_adjustments_schema() -> Value {
    let global = adjustments::adjustments_schema();
    local_schema(
        global["properties"]
            .as_object()
            .expect("static adjustment properties"),
    )
}
pub(super) fn validate_mask(value: &Value, partial: bool) -> Result<(), String> {
    validate(
        value,
        &mask_schema(local_adjustments_schema(), partial),
        "mask",
    )?;
    validate_id(value["id"].as_str().expect("validated id"))?;
    if let Some(local) = value.get("adjustments") {
        // Reuse individual field bounds and curve ordering from global edits.
        adjustments::validate_adjustments(local)?;
        if !partial {
            for channel in ["luma", "red", "green", "blue"] {
                let settings = &local["parametricCurve"][channel];
                let splits = [("split1", 25.0), ("split2", 50.0), ("split3", 75.0)]
                    .map(|(key, default)| settings[key].as_f64().unwrap_or(default));
                if !(splits[0] < splits[1] && splits[1] < splits[2]) {
                    return Err(format!(
                        "mask.adjustments.parametricCurve.{channel} split1 < split2 < split3 is required after filling defaults 25/50/75"
                    ));
                }
            }
        }
    }
    if let Some(sub_masks) = value["subMasks"].as_array() {
        unique_ids(sub_masks, "sub-mask")?;
        for component in sub_masks {
            if component["type"] == "ai-depth" {
                let params = &component["parameters"];
                if let (Some(min), Some(max)) =
                    (params["minDepth"].as_f64(), params["maxDepth"].as_f64())
                    && min > max
                {
                    return Err("ai-depth minDepth must not exceed maxDepth".into());
                }
            }
        }
    }
    Ok(())
}
fn unique_ids(values: &[Value], kind: &str) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let id = value["id"]
            .as_str()
            .ok_or(format!("{kind}.id must be a string"))?;
        validate_id(id)?;
        if !seen.insert(id) {
            return Err(format!("duplicate {kind} id: {id}"));
        }
    }
    Ok(())
}
pub(super) fn validate_masks(value: &Value) -> Result<(), String> {
    let masks = value.as_array().ok_or("masks must be an array")?;
    if masks.len() > 256 {
        return Err("MCP edits support at most 256 masks".into());
    }
    unique_ids(masks, "mask")?;
    for mask in masks {
        validate_mask(mask, false)?;
    }
    Ok(())
}

pub(super) fn change_masks(
    current: &Value,
    mask: Option<&Value>,
    remove: Option<&str>,
) -> Result<Value, String> {
    let mut values = current
        .as_array()
        .ok_or("current masks must be an array")?
        .clone();
    unique_ids(&values, "mask")?;
    if let Some(mask) = mask {
        validate_mask(mask, true)?;
        let id = mask["id"].as_str().expect("validated id");
        if let Some(index) = values.iter().position(|value| value["id"] == id) {
            let merged = adjustments::merge_adjustments(values[index].clone(), mask.clone())?;
            validate_mask(&merged, false)?;
            values[index] = merged;
        } else {
            validate_mask(mask, false)?;
            values.push(mask.clone());
        }
    } else if let Some(id) = remove {
        values.retain(|value| value["id"] != id);
    }
    if values.len() > 256 {
        return Err("MCP edits support at most 256 masks".into());
    }
    Ok(Value::Array(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn radial(id: &str) -> Value {
        json!({"id":id,"subMasks":[{"id":"component","type":"radial","parameters":
            {"centerX":3250,"centerY":1550,"radiusX":1200,"radiusY":1500,"rotation":-20,"feather":0.8}}],
            "adjustments":{"exposure":0.5,"hsl":{"yellows":{"hue":25,"saturation":-20}}}})
    }
    #[test]
    fn every_typed_parameter_key_survives_exhausted_text_projection() {
        fn fields(schema: &Value) -> Value {
            if let Some(properties) = schema["properties"].as_object() {
                Value::Object(
                    properties
                        .iter()
                        .map(|(key, rule)| (key.clone(), fields(rule)))
                        .collect(),
                )
            } else if let Some(items) = schema.get("items") {
                json!([fields(items)])
            } else {
                json!(false)
            }
        }
        for kind in TYPES {
            let parameters = fields(&parameters_schema(kind));
            let mut payload = json!({"mask":{"subMasks":[{"parameters":parameters}]}});
            for i in 0..20 {
                payload[format!("a{i:02}")] = json!("z".repeat(256));
            }
            let projected = crate::assistant_text::project(&payload);
            assert_eq!(
                projected["mask"]["subMasks"][0]["parameters"], parameters,
                "{kind} structural fields must survive"
            );
        }
    }
    #[test]
    fn exhausted_projection_echo_keeps_typed_masks_valid_and_siblings_exact() {
        let masks: Vec<_> = (0..20)
            .map(|i| {
                let mut mask = radial(&format!("mask-{i}"));
                mask["name"] = json!("z".repeat(256));
                mask["subMasks"][0]["name"] = json!("x".repeat(256));
                mask["subMasks"][0]["parameters"]["isInitialDraw"] = json!(false);
                mask
            })
            .collect();
        let current = json!({"masks":masks});
        validate_masks(&current["masks"]).unwrap();
        let shown = crate::assistant_text::project(&current);
        assert_eq!(shown["masks"][19]["name"], "");
        assert_eq!(
            shown["masks"][19]["subMasks"][0]["parameters"],
            current["masks"][19]["subMasks"][0]["parameters"]
        );
        let mut receipts = crate::assistant_text::NameEchoes::default();
        receipts.remember("photo", "revision", &current, &shown);
        let mut args = json!({"mask":shown["masks"][19]});
        receipts.input(&mut args, &current, "photo", "revision");
        let updated = change_masks(&current["masks"], Some(&args["mask"]), None).unwrap();
        assert_eq!(updated, current["masks"]);
    }
    #[test]
    fn typed_fields_round_trip_into_the_renderer() {
        for kind in TYPES {
            let mut parameters = json!({});
            let schema = parameters_schema(kind);
            for key in schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
            {
                parameters[key] = json!(if key == "feather" { 0.8 } else { 100.0 });
            }
            let value = json!({"id":"container","name":"Typed","visible":true,"invert":false,"opacity":100,
                "adjustments":{"exposure":0.5},"subMasks":[{"id":"component","type":kind,"visible":true,
                    "invert":false,"opacity":100,"mode":"intersect","parameters":parameters}]});
            validate_mask(&value, false).unwrap();
            let parsed: crate::mask_generation::MaskDefinition =
                serde_json::from_value(value).unwrap();
            let encoded = serde_json::to_value(parsed).unwrap();
            assert_eq!(encoded["subMasks"][0]["type"], *kind);
            assert_eq!(encoded["subMasks"][0]["mode"], "intersect");
        }
    }
    #[test]
    fn every_documented_parameter_is_accepted_by_the_renderer() {
        use base64::Engine;
        let png = image::GrayImage::from_pixel(8, 8, image::Luma([128u8]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        png.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let bitmap = base64::engine::general_purpose::STANDARD.encode(bytes.into_inner());
        let warped = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            64,
            64,
            image::Rgb([128, 128, 128]),
        ));
        for kind in TYPES {
            let rules = parameters_schema(kind);
            let mut parameters = json!({});
            for (key, rule) in rules["properties"].as_object().unwrap() {
                parameters[key] = match key.as_str() {
                    "lines" => {
                        json!([{"tool":"brush","brushSize":20,"feather":0.5,"points":[{"x":20,"y":20}]}])
                    }
                    "maskDataBase64" => json!(bitmap),
                    "orientationSteps" => json!(0),
                    "rotation" => json!(0),
                    "radiusX" | "radiusY" | "range" => json!(20),
                    "minDepth" | "minFade" | "maxFade" => json!(0),
                    "maxDepth" => json!(100),
                    "feather" => json!(0.5),
                    _ if rule["type"] == "boolean" || key.starts_with("flip") => json!(false),
                    _ => json!(20),
                };
                if *kind == "flow" && key == "lines" {
                    parameters[key][0]["flow"] = json!(25);
                }
            }
            let definition = json!({"id":"container","name":"Typed","visible":true,"invert":false,"opacity":75,
                "adjustments":{"exposure":0.5},"subMasks":[{"id":"component","name":"Component","type":kind,
                    "visible":true,"invert":false,"opacity":80,"mode":"additive","parameters":parameters}]});
            validate_mask(&definition, false).unwrap();
            let parsed: crate::mask_generation::MaskDefinition =
                serde_json::from_value(definition).unwrap();
            let rendered = crate::mask_generation::generate_mask_bitmap(
                &parsed,
                64,
                64,
                1.0,
                (0.0, 0.0),
                Some(&warped),
            );
            assert!(
                rendered.is_some(),
                "{kind} parameters were not parsed by the renderer"
            );
        }
        let nullable = json!({"id":"sky","subMasks":[{"id":"sky-component","type":"ai-sky","parameters":
            {"maskDataBase64":null,"rotation":null,"flipHorizontal":null,"flipVertical":null,"orientationSteps":null}}]});
        validate_mask(&nullable, false).unwrap();
        let duplicate = json!({"id":"a","subMasks":[{"id":"x","type":"all","parameters":{}},{"id":"x","type":"all","parameters":{}}]});
        assert!(validate_mask(&duplicate, false).is_err());
        assert!(
            validate_mask(
                &json!({"id":"a","subMasks":[{"id":"x","type":"ai-subject","parameters":{}}]}),
                false
            )
            .is_err()
        );
        assert!(validate_mask(&json!({"id":"a","subMasks":[{"id":"x","type":"ai-depth","parameters":{"minDepth":80,"maxDepth":20}}]}), false).is_err());
    }

    #[test]
    fn bad_units_shapes_and_ranges_are_rejected() {
        let mut value = radial("local");
        value["subMasks"][0]["parameters"]["feather"] = json!(80);
        assert!(validate_mask(&value, false).is_err());
        value = radial("local");
        value["opacity"] = json!(-1);
        assert!(validate_mask(&value, false).is_err());
        value = radial("local");
        value["adjustments"]["crop"] = json!(null);
        assert!(validate_mask(&value, false).is_err());
        value = radial("local");
        value["subMasks"][0]["mode"] = json!("divide");
        assert!(validate_mask(&value, false).is_err());
        value = radial("local");
        value["adjustments"]["curves"] = json!({"luma":[]});
        assert!(validate_mask(&value, false).is_err());
        value["adjustments"]["curves"] = json!({"luma":[{"x":100,"y":0},{"x":50,"y":255}]});
        assert!(validate_mask(&value, false).is_err());
        value = radial("local");
        value["adjustments"]["parametricCurve"] =
            json!({"luma":{"split1":80,"split2":20,"split3":60}});
        assert!(validate_mask(&value, false).is_err());
        assert!(validate_crop(&json!({"x":0,"y":0,"width":10,"height":10,"unit":"%"})).is_err());
        assert!(validate_crop(&json!({"x":0,"y":0,"width":0,"height":10})).is_err());
        assert!(
            validate_crop(&json!({"x":0,"y":0,"width":3000,"height":4000,"unit":"px"})).is_ok()
        );
    }
    #[test]
    fn identifiers_match_the_output_opaque_policy_and_mask_limit() {
        let id = "a".repeat(128);
        let value = json!({"id":id,"subMasks":[]});
        validate_mask(&value, false).unwrap();
        assert_eq!(crate::assistant_text::project(&value)["id"], value["id"]);
        for id in [
            "a".repeat(129),
            "é".into(),
            "bad id".into(),
            "bad\0id".into(),
            "".into(),
        ] {
            assert!(validate_mask(&json!({"id":id,"subMasks":[]}), false).is_err());
            assert!(validate_id(&id).is_err());
        }
        let values: Vec<_> = (0..256)
            .map(|i| json!({"id":format!("mask-{i}"),"subMasks":[]}))
            .collect();
        let values = json!(values);
        assert!(change_masks(&values, Some(&json!({"id":"new","subMasks":[]})), None).is_err());
        assert!(change_masks(&values, None, Some("mask-0")).is_ok());
    }

    #[test]
    fn parametric_order_uses_new_defaults_or_merged_existing_values() {
        let invalid = json!({"id":"new","subMasks":[],"adjustments":{"parametricCurve":{"luma":{"split1":90}}}});
        assert!(change_masks(&json!([]), Some(&invalid), None).is_err());
        let existing = json!([{"id":"local","subMasks":[],"adjustments":{"parametricCurve":{"luma":{"split1":10,"split2":80,"split3":90}}}}]);
        let patch = json!({"id":"local","adjustments":{"parametricCurve":{"luma":{"split1":55}}}});
        let changed = change_masks(&existing, Some(&patch), None).unwrap();
        assert_eq!(
            changed[0]["adjustments"]["parametricCurve"]["luma"],
            json!({"split1":55,"split2":80,"split3":90})
        );
        assert!(
            change_masks(
                &existing,
                Some(
                    &json!({"id":"local","adjustments":{"parametricCurve":{"luma":{"split2":95}}}})
                ),
                None
            )
            .is_err()
        );
        let valid = json!({"id":"new","subMasks":[],"adjustments":{"parametricCurve":{"luma":{"split1":10}}}});
        assert!(change_masks(&json!([]), Some(&valid), None).is_ok());
    }

    #[test]
    fn single_id_updates_preserve_siblings_nested_edits_and_array_replacement() {
        let first = radial("first");
        let values = json!([first.clone(), radial("second")]);
        let updated = change_masks(
            &values,
            Some(
                &json!({"id":"second","adjustments":{"hsl":{"yellows":{"hue":-10}}},"subMasks":[]}),
            ),
            None,
        )
        .unwrap();
        assert_eq!(updated[0], first);
        assert_eq!(
            updated[1]["adjustments"]["hsl"]["yellows"],
            json!({"hue":-10,"saturation":-20})
        );
        assert_eq!(updated[1]["subMasks"], json!([]));
        let removed = change_masks(&updated, None, Some("second")).unwrap();
        assert_eq!(removed, json!([first]));
        assert_eq!(
            change_masks(&removed, None, Some("missing")).unwrap(),
            removed
        );
        assert!(
            change_masks(
                &values,
                Some(&json!({"id":"new","adjustments":{"exposure":1}})),
                None
            )
            .is_err()
        );
        assert!(
            change_masks(
                &json!([radial("same"), radial("same")]),
                Some(&radial("same")),
                None
            )
            .is_err()
        );
    }
}
