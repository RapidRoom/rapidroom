//! Full photo edits beside the Lightroom projection. Inspired by the MIT
//! AutoShade payload/reconciliation design; this implements RapidRoom's schema.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub(crate) const URI: &str = "urn:rapidroom:edit:1.0";
const OWNED: &[&str] = &["Format", "Recipe", "RecipeSha256"];
const MAX_JSON: usize = 12 * 1024 * 1024;
const MAX_RASTERS: usize = 6 * 1024 * 1024;
const TOLERANCE: f64 = 2e-4;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {
    pub adjustments: Value,
    /// What the normal importer understood from the CRS fields we wrote.
    /// Comparing to this, rather than to native numbers, retains exact values
    /// through conversion and distinguishes Lightroom edits from rewrites.
    pub projection: Value,
}

fn check(value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("The RapidRoom edit must be an object".into());
    }
    fn visit(value: &Value, bytes: &mut usize) -> Result<(), String> {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == "maskDataBase64"
                        && let Some(data) = value.as_str()
                    {
                        let encoded = data.split_once(',').map_or(data, |(_, body)| body);
                        if encoded.len() > MAX_RASTERS * 4 / 3 {
                            return Err("Embedded masks exceed the 6 MiB round-trip limit".into());
                        }
                        let raster = STANDARD
                            .decode(encoded)
                            .map_err(|_| "Invalid embedded mask data")?;
                        *bytes = bytes
                            .checked_add(raster.len())
                            .ok_or("Embedded masks are too large")?;
                        if *bytes > MAX_RASTERS {
                            return Err("Embedded masks exceed the 6 MiB round-trip limit".into());
                        }
                    }
                    visit(value, bytes)?;
                }
            }
            Value::Array(array) => {
                for item in array {
                    visit(item, bytes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value, &mut 0)
}

fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>, String> {
    struct BoundedJson(Vec<u8>);
    impl Write for BoundedJson {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_JSON - self.0.len() {
                return Err(std::io::Error::other(
                    "The full edit exceeds the 12 MiB round-trip limit",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bounded = BoundedJson(Vec::new());
    serde_json::to_writer(&mut bounded, value).map_err(|e| e.to_string())?;
    Ok(bounded.0)
}

pub(crate) fn validate_edit(value: &Value) -> Result<(), String> {
    check(value)?;
    bounded_json(value).map(|_| ())
}

pub(crate) fn embed(content: &str, payload: &Payload) -> Result<String, String> {
    check(&payload.adjustments)?;
    check(&payload.projection)?;
    let json = bounded_json(payload)?;
    let checksum = hex::encode(Sha256::digest(&json));
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json).map_err(|e| e.to_string())?;
    let encoded = STANDARD.encode(encoder.finish().map_err(|e| e.to_string())?);
    crate::xmp::replace_image_properties(
        content,
        URI,
        "rr",
        OWNED,
        &[
            ("Format", "1".into()),
            ("Recipe", encoded),
            ("RecipeSha256", checksum),
        ],
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn read(content: &str) -> Result<Option<Payload>, String> {
    let property = |key| crate::xmp::image_property(content, URI, key).map_err(|e| e.to_string());
    let recipe = property("Recipe")?;
    let format = property("Format")?;
    let checksum = property("RecipeSha256")?;
    if recipe.is_none() && format.is_none() && checksum.is_none() {
        return Ok(None);
    }
    if format.as_deref() != Some("1") {
        return Err("Unsupported RapidRoom round-trip payload version".into());
    }
    let packed = STANDARD
        .decode(recipe.ok_or("Missing full-edit payload")?.trim())
        .map_err(|_| "The full-edit payload is not valid base64")?;
    let mut decoder = ZlibDecoder::new(packed.as_slice());
    let mut json = Vec::new();
    decoder
        .by_ref()
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|e| e.to_string())?;
    if json.len() > MAX_JSON {
        return Err("The full-edit payload inflates past the 12 MiB limit".into());
    }
    if decoder.total_in() != packed.len() as u64 {
        return Err("Trailing data in the full-edit payload".into());
    }
    if checksum.as_deref() != Some(hex::encode(Sha256::digest(&json)).as_str()) {
        return Err("The full-edit payload checksum does not match".into());
    }
    let payload: Payload = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    check(&payload.adjustments)?;
    check(&payload.projection)?;
    Ok(Some(payload))
}

fn equivalent(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(Value::Number(a)), Some(Value::Number(b))) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= TOLERANCE,
            _ => a == b,
        },
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(Some(a), Some(b)))
        }
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| key == "id" || equivalent(Some(a), b.get(key)))
        }
        _ => a == b,
    }
}

/// Lightroom changes win for projected leaves. Unchanged and unprojected
/// leaves restore the authored values; deleting a projected leaf resets it.
pub(crate) fn restore(payload: &Payload, current: &Value, defaults: &Value) -> Value {
    fn merge(
        p: Option<&Value>,
        w: Option<&Value>,
        c: Option<&Value>,
        d: Option<&Value>,
    ) -> Option<Value> {
        if equivalent(w, c) {
            return p.cloned();
        }
        if let Some(Value::Object(p)) = p
            && (w.is_none() || w.is_some_and(Value::is_object))
            && c.is_some_and(Value::is_object)
        {
            let mut result = Map::new();
            let current = c.and_then(Value::as_object).expect("checked object");
            let mut keys: Vec<&String> = p.keys().chain(current.keys()).collect();
            if let Some(w) = w.and_then(Value::as_object) {
                keys.extend(w.keys());
            }
            keys.sort();
            keys.dedup();
            for key in keys {
                if key == "id" {
                    if let Some(value) = p.get(key).or_else(|| current.get(key)) {
                        result.insert(key.clone(), value.clone());
                    }
                    continue;
                }
                if matches!(key.as_str(), "masks" | "subMasks") {
                    let empty = Vec::new();
                    let authored = p.get(key).and_then(Value::as_array).unwrap_or(&empty);
                    let written = w
                        .and_then(|w| w.get(key))
                        .and_then(Value::as_array)
                        .unwrap_or(&empty);
                    let returned = current.get(key).and_then(Value::as_array).unwrap_or(&empty);
                    if equivalent(w.and_then(|w| w.get(key)), current.get(key)) {
                        if let Some(value) = p.get(key) {
                            result.insert(key.clone(), value.clone());
                        }
                    } else {
                        result.insert(key.clone(), masks(authored, written, returned));
                    }
                    continue;
                }
                if let Some(value) = merge(
                    p.get(key),
                    w.and_then(|w| w.get(key)),
                    current.get(key),
                    d.and_then(|d| d.get(key)),
                ) {
                    result.insert(key.clone(), value);
                }
            }
            return Some(Value::Object(result));
        }
        c.or(d).cloned()
    }
    fn masks(p: &[Value], w: &[Value], c: &[Value]) -> Value {
        fn pair(items: &[Value], item: &Value, preferred: usize, used: &[bool]) -> Option<usize> {
            let id = item.get("id").and_then(Value::as_str);
            let name = item.get("name").and_then(Value::as_str);
            let kind = item.get("type").and_then(Value::as_str);
            items
                .iter()
                .enumerate()
                .find(|(i, candidate)| {
                    !used[*i] && id.is_some() && candidate.get("id").and_then(Value::as_str) == id
                })
                .map(|(i, _)| i)
                .or_else(|| {
                    items
                        .iter()
                        .enumerate()
                        .find(|(i, candidate)| {
                            !used[*i]
                                && name.is_some()
                                && candidate.get("name").and_then(Value::as_str) == name
                        })
                        .map(|(i, _)| i)
                })
                .or_else(|| {
                    (preferred < items.len()
                        && !used[preferred]
                        && kind.is_some()
                        && items[preferred].get("type").and_then(Value::as_str) == kind)
                        .then_some(preferred)
                })
        }
        let mut w_used = vec![false; w.len()];
        let mut c_used = vec![false; c.len()];
        let mut out = Vec::new();
        for (i, authored) in p.iter().enumerate() {
            let Some(j) = pair(w, authored, i, &w_used) else {
                out.push((None, authored.clone()));
                continue;
            };
            w_used[j] = true;
            if let Some(k) = pair(c, &w[j], j, &c_used) {
                c_used[k] = true;
                if let Some(value) = merge(Some(authored), Some(&w[j]), Some(&c[k]), None) {
                    out.push((Some(k), value));
                }
            }
        }
        for (k, current) in c.iter().enumerate() {
            if !c_used[k] {
                out.push((Some(k), current.clone()));
            }
        }
        let mut projected: Vec<_> = out
            .iter()
            .filter_map(|(index, value)| index.map(|index| (index, value.clone())))
            .collect();
        projected.sort_by_key(|(index, _)| *index);
        let mut projected = projected.into_iter();
        Value::Array(
            out.into_iter()
                .map(|(index, value)| {
                    if index.is_some() {
                        projected.next().expect("one projected value per slot").1
                    } else {
                        value
                    }
                })
                .collect(),
        )
    }
    merge(
        Some(&payload.adjustments),
        Some(&payload.projection),
        Some(current),
        Some(defaults),
    )
    .unwrap_or_else(|| defaults.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unchanged_rewrite_restores_exact_native_edit_and_corruption_is_rejected() {
        let payload = Payload {
            adjustments: json!({"exposure":0.1234567,"halationAmount":42,"masks":[{"name":"Sky & land","subMasks":[]}]}),
            projection: json!({"exposure":0.12}),
        };
        let xml = embed(crate::xmp::SKELETON, &payload).unwrap();
        let read_back = read(&xml).unwrap().unwrap();
        assert_eq!(
            restore(
                &read_back,
                &json!({"exposure":0.12001}),
                &json!({"exposure":0})
            ),
            payload.adjustments
        );
        assert!(read(&xml.replace("rr:Format=\"1\"", "rr:Format=\"2\"")).is_err());
        let broken = xml.replace("rr:RecipeSha256=\"", "rr:RecipeSha256=\"0");
        assert!(read(&broken).is_err());
    }

    #[test]
    fn lightroom_changed_added_and_removed_leaves_win_without_dropping_unprojected_fields() {
        let payload = Payload {
            adjustments: json!({"exposure":0.333333,"contrast":10,"halationAmount":42,"hsl":{"reds":{"hue":17.123,"saturation":30}}}),
            projection: json!({"exposure":0.33,"contrast":10,"hsl":{"reds":{"hue":17,"saturation":30}}}),
        };
        let current =
            json!({"exposure":1.25,"shadows":20,"hsl":{"reds":{"hue":17.00001,"saturation":0}}});
        let result = restore(&payload, &current, &json!({"contrast":0}));
        assert_eq!(
            result,
            json!({"exposure":1.25,"contrast":0,"shadows":20,"halationAmount":42,"hsl":{"reds":{"hue":17.123,"saturation":0}}})
        );
    }

    #[test]
    fn absent_payload_is_ordinary_xmp_and_invalid_shapes_are_rejected() {
        assert!(read(crate::xmp::SKELETON).unwrap().is_none());
        let payload = Payload {
            adjustments: json!([]),
            projection: json!({}),
        };
        assert!(embed(crate::xmp::SKELETON, &payload).is_err());
    }

    #[test]
    fn oversized_edits_and_shared_raster_budgets_are_refused() {
        let oversized = json!({"note":"x".repeat(MAX_JSON)});
        assert!(validate_edit(&oversized).is_err());
        let raster = STANDARD.encode(vec![0; MAX_RASTERS / 2 + 1]);
        let data = json!({"masks":[{"maskDataBase64":raster},{"maskDataBase64":raster}]});
        assert!(validate_edit(&data).is_err());
        let invalid = json!({"masks":[{"maskDataBase64":"not base64!"}]});
        assert!(validate_edit(&invalid).is_err());
    }

    #[test]
    fn decompression_bombs_and_concatenated_streams_are_refused() {
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::best());
        compressor.write_all(&vec![b' '; MAX_JSON + 1]).unwrap();
        let packed = STANDARD.encode(compressor.finish().unwrap());
        let xml = crate::xmp::replace_image_properties(
            crate::xmp::SKELETON,
            URI,
            "rr",
            OWNED,
            &[
                ("Format", "1".into()),
                ("Recipe", packed),
                ("RecipeSha256", "ignored".into()),
            ],
        )
        .unwrap();
        assert!(read(&xml).unwrap_err().contains("inflates"));
        let payload = Payload {
            adjustments: json!({"exposure":1}),
            projection: json!({"exposure":1}),
        };
        let valid = embed(crate::xmp::SKELETON, &payload).unwrap();
        let recipe = crate::xmp::image_property(&valid, URI, "Recipe")
            .unwrap()
            .unwrap();
        let mut bytes = STANDARD.decode(recipe).unwrap();
        bytes.extend_from_slice(b"trailing stream");
        let changed = crate::xmp::replace_image_properties(
            &valid,
            URI,
            "rr",
            &["Recipe"],
            &[("Recipe", STANDARD.encode(bytes))],
        )
        .unwrap();
        assert!(read(&changed).unwrap_err().contains("Trailing"));
    }

    #[test]
    fn mask_reconciliation_preserves_native_fields_and_matches_lightroom_renames_by_sync_identity()
    {
        let payload = Payload {
            adjustments: json!({"masks":[{"id":"native","name":"Sky","adjustments":{"exposure":0.222222,"halationAmount":40},"subMasks":[{"id":"geometry","type":"linear","parameters":{"startX":0.2,"nativeOnly":17}}]},{"id":"bitmap","name":"Paint","subMasks":[]}]}),
            projection: json!({"masks":[{"id":"lr-sync","name":"Sky","adjustments":{"exposure":0.22},"subMasks":[{"id":"generated-before","type":"linear","parameters":{"startX":0.2}}]}]}),
        };
        let current = json!({"masks":[{"id":"lr-sync","name":"Renamed sky","adjustments":{"exposure":0.5},"subMasks":[{"id":"generated-after","type":"linear","parameters":{"startX":0.4}}]},{"id":"new","name":"New Lightroom mask","subMasks":[]}]});
        let result = restore(&payload, &current, &json!({"masks":[]}));
        assert_eq!(result["masks"][0]["id"], "native");
        assert_eq!(result["masks"][0]["name"], "Renamed sky");
        assert_eq!(
            result["masks"][0]["adjustments"],
            json!({"exposure":0.5,"halationAmount":40})
        );
        assert_eq!(
            result["masks"][0]["subMasks"][0]["parameters"],
            json!({"startX":0.4,"nativeOnly":17})
        );
        assert_eq!(result["masks"][1], payload.adjustments["masks"][1]);
        assert_eq!(result["masks"][2], current["masks"][1]);
        let removed = restore(&payload, &json!({"masks":[]}), &json!({"masks":[]}));
        assert_eq!(removed["masks"], json!([payload.adjustments["masks"][1]]));
    }
    #[test]
    fn returned_mask_order_wins_without_displacing_native_only_masks() {
        let payload = Payload {
            adjustments: json!({"masks":[{"id":"native-a","name":"A","extra":1},{"id":"paint","name":"Paint"},{"id":"native-b","name":"B","extra":2}]}),
            projection: json!({"masks":[{"id":"sync-a","name":"A"},{"id":"sync-b","name":"B"}]}),
        };
        let current = json!({"masks":[{"id":"sync-b","name":"B"},{"id":"sync-a","name":"A"}]});
        let restored = restore(&payload, &current, &json!({"masks":[]}));
        assert_eq!(
            restored["masks"],
            json!([{"id":"native-b","name":"B","extra":2},{"id":"paint","name":"Paint"},{"id":"native-a","name":"A","extra":1}])
        );
    }
}
