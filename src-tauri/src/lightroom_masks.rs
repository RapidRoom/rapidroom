//! Lightroom local corrections (`crs:MaskGroupBasedCorrections`) as RapidRAW
//! masks: linear and radial gradients and brush strokes, with their local
//! adjustments. AI masks (`Mask/Image`) keep their pixels in a separate `.acr`
//! file, so corrections that use them are skipped and reported instead.

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use regex::regex;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use uuid::Uuid;

/// Lightroom stores local slider values normalised to -1..1. Exposure spans
/// -4..+4 EV in the UI, so 1.0 is taken as 4 EV. This is unverified: local
/// exposure seems to be normalised differently from global Exposure2012, and
/// the Lightroom calibration set should tune this factor.
pub const LIGHTROOM_LOCAL_EXPOSURE_EV_PER_UNIT: f64 = 4.0;

/// Lightroom brush `Radius` is normalised to the image. Which side it is
/// relative to is unverified; the long side is assumed until the calibration
/// set confirms it.
fn lightroom_brush_radius_reference(width: f64, height: f64) -> f64 {
    width.max(height)
}

/// Lightroom local slider values (-1..1) and the RapidRAW mask adjustment
/// they map to (-100..100).
const LOCAL_ADJUSTMENTS: [(&str, &str); 13] = [
    ("LocalContrast2012", "contrast"),
    ("LocalHighlights2012", "highlights"),
    ("LocalShadows2012", "shadows"),
    ("LocalWhites2012", "whites"),
    ("LocalBlacks2012", "blacks"),
    ("LocalClarity2012", "clarity"),
    ("LocalDehaze", "dehaze"),
    ("LocalTexture", "structure"),
    ("LocalSaturation", "saturation"),
    ("LocalTemperature", "temperature"),
    ("LocalTint", "tint"),
    ("LocalSharpness", "sharpness"),
    ("LocalLuminanceNoise", "lumaNoiseReduction"),
];

/// Local keys without a RapidRAW counterpart that are reported when set.
/// Process 2010 duplicates (LocalExposure, LocalClarity, ...) are left out:
/// Lightroom writes them alongside the 2012 values.
const UNMAPPED_LOCAL_ADJUSTMENTS: [&str; 7] = [
    "LocalHue",
    "LocalToningHue",
    "LocalToningSaturation",
    "LocalMoire",
    "LocalDefringe",
    "LocalGrain",
    "LocalBrightness",
];

/// The uncropped image in RapidRAW's mask space: EXIF orientation applied,
/// then RapidRAW's fine rotation about the centre, before the crop.
#[derive(Debug, Clone, Copy)]
pub struct ImageFrame {
    /// Unoriented pixel size, as in `tiff:ImageWidth`/`tiff:ImageLength`.
    pub width: f64,
    pub height: f64,
    pub orientation: u16,
    /// RapidRAW `rotation` in degrees (clockwise), i.e. `-CropAngle`.
    pub rotation: f64,
}

impl ImageFrame {
    fn oriented_size(&self) -> (f64, f64) {
        if (5..=8).contains(&self.orientation) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Maps a point given in unoriented pixels into mask space.
    fn map_pixel(&self, x: f64, y: f64) -> (f64, f64) {
        self.map_normalized(x / self.width, y / self.height)
    }

    /// Maps a Lightroom point (0..1 of the unoriented image) into mask space.
    fn map_normalized(&self, u: f64, v: f64) -> (f64, f64) {
        let (u, v) = crate::preset_converter::orient_normalized_point(u, v, self.orientation);
        let (width, height) = self.oriented_size();
        let (x, y) = (u * width, v * height);
        if self.rotation.abs() > f64::EPSILON {
            crate::preset_converter::rotate_point_clockwise(
                x,
                y,
                width / 2.0,
                height / 2.0,
                self.rotation,
            )
        } else {
            (x, y)
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaskImportSummary {
    /// Corrections that could be converted, whether or not an image frame was
    /// available to place them.
    pub convertible: usize,
    /// Corrections skipped because they use AI (`Mask/Image`) components.
    pub skipped_ai: usize,
    /// Corrections skipped for other reasons (unsupported components, missing
    /// geometry, no components).
    pub skipped_unsupported: usize,
    /// Converted corrections that also set local adjustments RapidRAW lacks.
    pub with_unmapped_adjustments: usize,
}

pub fn has_mask_group_corrections(xmp_content: &str) -> bool {
    regex!(r"(?s)<crs:MaskGroupBasedCorrections>\s*<rdf:Seq>\s*<rdf:li").is_match(xmp_content)
}

/// Converts every supported correction into a RapidRAW mask container. Without
/// a frame nothing can be placed, so no masks are returned.
pub fn import_lightroom_masks(
    xmp_content: &str,
    frame: Option<&ImageFrame>,
    scale_exposure: impl Fn(&mut Map<String, Value>),
) -> (Vec<Value>, MaskImportSummary) {
    let mut summary = MaskImportSummary::default();
    let mut masks = Vec::new();
    if !has_mask_group_corrections(xmp_content) {
        return (masks, summary);
    }
    let Some(tree) = parse_xml_tree(xmp_content) else {
        summary.skipped_unsupported = 1;
        return (masks, summary);
    };
    let Some(corrections) = find_element(&tree, "crs:MaskGroupBasedCorrections") else {
        return (masks, summary);
    };

    for (index, item) in seq_items(corrections).into_iter().enumerate() {
        let correction = Resource::from_item(item);
        match convert_correction(&correction, index, frame, &scale_exposure) {
            Converted::Mask(mask, unmapped) => {
                summary.convertible += 1;
                if unmapped {
                    summary.with_unmapped_adjustments += 1;
                }
                if let Some(mask) = mask {
                    masks.push(mask);
                }
            }
            Converted::SkippedAi => summary.skipped_ai += 1,
            Converted::SkippedUnsupported => summary.skipped_unsupported += 1,
        }
    }
    (masks, summary)
}

enum Converted {
    /// The mask container (None without a frame) and whether the correction
    /// sets local adjustments that were not transferred.
    Mask(Option<Value>, bool),
    SkippedAi,
    SkippedUnsupported,
}

fn convert_correction(
    correction: &Resource,
    index: usize,
    frame: Option<&ImageFrame>,
    scale_exposure: &impl Fn(&mut Map<String, Value>),
) -> Converted {
    let components: Vec<Resource> = correction
        .structs
        .get("CorrectionMasks")
        .map(|masks| {
            seq_items(masks)
                .into_iter()
                .map(Resource::from_item)
                .collect()
        })
        .unwrap_or_default();
    if components
        .iter()
        .any(|component| component.what() == Some("Mask/Image"))
    {
        return Converted::SkippedAi;
    }
    if components.is_empty() {
        return Converted::SkippedUnsupported;
    }

    // A correction is imported whole or reported. Without a frame (for the
    // report), its components are still checked against a unit frame.
    let frame_or_unit = frame.copied().unwrap_or(ImageFrame {
        width: 1.0,
        height: 1.0,
        orientation: 1,
        rotation: 0.0,
    });
    let Some(sub_masks) = convert_components(&components, &frame_or_unit) else {
        return Converted::SkippedUnsupported;
    };

    let amount = correction
        .number("CorrectionAmount")
        .unwrap_or(1.0)
        .clamp(0.0, 2.0);
    let mut adjustments = Map::new();
    if let Some(exposure) = correction.number("LocalExposure2012") {
        adjustments.insert(
            "exposure".to_string(),
            json!(exposure * LIGHTROOM_LOCAL_EXPOSURE_EV_PER_UNIT * amount),
        );
        scale_exposure(&mut adjustments);
        if let Some(exposure) = adjustments.get("exposure").and_then(Value::as_f64) {
            adjustments.insert(
                "exposure".to_string(),
                json!(round(exposure.clamp(-5.0, 5.0))),
            );
        }
    }
    for (lightroom_key, rapidraw_key) in LOCAL_ADJUSTMENTS {
        if let Some(value) = correction.number(lightroom_key) {
            adjustments.insert(
                rapidraw_key.to_string(),
                json!(round((value * 100.0 * amount).clamp(-100.0, 100.0))),
            );
        }
    }
    adjustments.retain(|_, value| value.as_f64() != Some(0.0));
    let unmapped = UNMAPPED_LOCAL_ADJUSTMENTS
        .iter()
        .any(|key| correction.number(key).is_some_and(|value| value != 0.0));

    if frame.is_none() {
        return Converted::Mask(None, unmapped);
    }
    let name = correction
        .scalars
        .get("CorrectionName")
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("Lightroom mask {}", index + 1));
    let visible = correction
        .scalars
        .get("CorrectionActive")
        .is_none_or(|active| !is_false(active));

    Converted::Mask(
        Some(json!({
            "id": Uuid::new_v4().to_string(),
            "name": name,
            "visible": visible,
            "invert": false,
            "opacity": 100.0,
            "adjustments": adjustments,
            "subMasks": sub_masks,
        })),
        unmapped,
    )
}

/// RapidRAW sub-mask mode and invert flag for a Lightroom component.
/// Lightroom stores Subtract as Intersect (`MaskBlendMode="1"`) with the
/// component inverted. `shape_inverted` is the component's own inversion,
/// such as a radial gradient that applies outside its ellipse.
fn sub_mask_mode(component: &Resource, shape_inverted: bool) -> Option<(&'static str, bool)> {
    let inverted = component
        .scalars
        .get("MaskInverted")
        .is_some_and(|value| is_true(value));
    match component
        .scalars
        .get("MaskBlendMode")
        .map(|value| value.trim())
        .unwrap_or("0")
    {
        "0" => Some(("additive", shape_inverted ^ inverted)),
        "1" if inverted => Some(("subtractive", shape_inverted)),
        "1" => Some(("intersect", shape_inverted)),
        _ => None,
    }
}

/// Consecutive brush strokes with the same mode form one RapidRAW brush, so
/// erase strokes (MaskValue 0) can remove what earlier strokes painted.
struct BrushGroup {
    mode: &'static str,
    invert: bool,
    name: Option<String>,
    lines: Vec<Value>,
}

fn flush_brush(brush: &mut Option<BrushGroup>, sub_masks: &mut Vec<Value>) {
    if let Some(group) = brush.take() {
        sub_masks.push(brush_sub_mask(group));
    }
}

fn convert_components(components: &[Resource], frame: &ImageFrame) -> Option<Vec<Value>> {
    let mut sub_masks: Vec<Value> = Vec::new();
    let mut brush: Option<BrushGroup> = None;

    for component in components {
        match component.what()? {
            "Mask/Paint" => {
                let line = brush_line(component, frame)?;
                if component.number("MaskValue") == Some(0.0) {
                    // Erasing before anything was painted changes nothing.
                    if let Some(group) = brush.as_mut() {
                        group.lines.push(line);
                    }
                    continue;
                }
                let (mode, invert) = sub_mask_mode(component, false)?;
                if !brush
                    .as_ref()
                    .is_some_and(|group| group.mode == mode && group.invert == invert)
                {
                    flush_brush(&mut brush, &mut sub_masks);
                    brush = Some(BrushGroup {
                        mode,
                        invert,
                        name: component.name(),
                        lines: Vec::new(),
                    });
                }
                if let Some(group) = brush.as_mut() {
                    group.lines.push(line);
                }
            }
            "Mask/Gradient" => {
                flush_brush(&mut brush, &mut sub_masks);
                let (mode, invert) = sub_mask_mode(component, false)?;
                sub_masks.push(sub_mask(
                    component,
                    "linear",
                    mode,
                    invert,
                    linear_parameters(component, frame)?,
                ));
            }
            "Mask/CircularGradient" => {
                flush_brush(&mut brush, &mut sub_masks);
                // Flipped="true" applies the effect inside the ellipse, which
                // is RapidRAW's radial mask. The schema default is outside.
                let outside = !component
                    .scalars
                    .get("Flipped")
                    .is_some_and(|value| is_true(value));
                let (mode, invert) = sub_mask_mode(component, outside)?;
                sub_masks.push(sub_mask(
                    component,
                    "radial",
                    mode,
                    invert,
                    radial_parameters(component, frame)?,
                ));
            }
            _ => return None,
        }
    }
    flush_brush(&mut brush, &mut sub_masks);

    (!sub_masks.is_empty()).then_some(sub_masks)
}

fn sub_mask(
    component: &Resource,
    mask_type: &str,
    mode: &str,
    invert: bool,
    parameters: Value,
) -> Value {
    let mut sub_mask = json!({
        "id": Uuid::new_v4().to_string(),
        "type": mask_type,
        "visible": component
            .scalars
            .get("MaskActive")
            .is_none_or(|active| !is_false(active)),
        "invert": invert,
        "opacity": round(component.number("MaskValue").unwrap_or(1.0).clamp(0.0, 1.0) * 100.0),
        "mode": mode,
        "parameters": parameters,
    });
    if let Some(name) = component.name() {
        sub_mask["name"] = json!(name);
    }
    sub_mask
}

fn brush_sub_mask(group: BrushGroup) -> Value {
    let BrushGroup {
        mode,
        invert,
        name,
        lines,
    } = group;
    // RapidRAW's brush paints at full strength. Strokes with less flow or
    // density need its flow brush, which keeps a strength per stroke.
    let full_strength = lines
        .iter()
        .all(|line| line["flow"].as_f64().is_some_and(|flow| flow >= 99.95));
    let (mask_type, lines) = if full_strength {
        let lines = lines
            .into_iter()
            .map(|mut line| {
                if let Some(line) = line.as_object_mut() {
                    line.remove("flow");
                }
                line
            })
            .collect::<Vec<_>>();
        ("brush", lines)
    } else {
        ("flow", lines)
    };
    let mut sub_mask = json!({
        "id": Uuid::new_v4().to_string(),
        "type": mask_type,
        "visible": true,
        "invert": invert,
        "opacity": 100.0,
        "mode": mode,
        "parameters": { "lines": lines },
    });
    if let Some(name) = name {
        sub_mask["name"] = json!(name);
    }
    sub_mask
}

/// Lightroom's gradient is 0 % at the Zero point and 100 % at the Full point.
/// RapidRAW's linear mask runs along the line through `start` and `end`: 50 %
/// on the line, 100 % at `range` pixels to its left (seen from start to end)
/// and 0 % at `range` pixels to its right.
fn linear_parameters(component: &Resource, frame: &ImageFrame) -> Option<Value> {
    let zero = frame.map_normalized(component.number("ZeroX")?, component.number("ZeroY")?);
    let full = frame.map_normalized(component.number("FullX")?, component.number("FullY")?);
    let (dx, dy) = (zero.0 - full.0, zero.1 - full.1);
    let distance = dx.hypot(dy);
    if distance < 1e-9 {
        return None;
    }
    let (ux, uy) = (dx / distance, dy / distance);
    let middle = ((zero.0 + full.0) / 2.0, (zero.1 + full.1) / 2.0);
    let (width, height) = frame.oriented_size();
    let half_length = width.min(height) / 4.0;
    Some(json!({
        "startX": round(middle.0 - uy * half_length),
        "startY": round(middle.1 + ux * half_length),
        "endX": round(middle.0 + uy * half_length),
        "endY": round(middle.1 - ux * half_length),
        "range": round(distance / 2.0),
    }))
}

/// Top/Left/Bottom/Right bound the ellipse before its `Angle` is applied.
/// The angle is taken as counter-clockwise, like CropAngle; unverified.
/// Midpoint and Roundness have no RapidRAW counterpart and are ignored.
fn radial_parameters(component: &Resource, frame: &ImageFrame) -> Option<Value> {
    let top = component.number("Top")?;
    let left = component.number("Left")?;
    let bottom = component.number("Bottom")?;
    let right = component.number("Right")?;
    if right <= left || bottom <= top {
        return None;
    }
    let angle = component.number("Angle").unwrap_or(0.0).to_radians();
    let center = (
        (left + right) / 2.0 * frame.width,
        (top + bottom) / 2.0 * frame.height,
    );
    let radius_x = (right - left) / 2.0 * frame.width;
    let radius_y = (bottom - top) / 2.0 * frame.height;
    let (sin, cos) = angle.sin_cos();
    let axis_x_end = (center.0 + cos * radius_x, center.1 - sin * radius_x);
    let axis_y_end = (center.0 + sin * radius_y, center.1 + cos * radius_y);

    let mapped_center = frame.map_pixel(center.0, center.1);
    let mapped_x = frame.map_pixel(axis_x_end.0, axis_x_end.1);
    let mapped_y = frame.map_pixel(axis_y_end.0, axis_y_end.1);
    let axis_x = (mapped_x.0 - mapped_center.0, mapped_x.1 - mapped_center.1);
    let axis_y = (mapped_y.0 - mapped_center.0, mapped_y.1 - mapped_center.1);
    let feather = component
        .number("Feather")
        .unwrap_or(50.0)
        .clamp(0.0, 100.0)
        / 100.0;

    Some(json!({
        "centerX": round(mapped_center.0),
        "centerY": round(mapped_center.1),
        "radiusX": round(axis_x.0.hypot(axis_x.1)),
        "radiusY": round(axis_y.0.hypot(axis_y.1)),
        "rotation": round(normalize_degrees(axis_x.1.atan2(axis_x.0).to_degrees())),
        "feather": round(feather),
    }))
}

/// One Lightroom paint stroke as a RapidRAW brush line. `flow` (0..100) is
/// kept for the flow brush and dropped again for a full-strength brush.
fn brush_line(component: &Resource, frame: &ImageFrame) -> Option<Value> {
    let radius = component.number("Radius").filter(|radius| *radius > 0.0)?;
    let dabs = component.structs.get("Dabs")?;
    let points: Vec<Value> = seq_items(dabs)
        .into_iter()
        .filter_map(|dab| {
            let mut parts = dab.text.split_whitespace();
            if parts.next()? != "d" {
                return None;
            }
            let u = parts.next()?.parse::<f64>().ok()?;
            let v = parts.next()?.parse::<f64>().ok()?;
            let (x, y) = frame.map_normalized(u, v);
            Some(json!({ "x": round(x), "y": round(y) }))
        })
        .collect();
    if points.is_empty() {
        return None;
    }
    let erases = component.number("MaskValue") == Some(0.0);
    let density = if erases {
        1.0
    } else {
        component.number("MaskValue").unwrap_or(1.0)
    };
    let flow = component.number("Flow").unwrap_or(1.0);
    let feather = 1.0 - component.number("CenterWeight").unwrap_or(0.5);
    let reference = lightroom_brush_radius_reference(frame.width, frame.height);

    Some(json!({
        "tool": if erases { "eraser" } else { "brush" },
        "brushSize": round(radius * 2.0 * reference),
        "feather": round(feather.clamp(0.0, 1.0)),
        "flow": round((flow * density).clamp(0.0, 1.0) * 100.0),
        "points": points,
    }))
}

fn normalize_degrees(degrees: f64) -> f64 {
    let degrees = degrees.rem_euclid(360.0);
    if degrees > 180.0 {
        degrees - 360.0
    } else {
        degrees
    }
}

fn round(value: f64) -> f64 {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn is_true(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("true") || value == "1"
}

fn is_false(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("false") || value == "0"
}

#[derive(Debug, Default)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
}

fn element_from_start(start: &BytesStart) -> Element {
    Element {
        name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
        attributes: start
            .attributes()
            .with_checks(false)
            .filter_map(Result::ok)
            .map(|attribute| {
                (
                    String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map(|value| value.into_owned())
                        .unwrap_or_default(),
                )
            })
            .collect(),
        ..Element::default()
    }
}

/// A minimal element tree; the regex-based attribute parsing elsewhere can't
/// tell nested mask components apart.
fn parse_xml_tree(content: &str) -> Option<Element> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);
    let mut stack = vec![Element::default()];
    loop {
        match reader.read_event().ok()? {
            Event::Start(start) => stack.push(element_from_start(&start)),
            Event::Empty(start) => {
                let element = element_from_start(&start);
                stack.last_mut()?.children.push(element);
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    return None;
                }
                let element = stack.pop()?;
                stack.last_mut()?.children.push(element);
            }
            Event::Text(text) => {
                if let Ok(text) = text.decode() {
                    stack.last_mut()?.text.push_str(&text);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    (stack.len() == 1).then(|| stack.pop()).flatten()
}

fn find_element<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    if element.name == name {
        return Some(element);
    }
    element
        .children
        .iter()
        .filter(|child| child.name != "crs:Look")
        .find_map(|child| find_element(child, name))
}

/// The `rdf:li` items of a property holding an `rdf:Seq` or `rdf:Bag`.
fn seq_items(property: &Element) -> Vec<&Element> {
    property
        .children
        .iter()
        .filter(|child| child.name == "rdf:Seq" || child.name == "rdf:Bag")
        .flat_map(|list| list.children.iter())
        .filter(|item| item.name == "rdf:li")
        .collect()
}

/// The `crs:` properties of an RDF resource, whether written as attributes, as
/// simple elements, or inside a nested `rdf:Description`.
struct Resource<'a> {
    scalars: HashMap<String, String>,
    structs: HashMap<String, &'a Element>,
}

impl<'a> Resource<'a> {
    fn from_item(item: &'a Element) -> Self {
        let mut resource = Resource {
            scalars: HashMap::new(),
            structs: HashMap::new(),
        };
        resource.collect(item);
        for description in item
            .children
            .iter()
            .filter(|child| child.name == "rdf:Description")
        {
            resource.collect(description);
        }
        resource
    }

    fn collect(&mut self, element: &'a Element) {
        for (key, value) in &element.attributes {
            if let Some(key) = key.strip_prefix("crs:") {
                self.scalars.insert(key.to_string(), value.clone());
            }
        }
        for child in &element.children {
            let Some(key) = child.name.strip_prefix("crs:") else {
                continue;
            };
            if child.children.is_empty() {
                self.scalars
                    .entry(key.to_string())
                    .or_insert_with(|| child.text.trim().to_string());
            } else {
                self.structs.insert(key.to_string(), child);
            }
        }
    }

    fn what(&self) -> Option<&str> {
        self.scalars.get("What").map(|value| value.trim())
    }

    fn name(&self) -> Option<String> {
        self.scalars
            .get("MaskName")
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.scalars
            .get(key)?
            .trim()
            .trim_start_matches('+')
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::preset_converter::{
        convert_xmp_sidecar_to_preset, lightroom_settings_not_transferred,
    };

    /// A Lightroom Classic sidecar for a 6000x4000 raw with the given
    /// `crs:MaskGroupBasedCorrections` items, hand-written from the crs schema.
    pub(crate) fn sidecar_with_corrections(attributes: &str, corrections: &str) -> String {
        format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
   xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
   xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   tiff:ImageWidth="6000"
   tiff:ImageLength="4000"
   crs:ProcessVersion="15.4"
   crs:Exposure2012="+0.30"
   {attributes}>
   <crs:MaskGroupBasedCorrections>
    <rdf:Seq>
     {corrections}
    </rdf:Seq>
   </crs:MaskGroupBasedCorrections>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
        )
    }

    fn correction(attributes: &str, components: &str) -> String {
        format!(
            r#"<rdf:li>
      <rdf:Description
       crs:What="Correction"
       crs:CorrectionAmount="1"
       crs:CorrectionActive="true"
       {attributes}>
      <crs:CorrectionMasks>
       <rdf:Seq>
        {components}
       </rdf:Seq>
      </crs:CorrectionMasks>
      </rdf:Description>
     </rdf:li>"#
        )
    }

    const LINEAR_GRADIENT: &str = r#"<rdf:li
         crs:What="Mask/Gradient"
         crs:MaskActive="true"
         crs:MaskName="Linear Gradient 1"
         crs:MaskBlendMode="0"
         crs:MaskInverted="false"
         crs:MaskValue="1"
         crs:ZeroX="0.5"
         crs:ZeroY="0.6"
         crs:FullX="0.5"
         crs:FullY="0.2"/>"#;

    const RADIAL_GRADIENT: &str = r#"<rdf:li
         crs:What="Mask/CircularGradient"
         crs:MaskActive="true"
         crs:MaskName="Radial Gradient 1"
         crs:MaskBlendMode="0"
         crs:MaskInverted="false"
         crs:MaskValue="1"
         crs:Top="0.25"
         crs:Left="0.25"
         crs:Bottom="0.75"
         crs:Right="0.75"
         crs:Angle="0"
         crs:Midpoint="50"
         crs:Roundness="0"
         crs:Feather="50"
         crs:Flipped="true"
         crs:Version="2"/>"#;

    fn without_ids(mut value: Value) -> Value {
        match &mut value {
            Value::Object(object) => {
                object.remove("id");
                for child in object.values_mut() {
                    *child = without_ids(child.take());
                }
            }
            Value::Array(items) => {
                for item in items.iter_mut() {
                    *item = without_ids(item.take());
                }
            }
            _ => {}
        }
        value
    }

    fn imported_masks(xmp: &str) -> Value {
        let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();
        let masks = preset
            .adjustments
            .get("masks")
            .cloned()
            .unwrap_or(Value::Null);
        // The renderer drops every mask if one of them fails to parse.
        assert_eq!(
            crate::mask_generation::parse_mask_definitions(&preset.adjustments).len(),
            masks.as_array().map_or(0, Vec::len)
        );
        without_ids(masks)
    }

    fn not_transferred(xmp: &str) -> Vec<&'static str> {
        let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();
        lightroom_settings_not_transferred(xmp, &preset)
    }

    #[test]
    fn imports_a_linear_gradient_with_its_local_adjustments() {
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:CorrectionName="Sky"
       crs:LocalExposure2012="-0.125"
       crs:LocalContrast2012="0.2"
       crs:LocalHighlights2012="-0.35"
       crs:LocalShadows2012="0"
       crs:LocalClarity2012="0.1"
       crs:LocalDehaze="0.15"
       crs:LocalTemperature="-0.1"
       crs:LocalTint="0.05"
       crs:LocalSaturation="0.25"
       crs:LocalCurveRefineSaturation="100""#,
                LINEAR_GRADIENT,
            ),
        );

        assert_eq!(
            imported_masks(&xmp),
            json!([{
                "name": "Sky",
                "visible": true,
                "invert": false,
                "opacity": 100.0,
                "adjustments": {
                    "exposure": -0.5,
                    "contrast": 20.0,
                    "highlights": -35.0,
                    "clarity": 10.0,
                    "dehaze": 15.0,
                    "temperature": -10.0,
                    "tint": 5.0,
                    "saturation": 25.0,
                },
                "subMasks": [{
                    "type": "linear",
                    "name": "Linear Gradient 1",
                    "visible": true,
                    "invert": false,
                    "opacity": 100.0,
                    "mode": "additive",
                    // 100 % at Full (y = 800), 0 % at Zero (y = 2400).
                    "parameters": {
                        "startX": 2000.0,
                        "startY": 1600.0,
                        "endX": 4000.0,
                        "endY": 1600.0,
                        "range": 800.0,
                    },
                }],
            }])
        );
        assert!(not_transferred(&xmp).is_empty());
    }

    #[test]
    fn imports_an_inverted_radial_gradient() {
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:CorrectionName="Vignette" crs:LocalExposure2012="-0.25""#,
                &RADIAL_GRADIENT
                    .replace(r#"crs:MaskInverted="false""#, r#"crs:MaskInverted="true""#)
                    .replace(r#"crs:MaskValue="1""#, r#"crs:MaskValue="0.8""#)
                    .replace(r#"crs:Feather="50""#, r#"crs:Feather="40""#),
            ),
        );

        assert_eq!(
            imported_masks(&xmp),
            json!([{
                "name": "Vignette",
                "visible": true,
                "invert": false,
                "opacity": 100.0,
                "adjustments": { "exposure": -1.0 },
                "subMasks": [{
                    "type": "radial",
                    "name": "Radial Gradient 1",
                    "visible": true,
                    "invert": true,
                    "opacity": 80.0,
                    "mode": "additive",
                    "parameters": {
                        "centerX": 3000.0,
                        "centerY": 2000.0,
                        "radiusX": 1500.0,
                        "radiusY": 1000.0,
                        "rotation": 0.0,
                        "feather": 0.4,
                    },
                }],
            }])
        );
    }

    #[test]
    fn a_radial_gradient_without_flipped_applies_outside_the_ellipse() {
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:LocalExposure2012="0.1""#,
                &RADIAL_GRADIENT.replace(r#"crs:Flipped="true""#, r#"crs:Flipped="false""#),
            ),
        );
        let masks = imported_masks(&xmp);
        assert_eq!(masks[0]["name"], json!("Lightroom mask 1"));
        assert_eq!(masks[0]["subMasks"][0]["invert"], json!(true));
    }

    #[test]
    fn imports_subtract_and_intersect_components() {
        let subtract_linear = LINEAR_GRADIENT
            .replace(r#"crs:MaskBlendMode="0""#, r#"crs:MaskBlendMode="1""#)
            .replace(r#"crs:MaskInverted="false""#, r#"crs:MaskInverted="true""#)
            .replace("Linear Gradient 1", "Linear Gradient 2");
        let intersect_radial = RADIAL_GRADIENT
            .replace(r#"crs:MaskBlendMode="0""#, r#"crs:MaskBlendMode="1""#)
            .replace(r#"crs:MaskActive="true""#, r#"crs:MaskActive="false""#)
            .replace("Radial Gradient 1", "Radial Gradient 2");
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:CorrectionName="Subject" crs:CorrectionActive="false" crs:LocalShadows2012="0.3""#,
                &format!("{RADIAL_GRADIENT}\n{subtract_linear}\n{intersect_radial}"),
            ),
        );

        let masks = imported_masks(&xmp);
        assert_eq!(masks[0]["visible"], json!(false));
        assert_eq!(masks[0]["adjustments"], json!({ "shadows": 30.0 }));
        let modes: Vec<_> = masks[0]["subMasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sub_mask| {
                (
                    sub_mask["name"].as_str().unwrap(),
                    sub_mask["type"].as_str().unwrap(),
                    sub_mask["mode"].as_str().unwrap(),
                    sub_mask["invert"].as_bool().unwrap(),
                    sub_mask["visible"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            modes,
            vec![
                ("Radial Gradient 1", "radial", "additive", false, true),
                ("Linear Gradient 2", "linear", "subtractive", false, true),
                ("Radial Gradient 2", "radial", "intersect", false, false),
            ]
        );
    }

    #[test]
    fn imports_brush_strokes_with_erasing() {
        let stroke = |value: &str, radius: &str, dabs: &[&str]| {
            let dabs: String = dabs
                .iter()
                .map(|dab| format!("<rdf:li>d {dab}</rdf:li>"))
                .collect();
            format!(
                r#"<rdf:li
         crs:What="Mask/Paint"
         crs:MaskActive="true"
         crs:MaskName="Brush 1"
         crs:MaskBlendMode="0"
         crs:MaskInverted="false"
         crs:MaskValue="{value}"
         crs:Radius="{radius}"
         crs:Flow="1"
         crs:CenterWeight="0.25">
        <crs:Dabs>
         <rdf:Seq>{dabs}</rdf:Seq>
        </crs:Dabs>
        </rdf:li>"#
            )
        };
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:CorrectionName="Face" crs:LocalExposure2012="0.0625" crs:LocalTexture="-0.2""#,
                &[
                    stroke("1", "0.01", &["0.1 0.1", "0.2 0.1"]),
                    stroke("0", "0.005", &["0.15 0.1"]),
                    stroke("1", "0.01", &["0.5 0.5"]),
                ]
                .join("\n"),
            ),
        );

        assert_eq!(
            imported_masks(&xmp),
            json!([{
                "name": "Face",
                "visible": true,
                "invert": false,
                "opacity": 100.0,
                "adjustments": { "exposure": 0.25, "structure": -20.0 },
                "subMasks": [{
                    "type": "brush",
                    "name": "Brush 1",
                    "visible": true,
                    "invert": false,
                    "opacity": 100.0,
                    "mode": "additive",
                    "parameters": {
                        "lines": [
                            {
                                "tool": "brush",
                                "brushSize": 120.0,
                                "feather": 0.75,
                                "points": [{ "x": 600.0, "y": 400.0 }, { "x": 1200.0, "y": 400.0 }],
                            },
                            {
                                "tool": "eraser",
                                "brushSize": 60.0,
                                "feather": 0.75,
                                "points": [{ "x": 900.0, "y": 400.0 }],
                            },
                            {
                                "tool": "brush",
                                "brushSize": 120.0,
                                "feather": 0.75,
                                "points": [{ "x": 3000.0, "y": 2000.0 }],
                            },
                        ],
                    },
                }],
            }])
        );

        let partial_flow = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:LocalExposure2012="0.0625""#,
                &stroke("1", "0.01", &["0.1 0.1"]).replace(r#"crs:Flow="1""#, r#"crs:Flow="0.5""#),
            ),
        );
        let masks = imported_masks(&partial_flow);
        assert_eq!(masks[0]["subMasks"][0]["type"], json!("flow"));
        assert_eq!(
            masks[0]["subMasks"][0]["parameters"]["lines"][0]["flow"],
            json!(50.0)
        );
    }

    #[test]
    fn skips_and_reports_corrections_with_ai_components() {
        let ai_subject = r#"<rdf:li
         crs:What="Mask/Image"
         crs:MaskActive="true"
         crs:MaskName="Subject 1"
         crs:MaskBlendMode="0"
         crs:MaskInverted="false"
         crs:MaskValue="1"
         crs:MaskSubType="1"
         crs:ReferencePoint="0.5 0.5"/>"#;
        let object_stroke = r#"<rdf:li
         crs:What="Mask/Paint"
         crs:MaskValue="1"
         crs:Radius="0.02"
         crs:Flow="1"
         crs:CenterWeight="0">
        <crs:Dabs><rdf:Seq><rdf:li>d 0.5 0.5</rdf:li></rdf:Seq></crs:Dabs>
        </rdf:li>"#;
        let xmp = sidecar_with_corrections(
            "",
            &[
                correction(
                    r#"crs:CorrectionName="Subject" crs:LocalExposure2012="0.1""#,
                    &format!("{ai_subject}\n{object_stroke}"),
                ),
                correction(
                    r#"crs:CorrectionName="Sky" crs:LocalExposure2012="-0.1""#,
                    LINEAR_GRADIENT,
                ),
            ]
            .join("\n"),
        );

        let masks = imported_masks(&xmp);
        assert_eq!(masks.as_array().unwrap().len(), 1);
        assert_eq!(masks[0]["name"], json!("Sky"));
        assert_eq!(not_transferred(&xmp), vec!["aiMasks"]);
    }

    #[test]
    fn skips_and_reports_corrections_with_unsupported_components() {
        let xmp = sidecar_with_corrections(
            "",
            &[
                correction(
                    r#"crs:CorrectionName="Range""#,
                    r#"<rdf:li crs:What="Mask/RangeMask" crs:MaskValue="1"/>"#,
                ),
                correction(r#"crs:CorrectionName="Sky""#, LINEAR_GRADIENT),
            ]
            .join("\n"),
        );

        let masks = imported_masks(&xmp);
        assert_eq!(masks.as_array().unwrap().len(), 1);
        assert_eq!(masks[0]["name"], json!("Sky"));
        assert_eq!(not_transferred(&xmp), vec!["masks"]);
    }

    #[test]
    fn reports_local_adjustments_without_a_rapidraw_counterpart() {
        let xmp = sidecar_with_corrections(
            "",
            &correction(
                r#"crs:LocalExposure2012="0.1" crs:LocalMoire="0.4""#,
                LINEAR_GRADIENT,
            ),
        );
        assert_eq!(not_transferred(&xmp), vec!["localAdjustments"]);
    }

    #[test]
    fn places_masks_through_orientation_and_crop_angle() {
        let xmp = sidecar_with_corrections(
            r#"tiff:Orientation="6""#,
            &correction(r#"crs:LocalExposure2012="0.1""#, RADIAL_GRADIENT),
        );
        let parameters = &imported_masks(&xmp)[0]["subMasks"][0]["parameters"];
        // The portrait frame is 4000x6000; the ellipse's long axis turns with it.
        assert_eq!(parameters["centerX"], json!(2000.0));
        assert_eq!(parameters["centerY"], json!(3000.0));
        assert_eq!(parameters["radiusX"], json!(1500.0));
        assert_eq!(parameters["radiusY"], json!(1000.0));
        assert_eq!(parameters["rotation"], json!(90.0));

        let straightened = sidecar_with_corrections(
            r#"crs:HasCrop="True"
   crs:CropLeft="0.1"
   crs:CropTop="0.1"
   crs:CropRight="0.9"
   crs:CropBottom="0.9"
   crs:CropAngle="10""#,
            &correction(
                r#"crs:LocalExposure2012="0.1""#,
                &format!("{RADIAL_GRADIENT}\n{LINEAR_GRADIENT}"),
            ),
        );
        let preset = convert_xmp_sidecar_to_preset(&straightened).unwrap();
        assert_eq!(preset.adjustments["rotation"], json!(-10.0));
        let sub_masks = &preset.adjustments["masks"][0]["subMasks"];
        // Rotating about the image centre keeps the centred ellipse in place.
        assert_eq!(sub_masks[0]["parameters"]["centerX"], json!(3000.0));
        assert_eq!(sub_masks[0]["parameters"]["centerY"], json!(2000.0));
        assert_eq!(sub_masks[0]["parameters"]["rotation"], json!(-10.0));
        // The gradient turns by the same angle: its 100 % side still faces Full.
        let linear = &sub_masks[1]["parameters"];
        let direction = (
            linear["endX"].as_f64().unwrap() - linear["startX"].as_f64().unwrap(),
            linear["endY"].as_f64().unwrap() - linear["startY"].as_f64().unwrap(),
        );
        let angle = direction.1.atan2(direction.0).to_degrees();
        assert!((angle - -10.0).abs() < 0.01, "{angle}");
        assert_eq!(linear["range"], json!(800.0));
    }

    #[test]
    fn leaves_masks_out_of_reusable_presets() {
        let xmp = sidecar_with_corrections(
            "",
            &correction(r#"crs:LocalExposure2012="0.1""#, LINEAR_GRADIENT),
        );
        let preset = crate::preset_converter::convert_xmp_to_preset(&xmp).unwrap();
        assert!(preset.adjustments.get("masks").is_none());
    }
}
