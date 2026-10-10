use crate::file_management::Preset;
use serde_json::Value;
use std::collections::HashSet;

const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const RR: &str = "urn:rapidroom:preset:1.0";

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn group_name(xmp: &str) -> Option<String> {
    crate::xmp::element_text(xmp, CRS, "Group")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

pub fn exact_adjustments(xmp: &str) -> Result<Option<Value>, String> {
    let Some(text) = crate::xmp::element_text(xmp, RR, "Adjustments").map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| format!("Invalid RapidRoom preset settings: {e}"))?;
    if !value.is_object() {
        return Err("RapidRoom preset settings must be an object".into());
    }
    Ok(Some(value))
}

pub fn unavailable_reason(xmp: &str) -> Result<Option<String>, String> {
    crate::xmp::element_text(xmp, RR, "UnavailableReason").map_err(|e| e.to_string())
}

/// CRS settings use the existing import mappings; RapidRoom's own payload
/// preserves all settings exactly when the file is reimported here.
pub fn serialize(preset: &Preset) -> Result<(String, Vec<String>), String> {
    let obj = preset
        .adjustments
        .as_object()
        .ok_or("Preset adjustments must be an object")?;
    let mut body = String::new();
    let mut mapped = HashSet::new();
    let mut unsupported = Vec::new();
    for (crs, key) in crate::preset_converter::BASIC_MAPPINGS {
        if let Some(value) = obj.get(*key).and_then(Value::as_f64) {
            body.push_str(&format!(" crs:{crs}=\"{value}\""));
            mapped.insert(*key);
        }
    }
    if let Some(hsl) = obj.get("hsl") {
        for (name, band) in [
            ("Red", "reds"),
            ("Orange", "oranges"),
            ("Yellow", "yellows"),
            ("Green", "greens"),
            ("Aqua", "aquas"),
            ("Blue", "blues"),
            ("Purple", "purples"),
            ("Magenta", "magentas"),
        ] {
            for (tag, channel) in [
                ("HueAdjustment", "hue"),
                ("SaturationAdjustment", "saturation"),
                ("LuminanceAdjustment", "luminance"),
            ] {
                if let Some(value) = hsl[band][channel].as_f64() {
                    let crs_value = if channel == "hue" {
                        value / 0.75
                    } else {
                        value
                    };
                    if !(-100.0..=100.0).contains(&crs_value) {
                        unsupported.push(format!("hsl.{band}.{channel}"));
                        continue;
                    }
                    body.push_str(&format!(" crs:{tag}{name}=\"{crs_value}\""));
                }
            }
        }
        mapped.insert("hsl");
    }
    let mut curves = String::new();
    if let Some(obj) = obj.get("curves").and_then(Value::as_object) {
        for (key, tag) in [
            ("luma", "ToneCurvePV2012"),
            ("red", "ToneCurvePV2012Red"),
            ("green", "ToneCurvePV2012Green"),
            ("blue", "ToneCurvePV2012Blue"),
        ] {
            if let Some(points) = obj.get(key).and_then(Value::as_array) {
                curves.push_str(&format!("<crs:{tag}><rdf:Seq>"));
                for point in points {
                    let x = point["x"].as_f64().ok_or("Invalid curve X")?;
                    let y = point["y"].as_f64().ok_or("Invalid curve Y")?;
                    curves.push_str(&format!("<rdf:li>{x}, {y}</rdf:li>"));
                }
                curves.push_str(&format!("</rdf:Seq></crs:{tag}>"));
            }
        }
        mapped.insert("curves");
    }
    unsupported.extend(obj.keys().filter(|k| !mapped.contains(k.as_str())).cloned());
    if let Some(camera) = &preset.camera_model_restriction {
        body.push_str(&format!(
            " crs:CameraModelRestriction=\"{}\"",
            escape(camera)
        ));
    }
    let mut applicability = String::new();
    if let Some(reason) = &preset.unavailable_reason {
        applicability = format!(
            "<rr:UnavailableReason>{}</rr:UnavailableReason>",
            escape(reason)
        );
        if let Some(profile) = reason.strip_prefix("Requires camera profile: ") {
            body.push_str(&format!(" crs:CameraProfile=\"{}\"", escape(profile)));
        } else {
            unsupported.push("unavailableReason".into());
        }
    }
    let payload = escape(&serde_json::to_string(&preset.adjustments).map_err(|e| e.to_string())?);
    let name = escape(&preset.name);
    let id = uuid::Uuid::new_v4().simple().to_string();
    let xmp = format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="{CRS}" xmlns:rr="{RR}" crs:PresetType="Normal" crs:UUID="{id}" crs:HasSettings="True" crs:ProcessVersion="11.0"{body}><crs:Name><rdf:Alt><rdf:li xml:lang="x-default">{name}</rdf:li></rdf:Alt></crs:Name><crs:Group><rdf:Alt><rdf:li xml:lang="x-default">RapidRoom</rdf:li></rdf:Alt></crs:Group>{curves}{applicability}<rr:Adjustments>{payload}</rr:Adjustments></rdf:Description></rdf:RDF></x:xmpmeta>"#
    );
    crate::xmp::validate(&xmp).map_err(|e| e.to_string())?;
    Ok((xmp, unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crs_hue_uses_inverse_units_without_private_payload() {
        let mut preset = crate::preset_converter::convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="1"/>"#,
        )
        .unwrap();
        preset.adjustments = serde_json::json!({"hsl":{"reds":{"hue":40,"saturation":12,"luminance":-8}, "blues":{"hue":90}}});
        let (xml, missing) = serialize(&preset).unwrap();
        assert_eq!(missing, vec!["hsl.blues.hue"]);
        let start = xml.find("<rr:Adjustments>").unwrap();
        let end = xml.find("</rr:Adjustments>").unwrap() + "</rr:Adjustments>".len();
        let crs_only = format!("{}{}", &xml[..start], &xml[end..]);
        let imported = crate::preset_converter::convert_xmp_to_preset(&crs_only).unwrap();
        assert!((imported.adjustments["hsl"]["reds"]["hue"].as_f64().unwrap() - 40.0).abs() < 1e-9);
        assert_eq!(imported.adjustments["hsl"]["reds"]["saturation"], 12);
        assert_eq!(imported.adjustments["hsl"]["reds"]["luminance"], -8);
        assert!(!crs_only.contains("crs:HueAdjustmentBlue="));
    }
    #[test]
    fn roundtrip_preserves_sparse_nested_settings_and_escapes_xml() {
        let mut preset = crate::preset_converter::convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="1.25"/>"#,
        )
        .unwrap();
        preset.name = "A & <B> \"quoted\"".into();
        preset.adjustments =
            serde_json::json!({"exposure":1.25,"hsl":{"reds":{"hue":20}},"masks":[{"name":"A&B"}]});
        let (xml, missing) = serialize(&preset).unwrap();
        assert_eq!(
            exact_adjustments(&xml).unwrap().unwrap(),
            preset.adjustments
        );
        assert!(xml.contains("A &amp; &lt;B&gt; &quot;quoted&quot;"));
        assert_eq!(missing, vec!["masks"]);
        assert_eq!(group_name(&xml).as_deref(), Some("RapidRoom"));
    }
}

#[cfg(test)]
mod namespace_tests {
    #[test]
    fn exact_settings_respect_namespace_aliases_and_reject_non_objects() {
        let good = r#"<root xmlns:p="urn:rapidroom:preset:1.0"><p:Adjustments>{"exposure":0}</p:Adjustments></root>"#;
        assert_eq!(
            super::exact_adjustments(good).unwrap(),
            Some(serde_json::json!({"exposure":0}))
        );
        assert!(
            super::exact_adjustments(r#"<root><Adjustments>{"exposure":3}</Adjustments></root>"#)
                .unwrap()
                .is_none()
        );
        assert!(
            super::exact_adjustments(
                r#"<root><rr:Adjustments>{"exposure":3}</rr:Adjustments></root>"#
            )
            .unwrap()
            .is_none()
        );
        let default_namespace = good
            .replace("xmlns:p=", "xmlns=")
            .replace("p:Adjustments", "Adjustments");
        assert_eq!(
            super::exact_adjustments(&default_namespace).unwrap(),
            super::exact_adjustments(good).unwrap()
        );
        let foreign = good.replace("urn:rapidroom:preset:1.0", "urn:foreign");
        assert!(super::exact_adjustments(&foreign).unwrap().is_none());
        assert!(super::exact_adjustments(&good.replace("{\"exposure\":0}", "[]")).is_err());
        assert!(super::exact_adjustments("<!DOCTYPE root><root/>").is_err());
    }
}
