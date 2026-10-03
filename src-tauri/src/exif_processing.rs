use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::formats::is_raw_file;
use crate::image_processing::ImageMetadata;
use crate::tagging::{COLOR_TAG_PREFIX, USER_TAG_PREFIX};
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, TimeZone, Utc};
use exif::{Exif, In, Value};
use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;
use little_exif::rational::{iR64, uR64};
use rawler::decoders::RawMetadata;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct CachedExifEntry {
    mtime_ns: u128,
    size: u64,
    data: HashMap<String, String>,
}

struct ExifCacheState {
    cache: HashMap<PathBuf, HashMap<String, CachedExifEntry>>,
    dirty: HashSet<PathBuf>,
    cache_dir: Option<PathBuf>,
}

impl ExifCacheState {
    fn get_cache_file_path(&self, folder: &Path) -> Option<PathBuf> {
        let base_dir = self.cache_dir.as_ref()?;
        let hash = blake3::hash(folder.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        Some(base_dir.join(format!("{}.json", hash)))
    }
}

fn get_exif_cache() -> &'static Mutex<ExifCacheState> {
    static EXIF_CACHE: OnceLock<Mutex<ExifCacheState>> = OnceLock::new();
    EXIF_CACHE.get_or_init(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(3));
                flush_all_dirty_caches();
            }
        });

        Mutex::new(ExifCacheState {
            cache: HashMap::new(),
            dirty: HashSet::new(),
            cache_dir: None,
        })
    })
}

pub fn initialize_cache_dir(cache_dir: PathBuf) {
    let dir = cache_dir.join("exif");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut state) = get_exif_cache().lock() {
        state.cache_dir = Some(dir);
    }
}

pub fn flush_all_dirty_caches() {
    let mut state = match get_exif_cache().lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    let dirty_folders: Vec<PathBuf> = state.dirty.drain().collect();
    let mut to_write = Vec::new();

    for folder in &dirty_folders {
        if let Some(folder_map) = state.cache.get(folder)
            && !folder_map.is_empty()
            && let Some(cache_path) = state.get_cache_file_path(folder)
        {
            to_write.push((cache_path, folder_map.clone()));
        }
    }

    drop(state);

    for (path, map) in to_write {
        if let Ok(json) = serde_json::to_string(&map) {
            let tmp_path = path.with_extension("tmp");
            if std::fs::write(&tmp_path, json).is_ok() {
                let _ = std::fs::rename(tmp_path, path);
            }
        }
    }
}

fn load_rrcache_for_folder(folder: &Path) {
    let mut cache_path = None;

    if let Ok(state) = get_exif_cache().lock() {
        if state.cache.contains_key(folder) {
            return;
        }
        cache_path = state.get_cache_file_path(folder);
    }

    let Some(path) = cache_path else {
        return;
    };

    let loaded_map = if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str::<HashMap<String, CachedExifEntry>>(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    if let Ok(mut state) = get_exif_cache().lock() {
        state
            .cache
            .entry(folder.to_path_buf())
            .or_insert(loaded_map);
    }
}

fn get_file_stamp(path: &Path) -> Option<(u128, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ns = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((mtime_ns, meta.len()))
}

fn get_exif_from_rrcache(image_path: &Path) -> Option<HashMap<String, String>> {
    let folder = image_path.parent()?;
    let filename = image_path.file_name()?.to_string_lossy().to_string();

    let (current_mtime, current_size) = get_file_stamp(image_path)?;

    load_rrcache_for_folder(folder);

    let mut state = get_exif_cache().lock().ok()?;
    let folder_map = state.cache.get_mut(folder)?;

    if let Some(entry) = folder_map.get(&filename) {
        if entry.mtime_ns == current_mtime && entry.size == current_size {
            return Some(entry.data.clone());
        }

        folder_map.remove(&filename);
        state.dirty.insert(folder.to_path_buf());
    }

    None
}

fn save_exif_to_rrcache(image_path: &Path, exif: HashMap<String, String>) {
    let Some(folder) = image_path.parent() else {
        return;
    };
    let Some(filename) = image_path.file_name() else {
        return;
    };

    let Some((mtime_ns, size)) = get_file_stamp(image_path) else {
        return;
    };

    load_rrcache_for_folder(folder);

    let mut state = match get_exif_cache().lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    let folder_map = state.cache.entry(folder.to_path_buf()).or_default();
    folder_map.insert(
        filename.to_string_lossy().to_string(),
        CachedExifEntry {
            mtime_ns,
            size,
            data: exif,
        },
    );
    state.dirty.insert(folder.to_path_buf());
}

pub fn truncate_large_exif(value: &str) -> String {
    if value.len() <= 500 {
        return value.to_string();
    }

    let mut start_idx = 200;
    while !value.is_char_boundary(start_idx) {
        start_idx -= 1;
    }

    let mut end_idx = value.len() - 200;
    while !value.is_char_boundary(end_idx) {
        end_idx += 1;
    }

    if start_idx < end_idx {
        let start_str = &value[..start_idx];
        let end_str = &value[end_idx..];
        return format!("{}...{}", start_str, end_str);
    }

    value.to_string()
}

pub fn load_sidecar(sidecar_path: &Path) -> ImageMetadata {
    if !sidecar_path.exists() {
        return ImageMetadata::default();
    }

    let Ok(content) = fs::read_to_string(sidecar_path) else {
        return ImageMetadata::default();
    };

    let mut meta = serde_json::from_str::<ImageMetadata>(&content).unwrap_or_default();
    let mut healed = false;

    if let Some(ref mut exif_map) = meta.exif {
        for val in exif_map.values_mut() {
            if val.len() > 500 {
                *val = truncate_large_exif(val);
                healed = true;
            }
        }
    }

    if healed && let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = crate::file_management::write_file_atomically(sidecar_path, json);
        log::info!(
            "Auto-healed bloated sidecar for: {}",
            sidecar_path.display()
        );
    }

    meta
}

pub fn load_sidecar_with_exif(sidecar_path: &Path, source_path: &Path) -> ImageMetadata {
    let mut meta = load_sidecar(sidecar_path);

    if meta.exif.is_none() {
        if let Some(cached_exif) = read_rrexif_sidecar(source_path) {
            meta.exif = Some(cached_exif);
        } else {
            let source_path_str = source_path.to_string_lossy();
            let extracted_exif =
                if let Ok(mmap) = crate::file_management::read_file_mapped(source_path) {
                    read_exif_data(&source_path_str, &mmap)
                } else if let Ok(bytes) = std::fs::read(source_path) {
                    read_exif_data(&source_path_str, &bytes)
                } else {
                    std::collections::HashMap::new()
                };

            if !extracted_exif.is_empty() {
                meta.exif = Some(extracted_exif);
            }
        }
    }

    meta
}

fn to_ur64(val: &exif::Rational) -> uR64 {
    uR64 {
        nominator: val.num,
        denominator: val.denom,
    }
}

fn to_ir64(val: &exif::SRational) -> iR64 {
    iR64 {
        nominator: val.num,
        denominator: val.denom,
    }
}

fn clean_creation_datetime_str(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('\'').trim()
}

fn fmt_date_str(s: String) -> String {
    if let Some(dt) = parse_creation_datetime(&s) {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    clean_creation_datetime_str(&s).to_string()
}

fn normalize_creation_datetime(s: &str) -> Option<String> {
    let normalized = s.replace('T', " ");
    let (date, time) = normalized.split_once(' ')?;
    Some(format!("{} {}", date.replace(':', "-"), time))
}

fn parse_creation_datetime(s: &str) -> Option<NaiveDateTime> {
    let clean = clean_creation_datetime_str(s);
    if clean.is_empty() {
        return None;
    }

    let normalized = normalize_creation_datetime(clean);
    for candidate in std::iter::once(clean).chain(normalized.as_deref()) {
        for format in [
            "%Y:%m:%d %H:%M:%S",
            "%Y:%m:%d %H:%M:%S%.f",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M:%S%.f",
        ] {
            if let Ok(dt) = NaiveDateTime::parse_from_str(candidate, format) {
                return Some(dt);
            }
        }
    }

    None
}

fn creation_datetime_to_utc(dt: NaiveDateTime) -> DateTime<Utc> {
    match Local.from_local_datetime(&dt) {
        LocalResult::Single(local_dt) | LocalResult::Ambiguous(local_dt, _) => {
            local_dt.with_timezone(&Utc)
        }
        LocalResult::None => DateTime::from_naive_utc_and_offset(dt, Utc),
    }
}

fn parse_creation_field(field: &exif::Field) -> Option<DateTime<Utc>> {
    parse_creation_datetime(&field.display_value().to_string()).map(creation_datetime_to_utc)
}

fn parse_raw_creation_date(date_str: Option<&str>) -> Option<DateTime<Utc>> {
    parse_creation_datetime(date_str?).map(creation_datetime_to_utc)
}

fn clean_ascii_value(value: &exif::Value) -> Option<String> {
    let exif::Value::Ascii(ref components) = *value else {
        return None;
    };

    let cleaned: Vec<String> = components
        .iter()
        .map(|c| {
            String::from_utf8_lossy(c)
                .trim_matches(char::from(0))
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.join(" "))
    }
}

fn rational_to_f32_checked(r: &exif::Rational) -> Option<f32> {
    if r.denom == 0 {
        None
    } else {
        Some(r.num as f32 / r.denom as f32)
    }
}

fn rawler_rational_to_f32_checked(r: &rawler::formats::tiff::Rational) -> Option<f32> {
    if r.d == 0 {
        None
    } else {
        Some(r.n as f32 / r.d as f32)
    }
}

fn format_min_max(min: f32, max: f32, tolerance: f32) -> String {
    if (min - max).abs() < tolerance {
        format!("{min}")
    } else {
        format!("{min}-{max}")
    }
}

fn format_lens_specification(components: &[exif::Rational]) -> Option<String> {
    if components.len() < 4 {
        return None;
    }

    let focal_min = rational_to_f32_checked(&components[0]);
    let focal_max = rational_to_f32_checked(&components[1]);
    let (focal_min, focal_max) = match (focal_min, focal_max) {
        (Some(min), Some(max)) => (min, max),
        _ => return None,
    };

    let mut spec = format!("{} mm", format_min_max(focal_min, focal_max, 0.01));

    let aperture_min = rational_to_f32_checked(&components[2]);
    let aperture_max = rational_to_f32_checked(&components[3]);
    if let (Some(amin), Some(amax)) = (aperture_min, aperture_max) {
        spec.push_str(&format!(", f/{}", format_min_max(amin, amax, 0.01)));
    }

    Some(spec)
}

pub fn read_exif(file_bytes: &[u8]) -> Option<Exif> {
    let exifreader = exif::Reader::new();
    exifreader
        .read_from_container(&mut Cursor::new(file_bytes))
        .ok()
}

pub fn read_raw_metadata(file_bytes: &[u8]) -> Option<RawMetadata> {
    let loader = rawler::RawLoader::new();
    let raw_source = rawler::rawsource::RawSource::new_from_slice(file_bytes);
    let decoder = loader.get_decoder(&raw_source).ok()?;
    decoder.raw_metadata(&raw_source, &Default::default()).ok()
}

/// Map an XMP `xmp:Rating` textual value onto a star rating.
/// `-1` ("rejected") is intentionally not a star rating, so it yields `None`.
fn parse_xmp_rating(raw: &str) -> Option<u8> {
    let v: i32 = raw.trim().parse().ok()?;
    match v {
        -1 => None,
        0..=5 => Some(v as u8),
        _ => None,
    }
}

const MAX_RATING_IFDS: usize = 8;
const MAX_IFD_ENTRIES: usize = 1024;
const MAX_XMP_PACKET: u64 = 1 << 20;
const MAX_JPEG_SEGMENTS: usize = 64;
const MAX_BMFF_BOXES: usize = 64;
const XMP_UUID: [u8; 16] = [
    0xbe, 0x7a, 0xcf, 0xcb, 0x97, 0xa9, 0x42, 0xe8, 0x9c, 0x71, 0x99, 0x94, 0x91, 0xe3, 0xaf, 0xac,
];

fn xmp_rating(packet: &[u8]) -> Option<u8> {
    static PATTERN: OnceLock<regex::bytes::Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        regex::bytes::Regex::new(
            r#"(?:xmp|xap):Rating\s*=\s*["']\s*(-?[0-9]+)\s*["']|<(?:xmp|xap):Rating\s*>\s*(-?[0-9]+)\s*</(?:xmp|xap):Rating\s*>"#,
        )
        .unwrap()
    });
    pattern.captures_iter(packet).find_map(|caps| {
        let m = caps.get(1).or_else(|| caps.get(2))?;
        parse_xmp_rating(std::str::from_utf8(m.as_bytes()).ok()?)
    })
}

fn read_bytes<R: Read + Seek>(reader: &mut R, offset: u64, len: u64) -> Option<Vec<u8>> {
    if len > MAX_XMP_PACKET {
        return None;
    }
    reader.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0; len as usize];
    reader.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Star rating (0..=5) stored in the file by the camera or another tool: the
/// embedded XMP `xmp:Rating`, or else the EXIF Rating tag (0x4746). Reads only
/// the headers it needs and never decodes pixels.
pub fn read_image_rating<R: Read + Seek>(reader: &mut R) -> Option<u8> {
    let mut header = [0u8; 16];
    reader.read_exact(&mut header).ok()?;
    match header {
        [b'I', b'I', ..] | [b'M', b'M', ..] => tiff_rating(reader, 0),
        [0xff, 0xd8, ..] => jpeg_rating(reader, 2),
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => bmff_rating(reader),
        _ if &header[..15] == b"FUJIFILMCCD-RAW" => {
            let jpeg_offset = u32::from_be_bytes(read_bytes(reader, 84, 4)?.try_into().ok()?);
            let soi = read_bytes(reader, jpeg_offset as u64, 2)?;
            (soi == [0xff, 0xd8]).then_some(())?;
            jpeg_rating(reader, jpeg_offset as u64 + 2)
        }
        _ => None,
    }
}

pub fn read_embedded_rating(path: &Path) -> Option<u8> {
    let mut file = fs::File::open(path).ok()?;
    read_image_rating(&mut file)
}

/// The rating to show: the user's rating from the sidecar wins, including a
/// cleared one; otherwise the rating embedded in the file. Never writes a sidecar.
pub fn resolve_rating(image_path: &Path, metadata: &ImageMetadata) -> u8 {
    if metadata.rating != 0
        || metadata.rating_is_explicit
        || crate::file_management::is_cloud_placeholder(image_path)
    {
        return metadata.rating;
    }
    read_embedded_rating(image_path).unwrap_or(0)
}

fn tiff_rating<R: Read + Seek>(reader: &mut R, base: u64) -> Option<u8> {
    let header = read_bytes(reader, base, 8)?;
    let little = match &header[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |b: &[u8]| {
        let b = [b[0], b[1]];
        if little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    };
    let u32_at = |b: &[u8]| {
        let b = [b[0], b[1], b[2], b[3]];
        if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    };
    // 42 is TIFF; Olympus ORF and Panasonic RW2 use their own magic numbers.
    if !matches!(u16_at(&header[2..]), 42 | 0x4f52 | 0x5352 | 0x55) {
        return None;
    }

    let mut pending = vec![u32_at(&header[4..])];
    let mut visited = Vec::new();
    let mut exif_rating = None;
    while let Some(offset) = pending.pop() {
        if offset == 0 || visited.contains(&offset) || visited.len() >= MAX_RATING_IFDS {
            continue;
        }
        visited.push(offset);
        let ifd_start = base.checked_add(offset as u64)?;
        let Some(count) = read_bytes(reader, ifd_start, 2).map(|b| u16_at(&b) as usize) else {
            continue;
        };
        if count == 0 || count > MAX_IFD_ENTRIES {
            continue;
        }
        let Some(entries) = read_bytes(reader, ifd_start + 2, count as u64 * 12 + 4) else {
            continue;
        };
        for entry in entries[..count * 12].as_chunks::<12>().0 {
            let (tag, kind, len) = (u16_at(entry), u16_at(&entry[2..]), u32_at(&entry[4..]));
            match (tag, kind) {
                (0x4746, 3) if len == 1 => {
                    let value = u16_at(&entry[8..]);
                    if value <= 5 {
                        exif_rating = Some(value as u8);
                    }
                }
                (0x02bc, 1 | 7) if len > 4 => {
                    if let Some(rating) = base
                        .checked_add(u32_at(&entry[8..]) as u64)
                        .and_then(|start| read_bytes(reader, start, len as u64))
                        .and_then(|packet| xmp_rating(&packet))
                    {
                        return Some(rating);
                    }
                }
                (0x8769, 4 | 13) if len == 1 => pending.push(u32_at(&entry[8..])),
                _ => {}
            }
        }
        pending.push(u32_at(&entries[count * 12..]));
    }
    exif_rating
}

fn jpeg_rating<R: Read + Seek>(reader: &mut R, start: u64) -> Option<u8> {
    reader.seek(SeekFrom::Start(start)).ok()?;
    let mut exif_rating = None;
    for _ in 0..MAX_JPEG_SEGMENTS {
        let mut marker = [0u8; 2];
        reader.read_exact(&mut marker).ok()?;
        if marker[0] != 0xff || matches!(marker[1], 0xd9 | 0xda) {
            break;
        }
        if matches!(marker[1], 0x01 | 0xd0..=0xd8 | 0xff) {
            if marker[1] == 0xff {
                reader.seek(SeekFrom::Current(-1)).ok()?;
            }
            continue;
        }
        let mut len = [0u8; 2];
        reader.read_exact(&mut len).ok()?;
        let len = u16::from_be_bytes(len).checked_sub(2)? as usize;
        if marker[1] != 0xe1 {
            reader.seek(SeekFrom::Current(len as i64)).ok()?;
            continue;
        }
        let mut payload = vec![0; len];
        reader.read_exact(&mut payload).ok()?;
        if let Some(packet) = payload.strip_prefix(b"http://ns.adobe.com/xap/1.0/\0")
            && let Some(rating) = xmp_rating(packet)
        {
            return Some(rating);
        }
        if let Some(tiff) = payload.strip_prefix(b"Exif\0\0") {
            exif_rating = exif_rating.or_else(|| tiff_rating(&mut Cursor::new(tiff), 0));
        }
    }
    exif_rating
}

fn bmff_rating<R: Read + Seek>(reader: &mut R) -> Option<u8> {
    let mut offset = 0u64;
    for _ in 0..MAX_BMFF_BOXES {
        let header = read_bytes(reader, offset, 8)?;
        let mut size = u32::from_be_bytes(header[..4].try_into().ok()?) as u64;
        let mut header_len = 8;
        if size == 1 {
            size = u64::from_be_bytes(read_bytes(reader, offset + 8, 8)?.try_into().ok()?);
            header_len = 16;
        }
        if size < header_len {
            return None;
        }
        if &header[4..] == b"uuid" && read_bytes(reader, offset + header_len, 16)? == XMP_UUID {
            let start = offset + header_len + 16;
            let packet = read_bytes(reader, start, size.checked_sub(header_len + 16)?)?;
            return xmp_rating(&packet);
        }
        offset = offset.checked_add(size)?;
    }
    None
}

pub fn read_exposure_time_secs(path: &str, file_bytes: &[u8]) -> Option<f32> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path))
        && let Some(val_str) = map.get("ExposureTime").or(map.get("ShutterSpeedValue"))
    {
        let cleaned = val_str.replace(" s", "");
        if cleaned.contains('/') {
            let parts: Vec<&str> = cleaned.split('/').collect();
            if parts.len() == 2
                && let (Ok(num), Ok(den)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>())
                && den != 0.0
            {
                return Some(num / den);
            }
        } else if let Ok(val) = cleaned.parse::<f32>() {
            return Some(val);
        }
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if let Some(r) = meta.exif.exposure_time {
            return if r.d == 0 {
                None
            } else {
                Some(r.n as f32 / r.d as f32)
            };
        } else if let Some(r) = meta.exif.shutter_speed_value {
            return if r.d == 0 {
                None
            } else {
                Some(r.n as f32 / r.d as f32)
            };
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        if let Some(exposure) = exif.get_field(exif::Tag::ExposureTime, In::PRIMARY) {
            if let Value::Rational(ref r) = exposure.value {
                if r.is_empty() {
                    return None;
                }

                let val = r.first()?;

                return if val.denom == 0 {
                    None
                } else {
                    Some(val.num as f32 / val.denom as f32)
                };
            }
        } else if let Some(shutter_speed) =
            exif.get_field(exif::Tag::ShutterSpeedValue, In::PRIMARY)
            && let Value::Rational(ref r) = shutter_speed.value
        {
            if r.is_empty() {
                return None;
            }

            let val = r.first()?;

            return if val.denom == 0 {
                None
            } else {
                Some(val.num as f32 / val.denom as f32)
            };
        }
    }
    None
}

pub fn read_iso(path: &str, file_bytes: &[u8]) -> Option<u32> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path))
        && let Some(val_str) = map
            .get("ISOSpeed")
            .or(map.get("PhotographicSensitivity"))
            .or(map.get("ISOSpeedRatings"))
        && let Ok(val) = val_str.parse::<u32>()
    {
        return Some(val);
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if let Some(r) = meta.exif.iso_speed {
            return Some(r);
        } else if let Some(r) = meta.exif.iso_speed_ratings {
            return Some(r as u32);
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        if let Some(r) = exif.get_field(exif::Tag::ISOSpeed, In::PRIMARY) {
            return r.value.get_uint(0);
        } else if let Some(r) = exif.get_field(exif::Tag::PhotographicSensitivity, In::PRIMARY) {
            return r.value.get_uint(0);
        }
    }
    None
}

pub fn extract_metadata(file_bytes: &[u8]) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();

    if let Some(exif_obj) = read_exif(file_bytes) {
        for field in exif_obj.fields() {
            match field.tag {
                exif::Tag::ExposureTime => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let r = &v[0];
                        if r.num == 1 && r.denom > 1 {
                            map.insert("ExposureTime".to_string(), format!("1/{} s", r.denom));
                        } else {
                            let val = r.num as f32 / r.denom as f32;
                            if val < 1.0 && val > 0.0 {
                                map.insert(
                                    "ExposureTime".to_string(),
                                    format!("1/{} s", (1.0 / val).round()),
                                );
                            } else {
                                map.insert("ExposureTime".to_string(), format!("{} s", val));
                            }
                        }
                    }
                }
                exif::Tag::ShutterSpeedValue => {
                    if let exif::Value::SRational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("ShutterSpeedValue".to_string(), val.to_string());
                    }
                }
                exif::Tag::FNumber => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("FNumber".to_string(), format!("f/{}", val));
                    }
                }
                exif::Tag::ApertureValue => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("ApertureValue".to_string(), format!("f/{}", val));
                    }
                }
                exif::Tag::FocalLength => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("FocalLength".to_string(), val.to_string());
                        map.insert("FocalLengthIn35mmFilm".to_string(), val.to_string());
                    }
                }
                exif::Tag::PhotographicSensitivity | exif::Tag::ISOSpeed => {
                    map.insert(
                        "PhotographicSensitivity".to_string(),
                        field.display_value().to_string(),
                    );
                    map.insert("ISOSpeed".to_string(), field.display_value().to_string());
                }
                exif::Tag::DateTimeOriginal => {
                    map.insert(
                        "DateTimeOriginal".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::DateTime => {
                    map.insert(
                        "CreateDate".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::DateTimeDigitized => {
                    map.insert(
                        "ModifyDate".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::LensSpecification => {
                    if let exif::Value::Rational(ref v) = field.value
                        && v.len() >= 4
                        && let (Some(focal_min), Some(focal_max)) = (
                            rational_to_f32_checked(&v[0]),
                            rational_to_f32_checked(&v[1]),
                        )
                    {
                        let mut spec = format!("{} mm", format_min_max(focal_min, focal_max, 0.01));

                        let aperture = match (
                            rational_to_f32_checked(&v[2]),
                            rational_to_f32_checked(&v[3]),
                        ) {
                            (Some(amin), Some(amax)) => Some((amin, amax)),
                            _ => read_raw_metadata(file_bytes).and_then(|meta| {
                                let lens_desc = meta.lens?;
                                let amin =
                                    rawler_rational_to_f32_checked(&lens_desc.aperture_range[0])?;
                                let amax =
                                    rawler_rational_to_f32_checked(&lens_desc.aperture_range[1])?;
                                Some((amin, amax))
                            }),
                        };

                        if let Some((amin, amax)) = aperture
                            && (amin > 0.0 || amax > 0.0)
                        {
                            spec.push_str(&format!(", f/{}", format_min_max(amin, amax, 0.01)));
                        }

                        map.insert("LensSpecification".to_string(), spec);
                    }
                }
                _ => match &field.value {
                    exif::Value::Ascii(_) => {
                        if let Some(val) = clean_ascii_value(&field.value) {
                            map.insert(field.tag.to_string(), truncate_large_exif(&val));
                        }
                    }
                    _ => {
                        let val = field.display_value().with_unit(&exif_obj).to_string();
                        if !val.trim().is_empty() {
                            map.insert(field.tag.to_string(), truncate_large_exif(&val));
                        }
                    }
                },
            }
        }
    }

    if !map.is_empty() {
        return Some(map);
    }

    let metadata = read_raw_metadata(file_bytes)?;

    let exif = metadata.exif;

    let fmt_rat = |r: &rawler::formats::tiff::Rational| -> f32 {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };

    let fmt_srat = |r: &rawler::formats::tiff::SRational| -> f32 {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };

    let mut insert_if_present = |key: &str, val: String| {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            map.insert(key.to_string(), truncate_large_exif(trimmed));
        }
    };

    insert_if_present("Make", metadata.make);
    insert_if_present("Model", metadata.model);

    if let Some(v) = exif.artist {
        insert_if_present("Artist", v);
    }
    if let Some(v) = exif.copyright {
        insert_if_present("Copyright", v);
    }
    if let Some(v) = exif.owner_name {
        insert_if_present("OwnerName", v);
    }
    if let Some(v) = exif.serial_number {
        insert_if_present("SerialNumber", v);
    }
    if let Some(v) = exif.image_number {
        insert_if_present("ImageNumber", v.to_string());
    }
    if let Some(v) = exif.user_comment {
        insert_if_present("UserComment", v);
    }

    if let Some(v) = exif.date_time_original {
        insert_if_present("DateTimeOriginal", fmt_date_str(v));
    }
    if let Some(v) = exif.create_date {
        insert_if_present("CreateDate", fmt_date_str(v));
    }
    if let Some(v) = exif.modify_date {
        insert_if_present("ModifyDate", fmt_date_str(v));
    }

    if let Some(v) = exif.offset_time {
        insert_if_present("OffsetTime", v);
    }
    if let Some(v) = exif.offset_time_original {
        insert_if_present("OffsetTimeOriginal", v);
    }
    if let Some(v) = exif.offset_time_digitized {
        insert_if_present("OffsetTimeDigitized", v);
    }
    if let Some(v) = exif.sub_sec_time {
        insert_if_present("SubSecTime", v);
    }
    if let Some(v) = exif.sub_sec_time_original {
        insert_if_present("SubSecTimeOriginal", v);
    }
    if let Some(v) = exif.sub_sec_time_digitized {
        insert_if_present("SubSecTimeDigitized", v);
    }

    if let Some(v) = exif.lens_model {
        insert_if_present("LensModel", v);
    } else if let Some(lens_desc) = &metadata.lens {
        insert_if_present("LensModel", lens_desc.lens_model.clone());
    }

    if let Some(v) = exif.lens_make {
        insert_if_present("LensMake", v);
    } else if let Some(lens_desc) = &metadata.lens {
        insert_if_present("LensMake", lens_desc.lens_make.clone());
    }

    if let Some(v) = exif.lens_serial_number {
        insert_if_present("LensSerialNumber", v);
    }

    if let Some(lens_desc) = &metadata.lens {
        let focal_min = fmt_rat(&lens_desc.focal_range[0]);
        let focal_max = fmt_rat(&lens_desc.focal_range[1]);
        let mut spec = format!("{} mm", format_min_max(focal_min, focal_max, 0.01));

        let aperture_min = fmt_rat(&lens_desc.aperture_range[0]);
        let aperture_max = fmt_rat(&lens_desc.aperture_range[1]);
        if aperture_min > 0.0 || aperture_max > 0.0 {
            spec.push_str(&format!(
                ", f/{}",
                format_min_max(aperture_min, aperture_max, 0.01)
            ));
        }

        insert_if_present("LensSpecification", spec);
    }

    if let Some(v) = exif.orientation {
        insert_if_present("Orientation", v.to_string());
    }

    if let Some(r) = exif.fnumber {
        let val = fmt_rat(&r);
        insert_if_present("FNumber", format!("f/{}", val));
    }

    if let Some(r) = exif.aperture_value {
        let val = fmt_rat(&r);
        insert_if_present("ApertureValue", format!("f/{}", val));
    }

    if let Some(r) = exif.max_aperture_value {
        insert_if_present("MaxApertureValue", fmt_rat(&r).to_string());
    }

    if let Some(r) = exif.exposure_time {
        if r.n == 1 && r.d > 1 {
            insert_if_present("ExposureTime", format!("1/{} s", r.d));
        } else {
            let val = fmt_rat(&r);
            if val < 1.0 && val > 0.0 {
                insert_if_present("ExposureTime", format!("1/{} s", (1.0 / val).round()));
            } else {
                insert_if_present("ExposureTime", format!("{} s", val));
            }
        }
    }

    if let Some(r) = exif.shutter_speed_value {
        insert_if_present("ShutterSpeedValue", fmt_srat(&r).to_string());
    }

    if let Some(v) = exif.iso_speed {
        insert_if_present("PhotographicSensitivity", v.to_string());
        insert_if_present("ISOSpeed", v.to_string());
    } else if let Some(v) = exif.iso_speed_ratings {
        insert_if_present("PhotographicSensitivity", v.to_string());
        insert_if_present("ISOSpeedRatings", v.to_string());
    }

    if let Some(v) = exif.recommended_exposure_index {
        insert_if_present("RecommendedExposureIndex", v.to_string());
    }
    if let Some(v) = exif.sensitivity_type {
        insert_if_present("SensitivityType", v.to_string());
    }

    if let Some(r) = exif.focal_length {
        let val = fmt_rat(&r);
        insert_if_present("FocalLength", val.to_string());
        insert_if_present("FocalLengthIn35mmFilm", val.to_string());
    }

    if let Some(r) = exif.exposure_bias {
        insert_if_present("ExposureBiasValue", fmt_srat(&r).to_string());
    }

    if let Some(v) = exif.metering_mode {
        insert_if_present("MeteringMode", v.to_string());
    }
    if let Some(v) = exif.light_source {
        insert_if_present("LightSource", v.to_string());
    }
    if let Some(v) = exif.flash {
        insert_if_present("Flash", v.to_string());
    }
    if let Some(v) = exif.white_balance {
        insert_if_present("WhiteBalance", v.to_string());
    }
    if let Some(v) = exif.exposure_program {
        insert_if_present("ExposureProgram", v.to_string());
    }
    if let Some(v) = exif.exposure_mode {
        insert_if_present("ExposureMode", v.to_string());
    }
    if let Some(v) = exif.scene_capture_type {
        insert_if_present("SceneCaptureType", v.to_string());
    }
    if let Some(v) = exif.color_space {
        insert_if_present("ColorSpace", v.to_string());
    }
    if let Some(r) = exif.flash_energy {
        insert_if_present("FlashEnergy", fmt_rat(&r).to_string());
    }
    if let Some(r) = exif.brightness_value {
        insert_if_present("BrightnessValue", fmt_srat(&r).to_string());
    }

    if let Some(r) = exif.subject_distance {
        insert_if_present("SubjectDistance", fmt_rat(&r).to_string());
    }
    if let Some(v) = exif.subject_distance_range {
        insert_if_present("SubjectDistanceRange", v.to_string());
    }

    if let Some(gps) = exif.gps {
        let fmt_gps_coord = |coords: &[rawler::formats::tiff::Rational; 3]| -> String {
            format!(
                "{} deg {} min {} sec",
                fmt_rat(&coords[0]),
                fmt_rat(&coords[1]),
                fmt_rat(&coords[2])
            )
        };

        if let Some(lat) = gps.gps_latitude {
            insert_if_present("GPSLatitude", fmt_gps_coord(&lat));
        }
        if let Some(lat_ref) = gps.gps_latitude_ref {
            insert_if_present("GPSLatitudeRef", lat_ref);
        }
        if let Some(lon) = gps.gps_longitude {
            insert_if_present("GPSLongitude", fmt_gps_coord(&lon));
        }
        if let Some(lon_ref) = gps.gps_longitude_ref {
            insert_if_present("GPSLongitudeRef", lon_ref);
        }
        if let Some(alt) = gps.gps_altitude {
            insert_if_present("GPSAltitude", fmt_rat(&alt).to_string());
        }
        if let Some(alt_ref) = gps.gps_altitude_ref {
            insert_if_present("GPSAltitudeRef", alt_ref.to_string());
        }
        if let Some(v) = gps.gps_img_direction {
            insert_if_present("GPSImgDirection", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_img_direction_ref {
            insert_if_present("GPSImgDirectionRef", v);
        }
        if let Some(v) = gps.gps_speed {
            insert_if_present("GPSSpeed", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_speed_ref {
            insert_if_present("GPSSpeedRef", v);
        }
        if let Some(v) = gps.gps_status {
            insert_if_present("GPSStatus", v);
        }
        if let Some(v) = gps.gps_measure_mode {
            insert_if_present("GPSMeasureMode", v);
        }
        if let Some(v) = gps.gps_dop {
            insert_if_present("GPSDOP", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_map_datum {
            insert_if_present("GPSMapDatum", v);
        }
    }

    Some(map)
}

pub fn get_creation_date_from_path(path: &Path) -> DateTime<Utc> {
    if let Some(dt) = try_get_exif_creation_date(path) {
        return dt;
    }

    fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now)
}

pub fn try_get_exif_creation_date(path: &Path) -> Option<DateTime<Utc>> {
    if let Some(map) = read_rrexif_sidecar(path)
        && let Some(dt_str) = map.get("DateTimeOriginal").or(map.get("CreateDate"))
        && let Some(dt) = parse_creation_datetime(dt_str)
    {
        return Some(creation_datetime_to_utc(dt));
    }

    if let Ok(file) = std::fs::File::open(path) {
        let mut bufreader = BufReader::new(&file);
        let exifreader = exif::Reader::new();

        if let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) {
            for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTime] {
                if let Some(field) = exif_obj.get_field(tag, exif::In::PRIMARY)
                    && let Some(dt) = parse_creation_field(field)
                {
                    return Some(dt);
                }
            }
        }
    }

    if is_raw_file(path) {
        let loader = rawler::RawLoader::new();
        if let Ok(raw_source) = rawler::rawsource::RawSource::new(path)
            && let Ok(decoder) = loader.get_decoder(&raw_source)
            && let Ok(metadata) = decoder.raw_metadata(&raw_source, &Default::default())
        {
            if let Some(dt) = parse_raw_creation_date(metadata.exif.date_time_original.as_deref()) {
                return Some(dt);
            }
            if let Some(dt) = parse_raw_creation_date(metadata.exif.create_date.as_deref()) {
                return Some(dt);
            }
        }
    }

    None
}

#[cfg(target_os = "android")]
pub fn get_creation_date_from_bytes(path_hint: &str, file_bytes: &[u8]) -> DateTime<Utc> {
    if let Some(exif_obj) = read_exif(file_bytes) {
        for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTime] {
            if let Some(field) = exif_obj.get_field(tag, exif::In::PRIMARY)
                && let Some(dt) = parse_creation_field(field)
            {
                return dt;
            }
        }
    }

    if is_raw_file(path_hint)
        && let Some(metadata) = read_raw_metadata(file_bytes)
    {
        if let Some(dt) = parse_raw_creation_date(metadata.exif.date_time_original.as_deref()) {
            return dt;
        }
        if let Some(dt) = parse_raw_creation_date(metadata.exif.create_date.as_deref()) {
            return dt;
        }
    }

    Utc::now()
}

fn copy_full_exif_from_source(
    metadata: &mut Metadata,
    original_path: &Path,
    strip_gps: bool,
) -> bool {
    let Ok(source_metadata) = Metadata::new_from_path(original_path) else {
        return false;
    };

    let mut copied_any = false;
    for ifd in source_metadata.get_ifds() {
        if ifd.get_generic_ifd_nr() != 0 {
            continue;
        }
        if strip_gps && ifd.get_ifd_type() == ExifTagGroup::GPS {
            continue;
        }
        for tag in ifd.get_tags() {
            metadata.set_tag(tag.clone());
            copied_any = true;
        }
    }
    copied_any
}

fn encode_user_comment(comment: &str) -> Vec<u8> {
    if comment.is_ascii() {
        let mut bytes = b"ASCII\0\0\0".to_vec();
        bytes.extend_from_slice(comment.as_bytes());
        bytes
    } else {
        let mut bytes = b"UNICODE\0".to_vec();
        bytes.extend(comment.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }
}

fn apply_sidecar_field_overrides(metadata: &mut Metadata, map: &HashMap<String, String>) {
    let clean_s = |s: &String| s.replace('"', "").trim().to_string();
    let is_user_edit = |s: &str| !s.is_empty() && s != "...";

    match map.get("Artist").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::Artist(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::Artist(String::new()));
        }
    }
    match map.get("Copyright").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::Copyright(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::Copyright(String::new()));
        }
    }
    match map.get("ImageDescription").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::ImageDescription(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::ImageDescription(String::new()));
        }
    }
    match map.get("UserComment").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) && !val.starts_with("0x") {
                metadata.set_tag(ExifTag::UserComment(encode_user_comment(&val)));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::UserComment(Vec::new()));
        }
    }
}

fn apply_gps_from_kamadak(metadata: &mut Metadata, original_path: &Path) {
    let Ok(file) = std::fs::File::open(original_path) else {
        return;
    };
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) else {
        return;
    };

    let get_string_val = |field: &exif::Field| -> String {
        match &field.value {
            exif::Value::Ascii(vec) => vec
                .iter()
                .map(|v| {
                    String::from_utf8_lossy(v)
                        .trim_matches(char::from(0))
                        .to_string()
                })
                .collect::<Vec<String>>()
                .join(" "),
            _ => field
                .display_value()
                .to_string()
                .replace("\"", "")
                .trim()
                .to_string(),
        }
    };

    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && v.len() >= 3
    {
        metadata.set_tag(ExifTag::GPSLatitude(vec![
            to_ur64(&v[0]),
            to_ur64(&v[1]),
            to_ur64(&v[2]),
        ]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY) {
        metadata.set_tag(ExifTag::GPSLatitudeRef(get_string_val(f)));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && v.len() >= 3
    {
        metadata.set_tag(ExifTag::GPSLongitude(vec![
            to_ur64(&v[0]),
            to_ur64(&v[1]),
            to_ur64(&v[2]),
        ]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY) {
        metadata.set_tag(ExifTag::GPSLongitudeRef(get_string_val(f)));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && !v.is_empty()
    {
        metadata.set_tag(ExifTag::GPSAltitude(vec![to_ur64(&v[0])]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY)
        && let Some(val) = f.value.get_uint(0)
    {
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![val as u8]));
    }
}

fn apply_gps_from_rawler(metadata: &mut Metadata, original_path_str: &str) {
    let loader = rawler::RawLoader::new();
    let Ok(raw_source) = rawler::rawsource::RawSource::new(Path::new(original_path_str)) else {
        return;
    };
    let Ok(decoder) = loader.get_decoder(&raw_source) else {
        return;
    };
    let Ok(meta) = decoder.raw_metadata(&raw_source, &Default::default()) else {
        return;
    };
    let Some(gps) = meta.exif.gps else {
        return;
    };
    if let Some(lat) = gps.gps_latitude {
        metadata.set_tag(ExifTag::GPSLatitude(vec![
            uR64 {
                nominator: lat[0].n,
                denominator: lat[0].d,
            },
            uR64 {
                nominator: lat[1].n,
                denominator: lat[1].d,
            },
            uR64 {
                nominator: lat[2].n,
                denominator: lat[2].d,
            },
        ]));
    }
    if let Some(lat_ref) = gps.gps_latitude_ref {
        metadata.set_tag(ExifTag::GPSLatitudeRef(lat_ref));
    }
    if let Some(lon) = gps.gps_longitude {
        metadata.set_tag(ExifTag::GPSLongitude(vec![
            uR64 {
                nominator: lon[0].n,
                denominator: lon[0].d,
            },
            uR64 {
                nominator: lon[1].n,
                denominator: lon[1].d,
            },
            uR64 {
                nominator: lon[2].n,
                denominator: lon[2].d,
            },
        ]));
    }
    if let Some(lon_ref) = gps.gps_longitude_ref {
        metadata.set_tag(ExifTag::GPSLongitudeRef(lon_ref));
    }
    if let Some(alt) = gps.gps_altitude {
        metadata.set_tag(ExifTag::GPSAltitude(vec![uR64 {
            nominator: alt.n,
            denominator: alt.d,
        }]));
    }
    if let Some(alt_ref) = gps.gps_altitude_ref {
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![alt_ref]));
    }
}

pub fn write_image_with_metadata(
    image_bytes: &mut Vec<u8>,
    original_path_str: &str,
    output_format: &str,
    keep_metadata: bool,
    strip_gps: bool,
    tags: Option<&[String]>,
) -> Result<(), String> {
    // FIXME: temporary solution until I find a way to write metadata to TIFF
    if !keep_metadata || output_format.to_lowercase() == "tiff" {
        return Ok(());
    }

    let original_path = Path::new(original_path_str);
    if !original_path.exists() {
        return Ok(());
    }

    // Skip TIFF sources to avoid potential tag corruption issues
    let original_ext = original_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if original_ext == "tiff" || original_ext == "tif" {
        return Ok(());
    }

    let file_type = match output_format.to_lowercase().as_str() {
        "jpg" | "jpeg" => FileExtension::JPEG,
        "png" => FileExtension::PNG {
            as_zTXt_chunk: true,
        },
        "tiff" => FileExtension::TIFF,
        "webp" => FileExtension::WEBP,
        _ => return Ok(()),
    };

    let mut metadata = Metadata::new();

    let full_exif_copied = !is_raw_file(original_path_str)
        && copy_full_exif_from_source(&mut metadata, original_path, strip_gps);
    let mut source_read_success = full_exif_copied;

    if !source_read_success && let Some(map) = read_rrexif_sidecar(original_path) {
        source_read_success = true;

        let clean_s = |s: &String| s.replace('"', "").trim().to_string();

        let parse_ur64 = |s: &str| -> Option<uR64> {
            let cleaned_string = s
                .replace("f/", "")
                .replace(" s", "")
                .replace(" mm", "")
                .replace("\"", "");

            let val = cleaned_string.trim();

            if val.contains('/') {
                let parts: Vec<&str> = val.split('/').collect();
                if parts.len() == 2
                    && let (Ok(n), Ok(d)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                {
                    return Some(uR64 {
                        nominator: n,
                        denominator: d,
                    });
                }
            } else if let Ok(f) = val.parse::<f32>() {
                return Some(uR64 {
                    nominator: (f * 1000.0) as u32,
                    denominator: 1000,
                });
            }
            None
        };
        if let Some(val) = map.get("Make") {
            metadata.set_tag(ExifTag::Make(clean_s(val)));
        }
        if let Some(val) = map.get("Model") {
            metadata.set_tag(ExifTag::Model(clean_s(val)));
        }
        if let Some(val) = map.get("LensMake") {
            metadata.set_tag(ExifTag::LensMake(clean_s(val)));
        }
        if let Some(val) = map.get("LensModel") {
            metadata.set_tag(ExifTag::LensModel(clean_s(val)));
        }
        if let Some(val) = map.get("Artist") {
            metadata.set_tag(ExifTag::Artist(clean_s(val)));
        }
        if let Some(val) = map.get("Copyright") {
            metadata.set_tag(ExifTag::Copyright(clean_s(val)));
        }
        if let Some(val) = map.get("UserComment") {
            metadata.set_tag(ExifTag::UserComment(encode_user_comment(&clean_s(val))));
        }
        if let Some(val) = map.get("ImageDescription") {
            metadata.set_tag(ExifTag::ImageDescription(clean_s(val)));
        }
        if let Some(val) = map.get("DateTimeOriginal") {
            metadata.set_tag(ExifTag::DateTimeOriginal(clean_s(val)));
        }
        if let Some(val) = map.get("CreateDate") {
            metadata.set_tag(ExifTag::CreateDate(clean_s(val)));
        }
        if let Some(val) = map.get("FNumber")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::FNumber(vec![ur]));
        }
        if let Some(val) = map.get("ExposureTime")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::ExposureTime(vec![ur]));
        }
        if let Some(val) = map.get("FocalLength")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::FocalLength(vec![ur]));
        }
        if let Some(val) = map.get("FocalLengthIn35mmFilm") {
            let cleaned = val.replace(" mm", "").replace("\"", "");
            let trimmed = cleaned.trim();
            if let Ok(f_val) = trimmed.parse::<f32>() {
                metadata.set_tag(ExifTag::FocalLengthIn35mmFormat(vec![f_val.round() as u16]));
            }
        }
        if let Some(val) = map.get("ISOSpeed").or(map.get("PhotographicSensitivity"))
            && let Ok(iso) = val.replace('"', "").trim().parse::<u16>()
        {
            metadata.set_tag(ExifTag::ISO(vec![iso]));
        }
    }

    if !source_read_success && let Ok(file) = std::fs::File::open(original_path) {
        let mut bufreader = std::io::BufReader::new(&file);
        let exifreader = exif::Reader::new();

        if let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) {
            source_read_success = true;

            let get_string_val = |field: &exif::Field| -> String {
                match &field.value {
                    exif::Value::Ascii(vec) => vec
                        .iter()
                        .map(|v| {
                            String::from_utf8_lossy(v)
                                .trim_matches(char::from(0))
                                .to_string()
                        })
                        .collect::<Vec<String>>()
                        .join(" "),
                    _ => field
                        .display_value()
                        .to_string()
                        .replace("\"", "")
                        .trim()
                        .to_string(),
                }
            };

            if let Some(f) = exif_obj.get_field(exif::Tag::Make, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Make(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Model, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Model(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::LensMake, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::LensMake(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::LensModel(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Artist, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Artist(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Copyright, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Copyright(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::DateTimeOriginal(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::DateTime, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::CreateDate(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FNumber, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::FNumber(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::ExposureTime(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::FocalLength(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::ExposureBiasValue, exif::In::PRIMARY) {
                match &f.value {
                    exif::Value::SRational(v) if !v.is_empty() => {
                        metadata.set_tag(ExifTag::ExposureCompensation(vec![to_ir64(&v[0])]));
                    }
                    exif::Value::Rational(v) if !v.is_empty() => {
                        metadata.set_tag(ExifTag::ExposureCompensation(vec![iR64 {
                            nominator: v[0].num as i32,
                            denominator: v[0].denom as i32,
                        }]));
                    }
                    _ => {}
                }
            }
            if let Some(f) =
                exif_obj.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
            {
                if let Some(val) = f.value.get_uint(0) {
                    metadata.set_tag(ExifTag::ISO(vec![val as u16]));
                }
            } else if let Some(f) = exif_obj.get_field(exif::Tag::ISOSpeed, exif::In::PRIMARY)
                && let Some(val) = f.value.get_uint(0)
            {
                metadata.set_tag(ExifTag::ISO(vec![val as u16]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FocalLengthIn35mmFilm, exif::In::PRIMARY)
                && let Some(val) = f.value.get_uint(0)
            {
                metadata.set_tag(ExifTag::FocalLengthIn35mmFormat(vec![val as u16]));
            }
        }
    }

    if !source_read_success && is_raw_file(original_path_str) {
        let loader = rawler::RawLoader::new();
        if let Ok(raw_source) = rawler::rawsource::RawSource::new(Path::new(original_path_str))
            && let Ok(decoder) = loader.get_decoder(&raw_source)
            && let Ok(meta) = decoder.raw_metadata(&raw_source, &Default::default())
        {
            if !meta.make.is_empty() {
                metadata.set_tag(ExifTag::Make(meta.make.clone()));
            }
            if !meta.model.is_empty() {
                metadata.set_tag(ExifTag::Model(meta.model.clone()));
            }
            let exif = meta.exif;
            if let Some(artist) = exif.artist {
                metadata.set_tag(ExifTag::Artist(artist));
            }
            if let Some(copyright) = exif.copyright {
                metadata.set_tag(ExifTag::Copyright(copyright));
            }
            if let Some(dt) = exif.date_time_original {
                metadata.set_tag(ExifTag::DateTimeOriginal(dt));
            }
            if let Some(dt) = exif.create_date {
                metadata.set_tag(ExifTag::CreateDate(dt));
            }
            if let Some(lens_make) = exif.lens_make {
                metadata.set_tag(ExifTag::LensMake(lens_make));
            }
            if let Some(lens_model) = exif.lens_model {
                metadata.set_tag(ExifTag::LensModel(lens_model));
            }
            if let Some(f) = exif.fnumber {
                metadata.set_tag(ExifTag::FNumber(vec![uR64 {
                    nominator: f.n,
                    denominator: f.d,
                }]));
            }
            if let Some(t) = exif.exposure_time {
                metadata.set_tag(ExifTag::ExposureTime(vec![uR64 {
                    nominator: t.n,
                    denominator: t.d,
                }]));
            }
            if let Some(fl) = exif.focal_length {
                metadata.set_tag(ExifTag::FocalLength(vec![uR64 {
                    nominator: fl.n,
                    denominator: fl.d,
                }]));
            }
            if let Some(iso) = exif.iso_speed {
                metadata.set_tag(ExifTag::ISO(vec![iso as u16]));
            } else if let Some(iso) = exif.iso_speed_ratings {
                metadata.set_tag(ExifTag::ISO(vec![iso]));
            }
            if let Some(ev) = exif.exposure_bias {
                metadata.set_tag(ExifTag::ExposureCompensation(vec![iR64 {
                    nominator: ev.n,
                    denominator: ev.d,
                }]));
            }
            if let Some(flash) = exif.flash {
                metadata.set_tag(ExifTag::Flash(vec![flash]));
            }
            if let Some(metering) = exif.metering_mode {
                metadata.set_tag(ExifTag::MeteringMode(vec![metering]));
            }
            if let Some(wb) = exif.white_balance {
                metadata.set_tag(ExifTag::WhiteBalance(vec![wb]));
            }
            if let Some(prog) = exif.exposure_program {
                metadata.set_tag(ExifTag::ExposureProgram(vec![prog]));
            }
        }
    }

    if !strip_gps {
        if is_raw_file(original_path_str) {
            apply_gps_from_rawler(&mut metadata, original_path_str);
        } else {
            apply_gps_from_kamadak(&mut metadata, original_path);
        }
    }

    if full_exif_copied && let Some(map) = read_rrexif_sidecar(original_path) {
        apply_sidecar_field_overrides(&mut metadata, &map);
    }

    metadata.set_tag(ExifTag::Software("RapidRAW".to_string()));
    metadata.set_tag(ExifTag::Orientation(vec![1u16]));
    metadata.set_tag(ExifTag::ColorSpace(vec![1u16]));

    if let Ok(reader) =
        image::ImageReader::new(Cursor::new(image_bytes.as_slice())).with_guessed_format()
        && let Ok((width, height)) = reader.into_dimensions()
    {
        metadata.set_tag(ExifTag::ExifImageWidth(vec![width]));
        metadata.set_tag(ExifTag::ExifImageHeight(vec![height]));
    }

    if let Err(e) = metadata.write_to_vec(image_bytes, file_type) {
        log::warn!("Failed to write metadata: {}", e);
    }

    if let Some(tags) = tags
        && !tags.is_empty()
    {
        inject_xmp_keywords(image_bytes, output_format, tags);
    }

    Ok(())
}

pub fn get_primary_sidecar_path(image_path: &Path) -> PathBuf {
    let mut filename = image_path.file_name().unwrap_or_default().to_os_string();
    filename.push(".rrdata");
    image_path.with_file_name(filename)
}

pub fn get_rrexif_path(image_path: &Path) -> PathBuf {
    let mut filename = image_path.file_name().unwrap_or_default().to_os_string();
    filename.push(".rrexif");
    image_path.with_file_name(filename)
}

fn load_primary_metadata(image_path: &Path) -> ImageMetadata {
    let primary = get_primary_sidecar_path(image_path);
    load_sidecar(&primary)
}

pub fn load_tags_from_sidecar(image_path: &Path) -> Option<Vec<String>> {
    let metadata = load_primary_metadata(image_path);
    let keywords = export_keywords(&metadata.tags.unwrap_or_default());
    (!keywords.is_empty()).then_some(keywords)
}

fn export_keywords(tags: &[String]) -> Vec<String> {
    let mut keywords: Vec<String> = Vec::new();
    for tag in tags {
        if tag.starts_with(COLOR_TAG_PREFIX) {
            continue;
        }
        let keyword = tag.strip_prefix(USER_TAG_PREFIX).unwrap_or(tag).trim();
        if !keyword.is_empty() && !keywords.iter().any(|k| k == keyword) {
            keywords.push(keyword.to_string());
        }
    }
    keywords
}

fn save_primary_metadata(image_path: &Path, metadata: &ImageMetadata) -> std::io::Result<()> {
    let primary = get_primary_sidecar_path(image_path);
    let json = serde_json::to_string_pretty(metadata).map_err(std::io::Error::other)?;
    crate::file_management::write_file_atomically(&primary, json)
}

pub fn read_rrexif_sidecar(image_path: &Path) -> Option<HashMap<String, String>> {
    let primary = get_primary_sidecar_path(image_path);

    if primary.exists() {
        let metadata = load_primary_metadata(image_path);
        if let Some(exif) = metadata.exif {
            return Some(exif);
        }
    }

    if let Some(exif) = get_exif_from_rrcache(image_path) {
        return Some(exif);
    }

    let legacy = get_rrexif_path(image_path);
    if legacy.exists()
        && let Ok(content) = fs::read_to_string(&legacy)
        && let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content)
    {
        save_exif_to_rrcache(image_path, map.clone());
        if !crate::file_management::is_card_read_only_path(&legacy) {
            let _ = fs::remove_file(&legacy);
        }
        return Some(map);
    }

    None
}

pub fn read_exif_data_from_bytes(path: &str, file_bytes: &[u8]) -> HashMap<String, String> {
    if is_raw_file(path)
        && let Some(map) = extract_metadata(file_bytes)
    {
        return map;
    }

    let mut exif_data = HashMap::new();
    if let Some(exif) = read_exif(file_bytes) {
        for field in exif.fields() {
            let raw_val = match &field.value {
                exif::Value::Ascii(_) => match clean_ascii_value(&field.value) {
                    Some(v) => v,
                    None => continue,
                },
                exif::Value::Rational(v) if field.tag == exif::Tag::LensSpecification => {
                    match format_lens_specification(v) {
                        Some(s) => s,
                        None => continue,
                    }
                }
                _ => field.display_value().with_unit(&exif).to_string(),
            };
            exif_data.insert(field.tag.to_string(), truncate_large_exif(&raw_val));
        }
    }
    exif_data
}

pub fn read_exif_data(path: &str, file_bytes: &[u8]) -> HashMap<String, String> {
    let source_path = Path::new(path);

    if let Some(cached_exif) = read_rrexif_sidecar(source_path) {
        return cached_exif;
    }

    let exif_map = read_exif_data_from_bytes(path, file_bytes);
    if !exif_map.is_empty() {
        let primary = get_primary_sidecar_path(source_path);
        if primary.exists() {
            let mut metadata = load_primary_metadata(source_path);
            metadata.exif = Some(exif_map.clone());
            let _ = save_primary_metadata(source_path, &metadata);
        } else {
            save_exif_to_rrcache(source_path, exif_map.clone());
        }
    }

    exif_map
}

pub fn persist_exif_if_missing(source_path: &Path, source_path_str: &str, file_bytes: &[u8]) {
    if read_rrexif_sidecar(source_path).is_some() {
        return;
    }

    let exif_map = read_exif_data_from_bytes(source_path_str, file_bytes);
    if exif_map.is_empty() {
        return;
    }

    let primary = get_primary_sidecar_path(source_path);

    if primary.exists() {
        let mut metadata = load_primary_metadata(source_path);
        if metadata.exif.is_none() {
            metadata.exif = Some(exif_map);
            let _ = save_primary_metadata(source_path, &metadata);
        }
    } else {
        save_exif_to_rrcache(source_path, exif_map);
    }
}

pub fn write_rrexif_sidecar(source_path_str: &str, target_image_path: &Path) -> Result<(), String> {
    let source_path = Path::new(source_path_str);

    let exif_data = if let Some(existing) = read_rrexif_sidecar(source_path) {
        existing
    } else if let Ok(bytes) = fs::read(source_path) {
        read_exif_data_from_bytes(source_path_str, &bytes)
    } else {
        return Ok(());
    };

    if exif_data.is_empty() {
        return Ok(());
    }

    let mut metadata = load_primary_metadata(target_image_path);
    metadata.exif = Some(exif_data);
    save_primary_metadata(target_image_path, &metadata)
        .map_err(|e| format!("Failed to write sidecar: {}", e))
}

/// Builds a minimal XMP packet containing dc:subject keywords and injects
/// it into the image bytes in the correct location for each container format.
fn inject_xmp_keywords(image_bytes: &mut Vec<u8>, format: &str, tags: &[String]) {
    let xmp_packet = build_xmp_keywords_packet(tags);
    let xmp_bytes = xmp_packet.as_bytes();

    match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => inject_xmp_jpeg(image_bytes, xmp_bytes),
        "png" => inject_xmp_png(image_bytes, xmp_bytes),
        "webp" => inject_xmp_webp(image_bytes, xmp_bytes),
        _ => {
            log::debug!("XMP keyword injection not supported for format: {}", format);
        }
    }
}

/// Builds a minimal XMP packet string containing dc:subject keywords.
fn build_xmp_keywords_packet(tags: &[String]) -> String {
    let items: String = tags
        .iter()
        .map(|t| {
            let escaped = t
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&apos;");
            format!("      <rdf:li>{}</rdf:li>\n", escaped)
        })
        .collect();

    format!(
        "<?xpacket begin='\u{FEFF}' id='W5M0MpCehiHzreSzNTczkc9d'?>\n\
         <x:xmpmeta xmlns:x='adobe:ns:meta/'>\n\
           <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\n\
             <rdf:Description rdf:about='' xmlns:dc='http://purl.org/dc/elements/1.1/'>\n\
               <dc:subject>\n\
                 <rdf:Bag>\n\
         {}\
                 </rdf:Bag>\n\
               </dc:subject>\n\
             </rdf:Description>\n\
           </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end='w'?>",
        items
    )
}

/// Injects XMP as a JPEG APP1 segment after the SOI marker and any leading
/// JFIF APP0 / Exif APP1 segments, which readers expect to come first.
fn inject_xmp_jpeg(image_bytes: &mut Vec<u8>, xmp_bytes: &[u8]) {
    const NAMESPACE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let segment_data_len = 2 + NAMESPACE.len() + xmp_bytes.len();
    if segment_data_len > 0xFFFF {
        log::warn!(
            "XMP keyword packet too large for JPEG APP1 segment ({} bytes); skipping",
            segment_data_len
        );
        return;
    }
    let length = segment_data_len as u16;

    let mut segment = Vec::with_capacity(2 + segment_data_len);
    segment.extend_from_slice(&[0xFF, 0xE1]);
    segment.extend_from_slice(&length.to_be_bytes());
    segment.extend_from_slice(NAMESPACE);
    segment.extend_from_slice(xmp_bytes);

    if image_bytes.len() >= 2 && image_bytes[0] == 0xFF && image_bytes[1] == 0xD8 {
        let pos = jpeg_xmp_insert_pos(image_bytes);
        image_bytes.splice(pos..pos, segment);
    } else {
        log::warn!("JPEG bytes do not begin with SOI marker; skipping XMP injection");
    }
}

fn jpeg_xmp_insert_pos(bytes: &[u8]) -> usize {
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() && bytes[pos] == 0xFF {
        let marker = bytes[pos + 1];
        let length = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        let is_jfif = marker == 0xE0;
        let is_exif = marker == 0xE1 && bytes[pos + 4..].starts_with(b"Exif\0\0");
        if !(is_jfif || is_exif) || pos + 2 + length > bytes.len() {
            break;
        }
        pos += 2 + length;
    }
    pos
}

/// Injects XMP as a PNG iTXt chunk before the first IDAT chunk, where all readers look for it.
fn inject_xmp_png(image_bytes: &mut Vec<u8>, xmp_bytes: &[u8]) {
    const KEYWORD: &[u8] = b"XML:com.adobe.xmp";
    let mut chunk_data: Vec<u8> = Vec::new();
    chunk_data.extend_from_slice(KEYWORD);
    chunk_data.push(0x00); // null separator
    chunk_data.push(0x00); // compression flag: none
    chunk_data.push(0x00); // compression method: none
    chunk_data.push(0x00); // language tag: empty
    chunk_data.push(0x00); // translated keyword: empty
    chunk_data.extend_from_slice(xmp_bytes);

    let length = chunk_data.len() as u32;
    let chunk_type = b"iTXt";

    let mut crc_input = Vec::with_capacity(4 + chunk_data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(&chunk_data);
    let crc = crc32_ieee(&crc_input);

    let mut chunk = Vec::with_capacity(12 + chunk_data.len());
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(chunk_type);
    chunk.extend_from_slice(&chunk_data);
    chunk.extend_from_slice(&crc.to_be_bytes());

    if let Some(idat_pos) = find_png_chunk(image_bytes, b"IDAT") {
        image_bytes.splice(idat_pos..idat_pos, chunk);
    } else {
        log::warn!("Could not find PNG IDAT chunk; skipping XMP injection");
    }
}

/// Injects XMP as a WebP XMP  chunk in the RIFF container.
fn inject_xmp_webp(image_bytes: &mut Vec<u8>, xmp_bytes: &[u8]) {
    let padded_len = xmp_bytes.len().next_multiple_of(2);
    let mut chunk: Vec<u8> = Vec::with_capacity(8 + padded_len);
    chunk.extend_from_slice(b"XMP ");
    chunk.extend_from_slice(&(xmp_bytes.len() as u32).to_le_bytes());
    chunk.extend_from_slice(xmp_bytes);
    if !xmp_bytes.len().is_multiple_of(2) {
        chunk.push(0x00);
    }

    if image_bytes.len() >= 30 && &image_bytes[0..4] == b"RIFF" && &image_bytes[8..12] == b"WEBP" {
        // An XMP chunk is only valid in the extended format, with the VP8X XMP flag set.
        if &image_bytes[12..16] != b"VP8X" && !convert_webp_to_extended(image_bytes) {
            log::warn!("Could not convert WebP to the extended format; skipping XMP injection");
            return;
        }
        image_bytes[20] |= 0x04;
        let current_riff_size =
            u32::from_le_bytes(image_bytes[4..8].try_into().unwrap_or([0u8; 4]));
        let new_riff_size = current_riff_size.saturating_add(chunk.len() as u32);
        image_bytes[4..8].copy_from_slice(&new_riff_size.to_le_bytes());
        image_bytes.extend_from_slice(&chunk);
    } else {
        log::warn!("WebP bytes do not have valid RIFF/WEBP header; skipping XMP injection");
    }
}

/// Turns a simple-format WebP (a lone VP8 or VP8L chunk, plus any trailing
/// chunks) into the extended format by inserting a VP8X header chunk.
fn convert_webp_to_extended(image_bytes: &mut Vec<u8>) -> bool {
    let payload = &image_bytes[20..];
    let (width, height, has_alpha) = match &image_bytes[12..16] {
        b"VP8 " if payload[3..6] == [0x9D, 0x01, 0x2A] => (
            (u16::from_le_bytes([payload[6], payload[7]]) & 0x3FFF) as u32,
            (u16::from_le_bytes([payload[8], payload[9]]) & 0x3FFF) as u32,
            false,
        ),
        b"VP8L" if payload[0] == 0x2F => {
            let bits = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]);
            (
                (bits & 0x3FFF) + 1,
                ((bits >> 14) & 0x3FFF) + 1,
                (bits >> 28) & 1 == 1,
            )
        }
        _ => return false,
    };
    if width == 0 || height == 0 {
        return false;
    }

    let mut flags = if has_alpha { 0x10u8 } else { 0 };
    let mut pos = 12usize;
    while pos + 8 <= image_bytes.len() {
        let size = u32::from_le_bytes(image_bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if &image_bytes[pos..pos + 4] == b"EXIF" {
            flags |= 0x08;
        }
        pos += 8 + size + (size % 2);
    }

    let mut chunk = Vec::with_capacity(18);
    chunk.extend_from_slice(b"VP8X");
    chunk.extend_from_slice(&10u32.to_le_bytes());
    chunk.extend_from_slice(&[flags, 0, 0, 0]);
    chunk.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    chunk.extend_from_slice(&(height - 1).to_le_bytes()[..3]);

    let riff_size = u32::from_le_bytes(image_bytes[4..8].try_into().unwrap());
    image_bytes[4..8].copy_from_slice(&(riff_size + chunk.len() as u32).to_le_bytes());
    image_bytes.splice(12..12, chunk);
    true
}

/// Finds the byte offset of the first chunk of the given type in PNG bytes.
fn find_png_chunk(bytes: &[u8], wanted: &[u8; 4]) -> Option<usize> {
    let mut pos = 8usize;
    while pos + 8 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let chunk_type = &bytes[pos + 4..pos + 8];
        if chunk_type == wanted {
            return Some(pos);
        }
        pos += 8 + length + 4;
    }
    None
}

/// CRC32 using the IEEE polynomial, as required by PNG.
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[index] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

/// Precomputed CRC32 IEEE lookup table.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

#[cfg(test)]
mod xmp_keyword_tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbImage};

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn encode(format: &str) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 8, image::Rgb([200, 80, 40])));
        if format == "webp" {
            return webp::Encoder::from_image(&image)
                .unwrap()
                .encode(90.0)
                .to_vec();
        }
        let image_format = if format == "png" {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        let mut cursor = Cursor::new(Vec::new());
        image.write_to(&mut cursor, image_format).unwrap();
        cursor.into_inner()
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn export_with_keywords(format: &str) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        fs::write(&source, encode("png")).unwrap();
        let mut bytes = encode(format);
        let keywords = tags(&["Beach", "Tom & Jerry"]);
        write_image_with_metadata(
            &mut bytes,
            source.to_str().unwrap(),
            format,
            true,
            false,
            Some(&keywords),
        )
        .unwrap();
        bytes
    }

    #[test]
    fn keywords_drop_color_labels_and_user_prefix() {
        let keywords = export_keywords(&tags(&[
            "color:red",
            "user:Beach",
            "Beach",
            "sunset",
            "user: ",
        ]));
        assert_eq!(keywords, tags(&["Beach", "sunset"]));
    }

    #[test]
    fn packet_escapes_xml() {
        let packet = build_xmp_keywords_packet(&tags(&["Tom & Jerry", "<b>"]));
        assert!(packet.contains("<rdf:li>Tom &amp; Jerry</rdf:li>"));
        assert!(packet.contains("<rdf:li>&lt;b&gt;</rdf:li>"));
    }

    #[test]
    fn jpeg_export_carries_keywords_after_jfif_and_exif() {
        let bytes = export_with_keywords("jpg");
        let xmp_pos = find(&bytes, b"http://ns.adobe.com/xap/1.0/\0").unwrap();
        if let Some(exif_pos) = find(&bytes, b"Exif\0\0") {
            assert!(exif_pos < xmp_pos);
        }
        assert_eq!(jpeg_xmp_insert_pos(&bytes), xmp_pos - 4);
        assert!(find(&bytes, b"<rdf:li>Tom &amp; Jerry</rdf:li>").is_some());
        image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).unwrap();
    }

    #[test]
    fn png_export_carries_keywords() {
        let bytes = export_with_keywords("png");
        let xmp_pos = find(&bytes, b"iTXtXML:com.adobe.xmp").unwrap();
        assert!(xmp_pos < find_png_chunk(&bytes, b"IDAT").unwrap());
        assert!(find(&bytes, b"<rdf:li>Beach</rdf:li>").is_some());
        // The png decoder verifies chunk CRCs.
        image::load_from_memory_with_format(&bytes, ImageFormat::Png).unwrap();
    }

    #[test]
    fn webp_export_carries_keywords_with_vp8x_flag() {
        let mut simple = encode("webp");
        assert_eq!(&simple[12..16], b"VP8 ");
        inject_xmp_keywords(&mut simple, "webp", &tags(&["Beach"]));
        assert_eq!(&simple[12..16], b"VP8X");
        assert_eq!(&simple[24..30], &[15, 0, 0, 7, 0, 0]);

        let bytes = export_with_keywords("webp");
        assert_eq!(&bytes[12..16], b"VP8X");
        assert_ne!(bytes[20] & 0x04, 0);
        let riff_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_size + 8, bytes.len());
        assert!(find(&bytes, b"XMP ").is_some());
        image::load_from_memory_with_format(&bytes, ImageFormat::WebP).unwrap();
    }
}

#[cfg(test)]
mod card_mode_tests {
    use super::*;
    use crate::file_management::card_mode_test_support::{CardMode, folders, snapshot};

    fn bloated_sidecar() -> String {
        format!(
            r#"{{"version":1,"rating":0,"adjustments":{{}},"exif":{{"MakerNote":"{}"}}}}"#,
            "x".repeat(2000)
        )
    }

    #[test]
    fn sidecar_auto_heal_refuses_the_card() {
        let f = folders();
        let card_sidecar = f.dcim.join("IMG_0001.jpg.rrdata");
        let library_sidecar = f.library.join("IMG_0001.jpg.rrdata");
        fs::write(&card_sidecar, bloated_sidecar()).unwrap();
        fs::write(&library_sidecar, bloated_sidecar()).unwrap();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            let meta = load_sidecar(&card_sidecar);
            assert!(meta.exif.unwrap()["MakerNote"].len() < 500);
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        load_sidecar(&library_sidecar);
        assert!(fs::read_to_string(&library_sidecar).unwrap().len() < 1000);
    }

    #[test]
    fn exif_sidecar_save_refuses_the_card() {
        let f = folders();
        let metadata = ImageMetadata {
            exif: Some(HashMap::from([("Make".to_string(), "Canon".to_string())])),
            ..Default::default()
        };
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert!(save_primary_metadata(&f.dcim.join("IMG_0001.jpg"), &metadata).is_err());
            assert!(save_primary_metadata(&f.dcim.join("IMG_0002.jpg"), &metadata).is_err());
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        save_primary_metadata(&f.library.join("IMG_0001.jpg"), &metadata).unwrap();
        assert!(
            fs::read_to_string(f.library.join("IMG_0001.jpg.rrdata"))
                .unwrap()
                .contains("Canon")
        );
    }

    #[test]
    fn legacy_rrexif_is_kept_on_the_card() {
        let f = folders();
        let exif = r#"{"Make":"Canon"}"#;
        for folder in [&f.dcim, &f.library] {
            fs::remove_file(folder.join("IMG_0001.jpg.rrdata")).unwrap();
            fs::write(folder.join("IMG_0001.jpg.rrexif"), exif).unwrap();
        }
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            let map = read_rrexif_sidecar(&f.dcim.join("IMG_0001.jpg")).unwrap();
            assert_eq!(map["Make"], "Canon");
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        read_rrexif_sidecar(&f.library.join("IMG_0001.jpg")).unwrap();
        assert!(!f.library.join("IMG_0001.jpg.rrexif").exists());
    }
}

#[cfg(test)]
pub(crate) mod rating_samples {
    pub fn xmp_packet(rating: &str) -> Vec<u8> {
        format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
             <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"XMP Core 5.1.2\">\
             <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
             xmp:Rating=\"{rating}\"/></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
        )
        .into_bytes()
    }

    /// A TIFF with one IFD; values longer than four bytes go after the IFD.
    pub fn tiff(little: bool, entries: &[(u16, u16, u32, Vec<u8>)]) -> Vec<u8> {
        let u16b = |v: u16| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let u32b = |v: u32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let mut out = if little {
            b"II".to_vec()
        } else {
            b"MM".to_vec()
        };
        out.extend(u16b(42));
        out.extend(u32b(8));
        out.extend(u16b(entries.len() as u16));
        let mut data_offset = 8 + 2 + entries.len() * 12 + 4;
        let mut data: Vec<u8> = Vec::new();
        for (tag, kind, count, value) in entries {
            out.extend(u16b(*tag));
            out.extend(u16b(*kind));
            out.extend(u32b(*count));
            if value.len() <= 4 {
                let mut inline = value.clone();
                inline.resize(4, 0);
                out.extend(inline);
            } else {
                out.extend(u32b(data_offset as u32));
                data_offset += value.len();
                data.extend(value);
            }
        }
        out.extend(u32b(0));
        out.extend(data);
        out
    }

    /// Laid out like a Sony ARW: IFD0 with Make, Model, the EXIF Rating tag
    /// and the XMP packet (tag 700).
    pub fn sony_arw(little: bool, xmp_rating: Option<&str>, exif_rating: Option<u16>) -> Vec<u8> {
        let short = |v: u16| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
            .to_vec()
        };
        let mut entries = vec![
            (0x010f, 2, 5, b"SONY\0".to_vec()),
            (0x0110, 2, 10, b"ILCE-7CM2\0".to_vec()),
        ];
        if let Some(rating) = exif_rating {
            entries.push((0x4746, 3, 1, short(rating)));
        }
        if let Some(rating) = xmp_rating {
            let packet = xmp_packet(rating);
            entries.push((0x02bc, 1, packet.len() as u32, packet));
        }
        tiff(little, &entries)
    }

    pub fn jpeg(segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0xff, 0xd8];
        for (marker, payload) in segments {
            out.extend([0xff, *marker]);
            out.extend(((payload.len() + 2) as u16).to_be_bytes());
            out.extend(payload);
        }
        out.extend([0xff, 0xda, 0x00, 0x02, 0xff, 0xd9]);
        out
    }

    pub fn xmp_app1(rating: &str) -> (u8, Vec<u8>) {
        let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        payload.extend(xmp_packet(rating));
        (0xe1, payload)
    }

    pub fn exif_app1(rating: u16) -> (u8, Vec<u8>) {
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend(tiff(
            false,
            &[(0x4746, 3, 1, rating.to_be_bytes().to_vec())],
        ));
        (0xe1, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::rating_samples::*;
    use super::{XMP_UUID, parse_xmp_rating, read_image_rating, xmp_rating};
    use std::io::Cursor;

    fn rating(bytes: &[u8]) -> Option<u8> {
        read_image_rating(&mut Cursor::new(bytes))
    }

    #[test]
    fn xmp_attribute_rating() {
        assert_eq!(
            xmp_rating(b"<rdf:Description xmp:Rating=\"4\"></rdf:Description>"),
            Some(4)
        );
        assert_eq!(xmp_rating(b"<rdf:Description xmp:Rating='2'/>"), Some(2));
        assert_eq!(
            xmp_rating(b"<rdf:Description xap:Rating = \" 1 \"/>"),
            Some(1)
        );
    }

    #[test]
    fn xmp_element_rating() {
        assert_eq!(
            xmp_rating(b"<rdf:Description><xmp:Rating>3</xmp:Rating></rdf:Description>"),
            Some(3)
        );
    }

    #[test]
    fn xmp_rating_mapping() {
        assert_eq!(parse_xmp_rating("-1"), None);
        assert_eq!(parse_xmp_rating("0"), Some(0));
        assert_eq!(parse_xmp_rating("5"), Some(5));
        assert_eq!(parse_xmp_rating("7"), None);
        assert_eq!(parse_xmp_rating("abc"), None);
        assert_eq!(parse_xmp_rating("99999999999"), None);
        assert_eq!(xmp_rating(b"<rdf:RDF></rdf:RDF> no rating present"), None);
    }

    #[test]
    fn sony_arw_xmp_rating_both_byte_orders() {
        for little in [true, false] {
            assert_eq!(rating(&sony_arw(little, Some("3"), None)), Some(3));
            assert_eq!(rating(&sony_arw(little, None, Some(4))), Some(4));
            assert_eq!(rating(&sony_arw(little, Some("5"), Some(1))), Some(5));
            assert_eq!(rating(&sony_arw(little, Some("0"), None)), Some(0));
            assert_eq!(rating(&sony_arw(little, None, None)), None);
        }
    }

    #[test]
    fn rejected_or_out_of_range_xmp_falls_back_to_exif() {
        assert_eq!(rating(&sony_arw(true, Some("-1"), Some(2))), Some(2));
        assert_eq!(rating(&sony_arw(true, Some("9"), None)), None);
        assert_eq!(rating(&sony_arw(true, None, Some(9))), None);
    }

    #[test]
    fn exif_sub_ifd_rating() {
        // IFD0 points to an EXIF IFD at offset 26 that holds the Rating tag.
        let mut bytes = b"II*\0\x08\0\0\0".to_vec();
        bytes.extend(1u16.to_le_bytes());
        bytes.extend([0x69, 0x87, 4, 0, 1, 0, 0, 0, 26, 0, 0, 0]);
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend([0x46, 0x47, 3, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
        bytes.extend(0u32.to_le_bytes());
        assert_eq!(rating(&bytes), Some(2));
    }

    #[test]
    fn jpeg_ratings() {
        assert_eq!(rating(&jpeg(&[xmp_app1("4")])), Some(4));
        assert_eq!(rating(&jpeg(&[exif_app1(2)])), Some(2));
        assert_eq!(rating(&jpeg(&[exif_app1(2), xmp_app1("5")])), Some(5));
        assert_eq!(rating(&jpeg(&[(0xe0, b"JFIF\0".to_vec())])), None);
    }

    #[test]
    fn fuji_raf_embedded_jpeg() {
        let jpeg = jpeg(&[exif_app1(1), xmp_app1("3")]);
        let mut bytes = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
        bytes.resize(84, 0);
        bytes.extend(100u32.to_be_bytes());
        bytes.extend((jpeg.len() as u32).to_be_bytes());
        bytes.resize(100, 0);
        bytes.extend(jpeg);
        assert_eq!(rating(&bytes), Some(3));
    }

    #[test]
    fn canon_cr3_xmp_box() {
        let mut bytes = Vec::new();
        bytes.extend(16u32.to_be_bytes());
        bytes.extend(b"ftypcrx \0\0\0\x01");
        bytes.extend(12u32.to_be_bytes());
        bytes.extend(b"moov\0\0\0\0");
        let packet = xmp_packet("2");
        bytes.extend(((8 + 16 + packet.len()) as u32).to_be_bytes());
        bytes.extend(b"uuid");
        bytes.extend(XMP_UUID);
        bytes.extend(packet);
        assert_eq!(rating(&bytes), Some(2));
    }

    #[test]
    fn malformed_data_returns_none() {
        let le = |v: u32| v.to_le_bytes();
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            b"II*\0".to_vec(),
            vec![0x42; 4096],
            // IFD offset past the end of the file.
            [b"II*\0".as_slice(), &le(0xffff_fff0), &[0; 8]].concat(),
            // Wrong TIFF magic.
            [b"II\x2b\0".as_slice(), &le(8), &[0; 8]].concat(),
            // 65535 entries claimed, none present.
            [b"II*\0".as_slice(), &le(8), &[0xff, 0xff], &[0; 8]].concat(),
            // IFD0 whose next-IFD pointer is itself.
            [
                b"II*\0".as_slice(),
                &le(8),
                &[1, 0, 0x0f, 1, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0],
                &le(8),
            ]
            .concat(),
            // XMP packet claiming 4 GiB, and one pointing past the end.
            tiff(true, &[(0x02bc, 7, u32::MAX, le(64).to_vec())]),
            tiff(true, &[(0x02bc, 7, 64, le(0xffff_0000).to_vec())]),
            // JPEG with a segment length below 2, and a truncated APP1.
            vec![
                0xff, 0xd8, 0xff, 0xe1, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            vec![
                0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff, b'E', b'x', b'i', b'f', 0, 0, 0, 0, 0, 0,
            ],
            // ISO BMFF boxes with size 0, a zero 64-bit size, and an overflowing one.
            [&0u32.to_be_bytes()[..], b"ftypcrx \0\0\0\0"].concat(),
            [
                &1u32.to_be_bytes()[..],
                b"ftyp",
                &0u64.to_be_bytes(),
                b"crx ",
            ]
            .concat(),
            [
                &1u32.to_be_bytes()[..],
                b"ftyp",
                &u64::MAX.to_be_bytes(),
                b"crx ",
            ]
            .concat(),
            // RAF whose JPEG offset is past the end.
            [
                b"FUJIFILMCCD-RAW ".as_slice(),
                &[0; 68],
                &u32::MAX.to_be_bytes(),
                &[0; 8],
            ]
            .concat(),
        ];
        for (i, bytes) in cases.iter().enumerate() {
            assert_eq!(rating(bytes), None, "case {i}");
        }
    }

    #[test]
    fn truncated_and_corrupted_files_do_not_panic() {
        let samples = [
            sony_arw(true, Some("3"), Some(3)),
            sony_arw(false, Some("3"), Some(3)),
            jpeg(&[exif_app1(2), xmp_app1("4")]),
        ];
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for sample in &samples {
            for len in 0..sample.len() {
                let _ = rating(&sample[..len]);
            }
            for _ in 0..2000 {
                let mut bytes = sample.clone();
                for _ in 0..4 {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let at = (seed as usize) % bytes.len();
                    bytes[at] = (seed >> 32) as u8;
                }
                if let Some(r) = rating(&bytes) {
                    assert!(r <= 5);
                }
            }
        }
    }
}
