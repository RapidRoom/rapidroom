//! Catalog develop settings are adapted from Laurensius Adi's importer
//! (laurensiusadi/RapidRAW@dda6cc51c69dc6a17906609dbb667eff3507aeeb).
//! Catalog access/path resolution and adjustment mapping remain shared with
//! the existing collections importer and image-aware XMP mapper.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::catalog::{Catalog, text};
use crate::file_management::{is_card_read_only_path, parse_virtual_path, write_file_atomically};
use crate::image_processing::ImageMetadata;
use crate::lrtemplate::{catalog_as_shot_white_balance, catalog_develop_to_xmp};
use crate::preset_converter::{
    convert_xmp_sidecar_to_preset_for_image, extract_namespaced_scalar,
    lightroom_settings_not_transferred,
};

const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ROWS: usize = 50_000;
static IMPORT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DevelopValueSource {
    #[serde(rename = "catalogDevelopSettings")]
    DevelopSettings,
    #[serde(rename = "catalogDevelopSettingsAndAsShotHistory")]
    DevelopSettingsAndAsShotHistory,
    #[serde(rename = "catalogOrientationAndPhotoExif")]
    OrientationAndPhotoExif,
    #[serde(rename = "catalogOrientationAndCatalogMetadata")]
    OrientationAndCatalogMetadata,
    #[serde(rename = "catalogRating")]
    Rating,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopPhotoPreview {
    pub id: i64,
    pub path: String,
    pub copy_name: String,
    pub virtual_copy: bool,
    pub found: bool,
    pub existing_sidecar: bool,
    pub existing_edits: bool,
    pub rating: Option<u8>,
    pub adjustments: Value,
    pub sources: BTreeMap<String, DevelopValueSource>,
    pub matching_xmp_sidecar: bool,
    pub unsupported: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopPreview {
    pub catalog_name: String,
    pub fingerprint: String,
    pub photos: Vec<DevelopPhotoPreview>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopImportResult {
    pub imported: usize,
    pub preserved: usize,
    pub virtual_copies: usize,
    pub errors: Vec<String>,
}

struct MappedDevelop {
    adjustments: Value,
    unsupported: Vec<String>,
    sources: BTreeMap<String, DevelopValueSource>,
}

impl MappedDevelop {
    fn empty() -> Self {
        Self {
            adjustments: json!({}),
            unsupported: Vec::new(),
            sources: BTreeMap::new(),
        }
    }
}

struct DevelopRow {
    id: i64,
    master: Option<i64>,
    rating: Option<u8>,
    orientation: String,
    copy_name: String,
    settings: Result<String, String>,
    width: Option<f64>,
    height: Option<f64>,
    xmp: String,
}

struct PreparedPhoto {
    preview: DevelopPhotoPreview,
    sidecar: PathBuf,
    sidecar_bytes: Option<Vec<u8>>,
    document: Value,
    inherit_from: Option<(PathBuf, Vec<u8>)>,
    raw_identity: Option<(u64, u128)>,
}

struct PreparedImport {
    name: String,
    fingerprint: String,
    photos: Vec<PreparedPhoto>,
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err("Develop settings exceed the supported row size.".into());
    }
    if let Ok(text) = std::str::from_utf8(bytes)
        && text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with(['s', '<'])
    {
        return Ok(text.into());
    }
    if bytes.len() <= 4 {
        return Err("Develop settings are empty or unreadable.".into());
    }
    let decoder = flate2::read::ZlibDecoder::new(&bytes[4..]);
    let mut decoded = Vec::new();
    decoder
        .take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(|_| "Compressed develop settings could not be read.".to_string())?;
    if decoded.len() > MAX_TEXT_BYTES {
        return Err("Expanded develop settings exceed the supported row size.".into());
    }
    String::from_utf8(decoded).map_err(|_| "Develop settings are not valid UTF-8.".into())
}

fn column(catalog: &Catalog, table: &str, alias: &str, name: &str) -> String {
    if catalog.has_column(table, name) {
        format!("{alias}.{name}")
    } else {
        "NULL".into()
    }
}

fn read_rows(catalog: &Catalog) -> Result<Vec<DevelopRow>, String> {
    catalog.require_tables(&["Adobe_imageDevelopSettings"])?;
    for (table, columns) in [
        ("Adobe_images", &["id_local"][..]),
        ("Adobe_imageDevelopSettings", &["image", "text"][..]),
    ] {
        for name in columns {
            if !catalog.has_column(table, name) {
                return Err(format!("This catalog has no {table}.{name} column."));
            }
        }
    }
    let additional = catalog.has_column("Adobe_AdditionalMetadata", "image")
        && catalog.has_column("Adobe_AdditionalMetadata", "xmp");
    let query = format!(
        "SELECT i.id_local, {}, {}, {}, {}, d.text, {}, {}, {} FROM Adobe_images i
        JOIN Adobe_imageDevelopSettings d ON d.image = i.id_local {} ORDER BY i.id_local",
        column(catalog, "Adobe_images", "i", "masterImage"),
        column(catalog, "Adobe_images", "i", "rating"),
        column(catalog, "Adobe_images", "i", "orientation"),
        column(catalog, "Adobe_images", "i", "copyName"),
        column(catalog, "Adobe_imageDevelopSettings", "d", "fileWidth"),
        column(catalog, "Adobe_imageDevelopSettings", "d", "fileHeight"),
        if additional { "m.xmp" } else { "NULL" },
        if additional {
            "LEFT JOIN Adobe_AdditionalMetadata m ON m.image = i.id_local"
        } else {
            ""
        },
    );
    let mut statement = catalog
        .connection()
        .prepare(&query)
        .map_err(|e| e.to_string())?;
    let mut rows = statement.query([]).map_err(|e| e.to_string())?;
    let mut output = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        if output.len() >= MAX_ROWS {
            return Err(
                "This catalog has more than 50,000 develop rows; import a smaller catalog.".into(),
            );
        }
        let bytes = match row.get_ref(5).map_err(|e| e.to_string())? {
            ValueRef::Text(bytes) | ValueRef::Blob(bytes) => bytes,
            _ => &[],
        };
        let rating = text(row, 2)
            .map_err(|e| e.to_string())?
            .parse::<f64>()
            .ok()
            .filter(|value| {
                value.is_finite() && value.fract() == 0.0 && (0.0..=5.0).contains(value)
            })
            .map(|value| value as u8);
        let positive_number = |index| -> Option<f64> {
            text(row, index)
                .ok()?
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 1.0)
        };
        output.push(DevelopRow {
            id: row.get(0).map_err(|e| e.to_string())?,
            master: text(row, 1)
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| *v > 0),
            rating,
            orientation: text(row, 3).map_err(|e| e.to_string())?,
            copy_name: text(row, 4).map_err(|e| e.to_string())?,
            settings: decode_text(bytes),
            width: positive_number(6),
            height: positive_number(7),
            xmp: match row.get_ref(8).map_err(|e| e.to_string())? {
                ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                    decode_text(bytes).unwrap_or_default()
                }
                _ => String::new(),
            },
        });
    }
    Ok(output)
}

fn history_white_balance(catalog: &Catalog, id: i64) -> Option<(f64, f64)> {
    if !catalog.has_column("Adobe_libraryImageDevelopHistoryStep", "image")
        || !catalog.has_column("Adobe_libraryImageDevelopHistoryStep", "text")
    {
        return None;
    }
    let order = if catalog.has_column("Adobe_libraryImageDevelopHistoryStep", "dateCreated") {
        " ORDER BY dateCreated"
    } else {
        ""
    };
    let mut statement = catalog
        .connection()
        .prepare(&format!(
            "SELECT text FROM Adobe_libraryImageDevelopHistoryStep WHERE image = ?1{order}"
        ))
        .ok()?;
    let mut rows = statement.query([id]).ok()?;
    while let Some(row) = rows.next().ok()? {
        let bytes = match row.get_ref(0).ok()? {
            ValueRef::Text(bytes) | ValueRef::Blob(bytes) => bytes,
            _ => continue,
        };
        if let Ok(text) = decode_text(bytes)
            && let Ok(Some(white_balance)) = catalog_as_shot_white_balance(&text)
        {
            return Some(white_balance);
        }
    }
    None
}

fn rotation_steps(orientation: u16) -> Option<u8> {
    match orientation {
        1 => Some(0),
        6 => Some(1),
        3 => Some(2),
        8 => Some(3),
        _ => None,
    }
}

fn source_orientation(row: &DevelopRow, path: &Path) -> Option<u16> {
    if let Some(value) = extract_namespaced_scalar(&row.xmp, "tiff", "Orientation")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|v| (1..=8).contains(v))
    {
        return Some(value);
    }
    let file = fs::File::open(path).ok()?;
    let metadata = exif::Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .ok()?;
    let value = metadata
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|v| v.value.get_uint(0))
        .unwrap_or(1);
    (1..=8).contains(&value).then_some(value as u16)
}

/// Repeated previews and the import validation pass must hash identical
/// adjustments. The shared XMP converter generates fresh mask UUIDs; catalog
/// masks instead use their catalog/photo identity and structural position.
fn stable_catalog_mask_ids(adjustments: &mut Value, catalog: &Catalog, id: i64) {
    fn assign(value: &mut Value, seed: &[u8], position: &str) {
        match value {
            Value::Object(object) => {
                if object.contains_key("id") {
                    let mut hash = blake3::Hasher::new();
                    hash.update(seed);
                    hash.update(position.as_bytes());
                    let digest = hash.finalize();
                    object.insert(
                        "id".into(),
                        json!(
                            Uuid::from_bytes(
                                digest.as_bytes()[..16]
                                    .try_into()
                                    .expect("hash is 32 bytes")
                            )
                            .to_string()
                        ),
                    );
                }
                for (key, child) in object.iter_mut() {
                    assign(child, seed, &format!("{position}/{key}"));
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter_mut().enumerate() {
                    assign(child, seed, &format!("{position}/{index}"));
                }
            }
            _ => {}
        }
    }
    let identity = catalog
        .path()
        .canonicalize()
        .unwrap_or_else(|_| catalog.path().to_path_buf());
    let seed =
        blake3::hash(format!("lightroom-masks:{}:{id}", identity.to_string_lossy()).as_bytes());
    if let Some(masks) = adjustments.get_mut("masks") {
        assign(masks, seed.as_bytes(), "masks");
    }
}

fn map_adjustments(
    catalog: &Catalog,
    row: &DevelopRow,
    path: &Path,
) -> Result<MappedDevelop, String> {
    let settings = row.settings.as_ref().map_err(Clone::clone)?;
    let as_shot = history_white_balance(catalog, row.id)
        .or_else(|| row.master.and_then(|id| history_white_balance(catalog, id)));
    let converted = catalog_develop_to_xmp(settings, as_shot)?;
    let mut xmp = converted.xmp;
    let target_orientation = match row.orientation.as_str() {
        "AB" => Some(1),
        "BC" => Some(6),
        "CD" => Some(3),
        "DA" => Some(8),
        _ => None,
    };
    let source_steps = source_orientation(row, path).and_then(rotation_steps);
    let target_steps = target_orientation.and_then(rotation_steps);
    let geometry_supported = source_steps.is_some() && target_steps.is_some();
    let dimensions = match (row.width, row.height) {
        (Some(width), Some(height)) => {
            format!(" tiff:ImageWidth=\"{width}\" tiff:ImageLength=\"{height}\"")
        }
        _ => String::new(),
    };
    if let Some(orientation) = target_orientation {
        xmp = xmp.replacen("<rdf:Description ", &format!(
            "<rdf:Description xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\" tiff:Orientation=\"{orientation}\"{dimensions} "
        ), 1);
    }
    let preset = convert_xmp_sidecar_to_preset_for_image(&xmp, path)?;
    let mut unsupported = converted.unsupported;
    unsupported.extend(
        lightroom_settings_not_transferred(&xmp, &preset)
            .into_iter()
            .map(str::to_owned),
    );
    let mut adjustments = preset.adjustments;
    if let (Some(source), Some(target)) = (source_steps, target_steps) {
        let steps = (target + 4 - source) % 4;
        if steps != 0 {
            adjustments["orientationSteps"] = json!(steps);
        }
    }
    if !geometry_supported {
        if let Some(map) = adjustments.as_object_mut() {
            for key in [
                "crop",
                "rotation",
                "aspectRatio",
                "orientationSteps",
                "masks",
            ] {
                if key == "masks" && map.contains_key(key) {
                    unsupported.push("orientationAndMasks".into());
                }
                map.remove(key);
            }
        }
        unsupported.push("orientationAndCrop".into());
    }
    if !row.copy_name.is_empty() {
        unsupported.push("virtualCopyName".into());
    }
    unsupported.sort();
    unsupported.dedup();
    stable_catalog_mask_ids(&mut adjustments, catalog, row.id);
    let mut sources = BTreeMap::new();
    for key in adjustments
        .as_object()
        .into_iter()
        .flat_map(|map| map.keys())
    {
        let source = if matches!(key.as_str(), "temperature" | "tint") && as_shot.is_some() {
            DevelopValueSource::DevelopSettingsAndAsShotHistory
        } else if key == "orientationSteps" {
            if extract_namespaced_scalar(&row.xmp, "tiff", "Orientation")
                .and_then(|v| v.parse::<u16>().ok())
                .is_some_and(|v| (1..=8).contains(&v))
            {
                DevelopValueSource::OrientationAndCatalogMetadata
            } else {
                DevelopValueSource::OrientationAndPhotoExif
            }
        } else {
            DevelopValueSource::DevelopSettings
        };
        sources.insert(key.clone(), source);
    }
    Ok(MappedDevelop {
        adjustments,
        unsupported,
        sources,
    })
}

fn virtual_path(catalog: &Catalog, row: &DevelopRow, path: &Path) -> String {
    let path_text = path.to_string_lossy();
    if row.master.is_none() {
        return path_text.into_owned();
    }
    let identity = catalog
        .path()
        .canonicalize()
        .unwrap_or_else(|_| catalog.path().to_path_buf());
    let hash =
        blake3::hash(format!("lightroom:{}:{}", identity.to_string_lossy(), row.id).as_bytes());
    let id = Uuid::from_bytes(hash.as_bytes()[..16].try_into().expect("hash is 32 bytes"));
    format!("{path_text}?vc={id}")
}

fn sidecar_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read existing edits: {error}")),
    }
}

fn file_identity(path: &Path) -> Option<(u64, u128)> {
    let metadata = fs::metadata(path).ok()?;
    Some((
        metadata.len(),
        metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos(),
    ))
}

fn hash_field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn hash_file(hash: &mut blake3::Hasher, path: &Path) -> Result<(), String> {
    hash_field(hash, path.to_string_lossy().as_bytes());
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            hash_field(hash, b"missing");
            return Ok(());
        }
        Err(error) => return Err(format!("Could not inspect catalog: {error}")),
    };
    let mut contents = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        contents.update(&buffer[..count]);
    }
    hash_field(hash, contents.finalize().as_bytes());
    Ok(())
}

fn prepare(path: &Path, mappings: &HashMap<String, String>) -> Result<PreparedImport, String> {
    let catalog = Catalog::open(path)?;
    let roots = catalog.resolve_roots(mappings)?;
    let locations = catalog.image_locations()?;
    let rows = read_rows(&catalog)?;
    let mut hash = blake3::Hasher::new();
    let ordered: BTreeMap<_, _> = mappings.iter().collect();
    hash_field(
        &mut hash,
        &serde_json::to_vec(&ordered).map_err(|e| e.to_string())?,
    );
    for suffix in ["", "-wal", "-journal"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        hash_file(&mut hash, Path::new(&file))?;
    }
    let mut photos = Vec::new();
    for row in rows {
        let source = locations
            .get(&row.id)
            .or_else(|| row.master.and_then(|id| locations.get(&id)))
            .and_then(|location| roots.file_path(location));
        let path = source.as_deref().unwrap_or_else(|| Path::new(""));
        let virtual_path = virtual_path(&catalog, &row, path);
        let (_, sidecar) = parse_virtual_path(&virtual_path);
        let identity = file_identity(path);
        let found = path.is_file() && identity.is_some();
        let mut error = (!found).then(|| {
            "The original photo could not be found. Relink its root folder first.".to_string()
        });
        let bytes = match sidecar_bytes(&sidecar) {
            Ok(bytes) => bytes,
            Err(message) => {
                error = Some(message);
                None
            }
        };
        let mut inherited = None;
        let mut document =
            serde_json::to_value(ImageMetadata::default()).map_err(|e| e.to_string())?;
        let mut metadata = ImageMetadata::default();
        let existing = bytes.is_some();
        let document_bytes = if existing {
            bytes.as_deref()
        } else if row.master.is_some() && found {
            let (_, physical_sidecar) = parse_virtual_path(&path.to_string_lossy());
            match sidecar_bytes(&physical_sidecar) {
                Ok(Some(bytes)) => {
                    inherited = Some((physical_sidecar, bytes));
                    inherited.as_ref().map(|(_, bytes)| bytes.as_slice())
                }
                Ok(None) => None,
                Err(message) => {
                    error = Some(message);
                    None
                }
            }
        } else {
            None
        };
        if let Some(bytes) = document_bytes {
            match serde_json::from_slice::<Value>(bytes).and_then(|value| {
                serde_json::from_value::<ImageMetadata>(value.clone())
                    .map(|metadata| (value, metadata))
            }) {
                Ok((value, saved)) => {
                    document = value;
                    metadata = saved;
                }
                Err(_) => {
                    error = Some(
                        "Existing edits are unreadable; repair or back them up before importing."
                            .into(),
                    )
                }
            }
        }
        let MappedDevelop {
            adjustments,
            unsupported,
            mut sources,
        } = if found {
            match map_adjustments(&catalog, &row, path) {
                Ok(mapped) => mapped,
                Err(message) => {
                    error = Some(message);
                    MappedDevelop::empty()
                }
            }
        } else {
            MappedDevelop::empty()
        };
        if row.rating.is_some() {
            sources.insert("rating".into(), DevelopValueSource::Rating);
        }
        hash_field(&mut hash, &row.id.to_le_bytes());
        hash_field(&mut hash, virtual_path.as_bytes());
        hash_field(
            &mut hash,
            &serde_json::to_vec(&identity).map_err(|e| e.to_string())?,
        );
        hash_field(&mut hash, bytes.as_deref().unwrap_or(b"missing"));
        if let Some((path, bytes)) = &inherited {
            hash_field(&mut hash, path.to_string_lossy().as_bytes());
            hash_field(&mut hash, bytes);
        }
        hash_field(
            &mut hash,
            &serde_json::to_vec(&adjustments).map_err(|e| e.to_string())?,
        );
        photos.push(PreparedPhoto {
            preview: DevelopPhotoPreview {
                id: row.id,
                path: virtual_path,
                copy_name: row.copy_name,
                virtual_copy: row.master.is_some(),
                found,
                existing_sidecar: existing,
                existing_edits: existing
                    && metadata
                        .adjustments
                        .as_object()
                        .is_some_and(|map| !map.is_empty()),
                rating: row.rating,
                adjustments,
                sources,
                matching_xmp_sidecar: ["xmp", "XMP"].iter().any(|extension| {
                    path.with_extension(extension).is_file()
                        || PathBuf::from(format!("{}.{}", path.display(), extension)).is_file()
                }),
                unsupported,
                error,
            },
            sidecar,
            sidecar_bytes: bytes,
            document,
            inherit_from: inherited,
            raw_identity: identity,
        });
    }
    let mut destinations: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (index, photo) in photos.iter().enumerate() {
        if photo.preview.found {
            destinations
                .entry(photo.sidecar.clone())
                .or_default()
                .push(index);
        }
    }
    for indices in destinations.values().filter(|indices| indices.len() > 1) {
        for index in indices {
            photos[*index].preview.error = Some(
                "Several catalog rows map to the same edit file. Check the root mappings.".into(),
            );
        }
    }
    Ok(PreparedImport {
        name: catalog.name(),
        fingerprint: hash.finalize().to_hex().to_string(),
        photos,
    })
}

impl PreparedImport {
    fn into_preview(self) -> DevelopPreview {
        DevelopPreview {
            catalog_name: self.name,
            fingerprint: self.fingerprint,
            photos: self.photos.into_iter().map(|photo| photo.preview).collect(),
        }
    }
}

fn apply_import(
    path: &Path,
    mappings: &HashMap<String, String>,
    fingerprint: &str,
    selections: &[i64],
    replace_edits: bool,
    import_ratings: bool,
    replace_ratings: bool,
) -> Result<DevelopImportResult, String> {
    let _guard = IMPORT_LOCK
        .lock()
        .map_err(|_| "Catalog import lock failed.".to_string())?;
    let prepared = prepare(path, mappings)?;
    if prepared.fingerprint != fingerprint {
        return Err("The catalog, photos, or saved edits changed after preview. Refresh the preview before importing.".into());
    }
    let selected: HashSet<_> = selections.iter().copied().collect();
    if selected.len() != selections.len() {
        return Err("The selection contains duplicate photo IDs.".into());
    }
    let known: HashSet<_> = prepared.photos.iter().map(|p| p.preview.id).collect();
    if !selected.is_subset(&known) {
        return Err("The selection contains photos outside this preview.".into());
    }
    let photos: Vec<_> = prepared
        .photos
        .into_iter()
        .filter(|p| selected.contains(&p.preview.id))
        .collect();
    for photo in &photos {
        if let Some(error) = &photo.preview.error {
            return Err(format!("{}: {error}", photo.preview.path));
        }
        let (source, _) = parse_virtual_path(&photo.preview.path);
        if is_card_read_only_path(&source) || is_card_read_only_path(&photo.sidecar) {
            return Err(
                "Card mode is read-only. Copy the photos to a local folder before importing edits."
                    .into(),
            );
        }
    }
    let mut result = DevelopImportResult::default();
    for mut photo in photos {
        let (source, _) = parse_virtual_path(&photo.preview.path);
        let unchanged = sidecar_bytes(&photo.sidecar)
            .is_ok_and(|bytes| bytes == photo.sidecar_bytes)
            && file_identity(&source) == photo.raw_identity
            && photo.inherit_from.as_ref().is_none_or(|(path, bytes)| {
                sidecar_bytes(path)
                    .is_ok_and(|current| current.as_deref() == Some(bytes.as_slice()))
            });
        if !unchanged {
            result.errors.push(format!(
                "{}: saved edits or the original changed; refresh the preview.",
                photo.preview.path
            ));
            continue;
        }
        let mut changed = false;
        if !photo.preview.existing_sidecar || replace_edits {
            photo.document["adjustments"] = photo.preview.adjustments;
            changed = true;
        }
        if import_ratings
            && (!photo.preview.existing_sidecar || replace_ratings)
            && let Some(rating) = photo.preview.rating
        {
            photo.document["rating"] = json!(rating);
            photo.document["rating_is_explicit"] = json!(true);
            changed = true;
        }
        if !changed {
            result.preserved += 1;
            continue;
        }
        let bytes = serde_json::to_vec_pretty(&photo.document).map_err(|e| e.to_string())?;
        match write_file_atomically(&photo.sidecar, bytes) {
            Ok(()) => {
                result.imported += 1;
                if photo.preview.virtual_copy && !photo.preview.existing_sidecar {
                    result.virtual_copies += 1;
                }
            }
            Err(error) => result
                .errors
                .push(format!("{}: {error}", photo.preview.path)),
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn preview_lightroom_develop(
    path: String,
    mappings: HashMap<String, String>,
) -> Result<DevelopPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        prepare(Path::new(&path), &mappings).map(PreparedImport::into_preview)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn import_lightroom_develop(
    path: String,
    mappings: HashMap<String, String>,
    fingerprint: String,
    selections: Vec<i64>,
    replace_edits: bool,
    import_ratings: bool,
    replace_ratings: bool,
) -> Result<DevelopImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        apply_import(
            Path::new(&path),
            &mappings,
            &fingerprint,
            &selections,
            replace_edits,
            import_ratings,
            replace_ratings,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::card_mode_test_support::CardMode;
    use crate::lightroom::test_catalog::{TestCatalog, folder_state, touch};
    use rusqlite::params;
    use std::io::Write;

    struct Library {
        dir: tempfile::TempDir,
        catalog: TestCatalog,
        path: PathBuf,
        photos: PathBuf,
        mappings: HashMap<String, String>,
    }

    impl Library {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.lrcat");
            let photos = dir.path().join("photos");
            fs::create_dir(&photos).unwrap();
            let catalog = TestCatalog::create(&path);
            catalog
                .connection
                .execute_batch(
                    "ALTER TABLE Adobe_images ADD COLUMN rating;
                ALTER TABLE Adobe_images ADD COLUMN orientation;
                CREATE TABLE Adobe_imageDevelopSettings (image, text, fileWidth, fileHeight);
                CREATE TABLE Adobe_AdditionalMetadata (image, xmp);
                CREATE TABLE Adobe_libraryImageDevelopHistoryStep (image, text, dateCreated);",
                )
                .unwrap();
            catalog.root(1, "/old/photos/", None);
            catalog.folder(1, 1, "");
            let mappings =
                HashMap::from([("/old/photos/".into(), photos.to_string_lossy().into_owned())]);
            Self {
                dir,
                catalog,
                path,
                photos,
                mappings,
            }
        }

        fn photo(&self, id: i64, name: &str, settings: &[u8]) -> PathBuf {
            self.catalog.image(id, 1, name, "ARW");
            let path = touch(&self.photos.join(format!("{name}.ARW")));
            self.develop(id, settings);
            path
        }

        fn develop(&self, id: i64, settings: &[u8]) {
            self.catalog
                .connection
                .execute(
                    "UPDATE Adobe_images SET orientation='AB', rating=3 WHERE id_local=?1",
                    [id],
                )
                .unwrap();
            self.catalog
                .connection
                .execute(
                    "INSERT INTO Adobe_imageDevelopSettings VALUES (?1, ?2, 8, 4)",
                    params![id, settings],
                )
                .unwrap();
            self.catalog
                .connection
                .execute(
                    "INSERT INTO Adobe_AdditionalMetadata VALUES (?1, ?2)",
                    params![id, "<rdf:Description tiff:Orientation=\"1\"/>"],
                )
                .unwrap();
        }

        fn prepare(&self) -> PreparedImport {
            prepare(&self.path, &self.mappings).unwrap()
        }

        fn apply(
            &self,
            fingerprint: &str,
            ids: &[i64],
            edits: bool,
            ratings: bool,
            replace_ratings: bool,
        ) -> Result<DevelopImportResult, String> {
            apply_import(
                &self.path,
                &self.mappings,
                fingerprint,
                ids,
                edits,
                ratings,
                replace_ratings,
            )
        }
    }

    const SETTINGS: &[u8] = b"s = { Exposure2012 = 0.5, FutureScalar = 7 }";

    const MASK_SETTINGS: &[u8] = br#"s = { Exposure2012 = 0.5,
      MaskGroupBasedCorrections = {
        { CorrectionName = 'Catalog sky', LocalExposure2012 = -0.125,
          CorrectionMasks = {
            { What = 'Mask/Gradient', MaskName = 'Sky',
              ZeroX = 0.5, ZeroY = 0.6, FullX = 0.5, FullY = 0.2 }
          }
        }
      } }"#;

    #[test]
    fn catalog_develop_and_masks_take_precedence_over_a_matching_xmp() {
        let _mode = CardMode::off();
        let library = Library::new();
        let raw = library.photo(1, "masked", MASK_SETTINGS);
        fs::write(
            raw.with_extension("xmp"),
            "<rdf:Description crs:Exposure2012='9'/>",
        )
        .unwrap();
        let before = folder_state(library.dir.path());
        let preview = library.prepare();
        let photo = &preview.photos[0].preview;
        let repeated = library.prepare();
        assert_eq!(repeated.fingerprint, preview.fingerprint);
        assert_eq!(repeated.photos[0].preview.adjustments, photo.adjustments);
        assert_eq!(photo.adjustments["exposure"], 0.5);
        assert_eq!(photo.adjustments["masks"][0]["name"], "Catalog sky");
        assert_eq!(
            photo.adjustments["masks"][0]["subMasks"][0]["type"],
            "linear"
        );
        assert_eq!(
            photo.sources["exposure"],
            DevelopValueSource::DevelopSettings
        );
        assert_eq!(photo.sources["masks"], DevelopValueSource::DevelopSettings);
        assert_eq!(photo.sources["rating"], DevelopValueSource::Rating);
        assert!(photo.matching_xmp_sidecar);
        assert_eq!(folder_state(library.dir.path()), before);
        library
            .apply(&preview.fingerprint, &[1], false, true, false)
            .unwrap();
        let (_, sidecar) = parse_virtual_path(&raw.to_string_lossy());
        let metadata: ImageMetadata = serde_json::from_slice(&fs::read(sidecar).unwrap()).unwrap();
        assert_eq!(metadata.adjustments, photo.adjustments);
        for path in [raw.clone(), raw.with_extension("xmp"), library.path.clone()] {
            assert_eq!(fs::read(&path).unwrap(), before[&path].0);
        }
    }

    #[test]
    fn catalog_masks_are_reported_when_photo_orientation_is_unknown() {
        let _mode = CardMode::off();
        let library = Library::new();
        library.photo(1, "masked", MASK_SETTINGS);
        library
            .catalog
            .connection
            .execute("DELETE FROM Adobe_AdditionalMetadata", [])
            .unwrap();
        let preview = library.prepare();
        let photo = &preview.photos[0].preview;
        assert_eq!(photo.adjustments["exposure"], 0.5);
        assert!(photo.adjustments.get("masks").is_none());
        assert!(!photo.sources.contains_key("masks"));
        assert!(photo.unsupported.contains(&"orientationAndMasks".into()));
    }

    #[test]
    fn preview_is_read_only_and_apply_requires_selected_photos() {
        let _mode = CardMode::off();
        let library = Library::new();
        let first = library.photo(1, "first", SETTINGS);
        let second = library.photo(2, "second", SETTINGS);
        let before = folder_state(library.dir.path());
        let preview = library.prepare();
        assert_eq!(folder_state(library.dir.path()), before);
        assert_eq!(preview.photos.len(), 2);
        assert_eq!(
            preview.photos[0].preview.adjustments["exposure"],
            json!(0.5)
        );
        assert!(
            preview.photos[0]
                .preview
                .unsupported
                .contains(&"FutureScalar".into())
        );
        let result = library
            .apply(&preview.fingerprint, &[1], false, true, false)
            .unwrap();
        assert_eq!(result.imported, 1);
        let (_, saved) = parse_virtual_path(&first.to_string_lossy());
        let metadata: ImageMetadata = serde_json::from_slice(&fs::read(saved).unwrap()).unwrap();
        assert_eq!(metadata.adjustments["exposure"], json!(0.5));
        assert_eq!(metadata.rating, 3);
        assert!(metadata.rating_is_explicit);
        assert!(!parse_virtual_path(&second.to_string_lossy()).1.exists());
        for path in [first, second, library.path.clone()] {
            assert_eq!(fs::read(&path).unwrap(), before[&path].0);
        }
    }

    #[test]
    fn existing_edits_and_ratings_need_independent_replacement_choices() {
        let _mode = CardMode::off();
        let library = Library::new();
        let raw = library.photo(1, "existing", SETTINGS);
        let (_, sidecar) = parse_virtual_path(&raw.to_string_lossy());
        let original = json!({"version":1, "rating":5, "rating_is_explicit":true,
            "adjustments":{"exposure":2}, "tags":["keep"], "flag":"pick",
            "flag_is_explicit":true, "futureMetadata":{"keep":true}});
        fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let preview = library.prepare();
        assert!(preview.photos[0].preview.existing_edits);
        let result = library
            .apply(&preview.fingerprint, &[1], false, true, false)
            .unwrap();
        assert_eq!(result.preserved, 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&sidecar).unwrap()).unwrap(),
            original
        );
        let preview = library.prepare();
        library
            .apply(&preview.fingerprint, &[1], true, true, false)
            .unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(saved["adjustments"]["exposure"], json!(0.5));
        assert_eq!(saved["rating"], json!(5));
        for key in ["tags", "flag", "flag_is_explicit", "futureMetadata"] {
            assert_eq!(saved[key], original[key]);
        }
        let preview = library.prepare();
        library
            .apply(&preview.fingerprint, &[1], false, true, true)
            .unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(saved["rating"], json!(3));
        assert_eq!(saved["adjustments"]["exposure"], json!(0.5));
    }

    #[test]
    fn stale_previews_cannot_replace_newer_edits_or_catalog_rows() {
        let _mode = CardMode::off();
        let library = Library::new();
        let raw = library.photo(1, "changing", SETTINGS);
        let preview = library.prepare();
        let (_, sidecar) = parse_virtual_path(&raw.to_string_lossy());
        let saved =
            serde_json::to_vec(&json!({"version":1,"rating":1,"adjustments":{"exposure":4}}))
                .unwrap();
        fs::write(&sidecar, &saved).unwrap();
        assert!(
            library
                .apply(&preview.fingerprint, &[1], true, true, true)
                .unwrap_err()
                .contains("changed after preview")
        );
        assert_eq!(fs::read(&sidecar).unwrap(), saved);
        let preview = library.prepare();
        library.catalog.connection.execute("UPDATE Adobe_imageDevelopSettings SET text='s = { Exposure2012 = 1 }' WHERE image=1", []).unwrap();
        assert!(
            library
                .apply(&preview.fingerprint, &[1], true, true, true)
                .is_err()
        );
        assert_eq!(fs::read(&sidecar).unwrap(), saved);
    }

    #[test]
    fn virtual_copy_ids_are_stable_and_keep_master_edits_untouched() {
        let _mode = CardMode::off();
        let library = Library::new();
        let master = library.photo(1, "master", SETTINGS);
        library.catalog.virtual_copy(16_777_217, 1);
        library.develop(16_777_217, SETTINGS);
        library.catalog.virtual_copy(33_554_433, 1);
        library.develop(33_554_433, SETTINGS);
        let before = folder_state(library.dir.path());
        let preview = library.prepare();
        assert_ne!(
            preview.photos[1].preview.path,
            preview.photos[2].preview.path
        );
        assert_eq!(
            preview.photos[1].preview.path,
            library.prepare().photos[1].preview.path
        );
        let result = library
            .apply(
                &preview.fingerprint,
                &[16_777_217, 33_554_433],
                false,
                true,
                false,
            )
            .unwrap();
        assert_eq!(result.virtual_copies, 2);
        assert!(!parse_virtual_path(&master.to_string_lossy()).1.exists());
        assert_eq!(fs::read(&master).unwrap(), before[&master].0);
        let preview = library.prepare();
        let result = library
            .apply(
                &preview.fingerprint,
                &[16_777_217, 33_554_433],
                false,
                true,
                false,
            )
            .unwrap();
        assert_eq!(result.preserved, 2);
        assert_eq!(result.virtual_copies, 0);
    }

    #[test]
    fn compressed_rows_are_bounded_and_malformed_rows_block_a_selected_batch() {
        let _mode = CardMode::off();
        let library = Library::new();
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(SETTINGS).unwrap();
        let mut compressed = (SETTINGS.len() as u32).to_le_bytes().to_vec();
        compressed.extend(encoder.finish().unwrap());
        library.photo(1, "compressed", &compressed);
        library.photo(2, "malformed", b"s = { Exposure2012 = os.execute('bad') }");
        let before = folder_state(library.dir.path());
        let preview = library.prepare();
        assert_eq!(
            preview.photos[0].preview.adjustments["exposure"],
            json!(0.5)
        );
        assert!(preview.photos[1].preview.error.is_some());
        assert!(
            library
                .apply(&preview.fingerprint, &[1, 2], false, true, false)
                .is_err()
        );
        assert_eq!(folder_state(library.dir.path()), before);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&vec![b' '; MAX_TEXT_BYTES + 1]).unwrap();
        let mut too_big = vec![0; 4];
        too_big.extend(encoder.finish().unwrap());
        assert!(decode_text(&too_big).unwrap_err().contains("Expanded"));
    }

    #[test]
    fn card_mode_is_checked_again_when_applying_a_preview() {
        let library = Library::new();
        library.photo(1, "card", SETTINGS);
        let preview = library.prepare();
        let before = folder_state(library.dir.path());
        let _mode = CardMode::on(&library.photos);
        assert!(
            library
                .apply(&preview.fingerprint, &[1], false, true, false)
                .unwrap_err()
                .contains("Card mode")
        );
        assert_eq!(folder_state(library.dir.path()), before);
    }

    #[test]
    fn portrait_crops_and_history_white_balance_use_the_shared_mapper() {
        let _mode = CardMode::off();
        let library = Library::new();
        library.photo(1, "portrait", b"s = { CropLeft=0.125, CropRight=0.25, CropTop=0, CropBottom=0.25, WhiteBalance='Custom', Temperature=5550, Tint=12 }");
        library
            .catalog
            .connection
            .execute(
                "UPDATE Adobe_images SET orientation='BC' WHERE id_local=1",
                [],
            )
            .unwrap();
        library
            .catalog
            .connection
            .execute(
                "INSERT INTO Adobe_libraryImageDevelopHistoryStep VALUES (1, ?1, 0)",
                ["s = { WhiteBalance='As Shot', Temperature=4440, Tint=-5 }"],
            )
            .unwrap();
        let preview = library.prepare();
        let adjustments = &preview.photos[0].preview.adjustments;
        assert_eq!(adjustments["orientationSteps"], json!(1));
        assert_eq!(
            adjustments["crop"],
            json!({"x":3.0,"y":1.0,"width":1.0,"height":1.0})
        );
        let expected = (1_000_000.0 / 4440.0 - 1_000_000.0 / 5550.0) / 150.0 * 100.0;
        assert!((adjustments["temperature"].as_f64().unwrap() - expected).abs() < 1e-6);
        assert!(
            !preview.photos[0]
                .preview
                .unsupported
                .contains(&"whiteBalance".into())
        );
    }

    #[test]
    fn unreadable_existing_edits_and_missing_photos_are_never_overwritten() {
        let _mode = CardMode::off();
        let library = Library::new();
        let raw = library.photo(1, "broken-sidecar", SETTINGS);
        let (_, sidecar) = parse_virtual_path(&raw.to_string_lossy());
        fs::write(&sidecar, b"{ broken json").unwrap();
        let missing = library.photo(2, "missing", SETTINGS);
        fs::remove_file(&missing).unwrap();
        let before = folder_state(library.dir.path());
        let preview = library.prepare();
        assert!(preview.photos[0].preview.error.is_some());
        assert!(!preview.photos[1].preview.found);
        assert!(
            library
                .apply(&preview.fingerprint, &[1], true, true, true)
                .is_err()
        );
        assert!(
            library
                .apply(&preview.fingerprint, &[2], true, true, true)
                .is_err()
        );
        assert_eq!(folder_state(library.dir.path()), before);
    }

    #[cfg(unix)]
    #[test]
    fn sidecar_write_failures_are_reported_without_damaging_originals() {
        use std::os::unix::fs::PermissionsExt;
        let _mode = CardMode::off();
        let library = Library::new();
        let raw = library.photo(1, "unwritable", SETTINGS);
        let preview = library.prepare();
        let original = fs::read(&raw).unwrap();
        let permissions = fs::metadata(&library.photos).unwrap().permissions();
        fs::set_permissions(&library.photos, fs::Permissions::from_mode(0o555)).unwrap();
        let result = library.apply(&preview.fingerprint, &[1], false, true, false);
        fs::set_permissions(&library.photos, permissions).unwrap();
        let result = result.unwrap();
        assert_eq!(result.imported, 0);
        assert_eq!(result.errors.len(), 1);
        assert!(!parse_virtual_path(&raw.to_string_lossy()).1.exists());
        assert_eq!(fs::read(&raw).unwrap(), original);
    }
}
