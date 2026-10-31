use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

pub(crate) const URI: &str = "urn:rapidroom:edit:1.0";
const OWNED: &[&str] = &["Format", "Recipe", "RecipeSha256"];
const MAX_JSON: usize = 12 * 1024 * 1024;
const MAX_RASTERS: usize = 6 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum ProjectedValue {
    Scalar(String),
    Sequence(Vec<String>),
}

pub(crate) type Projection = BTreeMap<String, ProjectedValue>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {
    pub adjustments: Value,
    pub source_sha256: String,
    pub projection: Projection,
}

fn check(value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("The RapidRoom edit must be an object".into());
    }
    fn count(data: &str, bytes: &mut usize) -> Result<(), String> {
        let encoded = data.split_once(',').map_or(data, |(_, body)| body);
        if encoded.len() > MAX_RASTERS * 4 / 3 {
            return Err("Embedded rasters exceed the 6 MiB round-trip limit".into());
        }
        let raster = STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid embedded raster data")?;
        *bytes = bytes
            .checked_add(raster.len())
            .ok_or("Embedded rasters are too large")?;
        if *bytes > MAX_RASTERS {
            return Err("Embedded rasters exceed the 6 MiB round-trip limit".into());
        }
        Ok(())
    }
    fn visit(value: &Value, bytes: &mut usize) -> Result<(), String> {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == "maskDataBase64"
                        && let Some(data) = value.as_str()
                    {
                        count(data, bytes)?;
                    }
                    if key == "patchData"
                        && let Some(data) = value.as_object()
                    {
                        for raster in ["color", "mask"]
                            .into_iter()
                            .filter_map(|key| data.get(key))
                        {
                            if let Some(data) = raster.as_str() {
                                count(data, bytes)?;
                            }
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

pub(crate) fn source_sha256(image: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(image)
        .map_err(|error| format!("Cannot read the source image for XMP round trip: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

pub(crate) fn embed(content: &str, payload: &Payload) -> Result<String, String> {
    check(&payload.adjustments)?;
    if payload.source_sha256.len() != 64
        || !payload
            .source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Invalid source image digest".into());
    }
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
    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_preserves_exact_json_and_rejects_corruption() {
        let payload = Payload {
            adjustments: json!({"exposure":0.1234567,"halationAmount":42,"masks":[{"name":"Sky & land","subMasks":[]}]}),
            source_sha256: "ab".repeat(32),
            projection: Projection::new(),
        };
        let xml = embed(crate::xmp::SKELETON, &payload).unwrap();
        let read_back = read(&xml).unwrap().unwrap();
        assert_eq!(read_back.adjustments, payload.adjustments);
        assert_eq!(read_back.source_sha256, payload.source_sha256);
        assert!(read(&xml.replace("rr:Format=\"1\"", "rr:Format=\"2\"")).is_err());
        let broken = xml.replace("rr:RecipeSha256=\"", "rr:RecipeSha256=\"0");
        assert!(read(&broken).is_err());
    }

    #[test]
    fn absent_payload_and_invalid_shapes_are_handled() {
        assert!(read(crate::xmp::SKELETON).unwrap().is_none());
        let payload = Payload {
            adjustments: json!([]),
            source_sha256: "ab".repeat(32),
            projection: Projection::new(),
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
        let patches = json!({"aiPatches":[{"patchData":{"color":raster,"mask":raster}}]});
        assert!(validate_edit(&patches).is_err());
        assert!(
            validate_edit(&json!({"aiPatches":[{"patchData":{"color":"YWJj","mask":"YWJj"}}]}))
                .is_ok()
        );
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
            source_sha256: "ab".repeat(32),
            projection: Projection::new(),
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
}
