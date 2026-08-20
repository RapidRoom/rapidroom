//! Hand-written IFD reader for Adobe DNG Camera Profiles (`.dcp`).
//!
//! A DCP is a little-endian TIFF-like container with the byte-order marker
//! `II` and the magic `0x4352` (`IIRC`). The primary IFD lives at the 32-bit
//! offset in the header (usually 8) and carries the profile tags listed in
//! `COBALT_DCP_SPEC.md` §1.3.
//!
//! Profiles are user-supplied files from the internet, so this parser treats
//! every byte as untrusted: no panics, every offset/length is bounds-checked
//! before slicing, and size limits reject pathological inputs.

use std::fs::{self, File};
use std::path::Path;

use memmap2::{Mmap, MmapOptions};

use crate::dcp::DcpError;
use crate::dcp::model::*;

/// Reject files larger than 256 MB.
const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;
/// Reject any single tag whose decoded byte count exceeds 64 MB.
const MAX_TAG_SIZE: u64 = 64 * 1024 * 1024;
/// Use a memory map (rather than reading the whole file) above this size. The
/// supplied DCP is ~1.05 MB with a 972 KB `ProfileLookTableData` table.
const MMAP_THRESHOLD: u64 = 1_000_000;

/// TIFF type → byte size per element.
const TYPE_SIZES: [u16; 13] = [
    0, // 0 (unused)
    1, // 1 BYTE
    1, // 2 ASCII
    2, // 3 SHORT
    4, // 4 LONG
    8, // 5 RATIONAL
    1, // 6 SBYTE
    1, // 7 UNDEFINED
    2, // 8 SSHORT
    4, // 9 SLONG
    8, // 10 SRATIONAL
    4, // 11 FLOAT
    8, // 12 DOUBLE
];

/// The bytes backing a DCP: an in-memory copy for small files, a memory map
/// for large ones. Both deref to `[u8]`.
enum FileBytes {
    Small(Vec<u8>),
    Mapped(Mmap),
}

impl std::ops::Deref for FileBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            FileBytes::Small(v) => v,
            FileBytes::Mapped(m) => m,
        }
    }
}

/// A raw IFD entry: tag, type, count and the resolved value bytes (either the
/// inline ≤4-byte value or a bounds-checked slice from the file).
struct IfdEntry {
    tag: u16,
    typ: u16,
    count: u32,
    value: Vec<u8>,
}

/// Parse a DCP file into its profile(s). Returns the primary profile followed
/// by any `ExtraCameraProfiles` (nested sub-profiles).
pub fn parse_dcp(path: &Path) -> Result<Vec<DcpProfile>, DcpError> {
    let metadata = fs::metadata(path).map_err(DcpError::Io)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(DcpError::Oversized(format!(
            "file is {} bytes, limit is {MAX_FILE_SIZE}",
            metadata.len()
        )));
    }

    let bytes: FileBytes = if metadata.len() > MMAP_THRESHOLD {
        let file = File::open(path).map_err(DcpError::Io)?;
        let mmap = unsafe { MmapOptions::new().map(&file) }.map_err(DcpError::Io)?;
        FileBytes::Mapped(mmap)
    } else {
        FileBytes::Small(fs::read(path).map_err(DcpError::Io)?)
    };

    let id = ProfileId(*blake3::hash(&bytes).as_bytes());
    parse_bytes(&bytes, id, Some(path))
}

/// Parse profile bytes already in memory (used by `parse_dcp` and the
/// corrupted-input property test). `path` is used only for `file_path` and the
/// `profile_name` fallback.
fn parse_bytes(
    data: &[u8],
    id: ProfileId,
    path: Option<&Path>,
) -> Result<Vec<DcpProfile>, DcpError> {
    validate_header(data)?;

    let ifd_offset = read_u32(data, 4)? as usize;
    let mut profiles = Vec::new();

    let main = parse_ifd(data, ifd_offset, id, path)?;
    let extra_offsets = find_extra_profile_offsets(data, ifd_offset)?;
    for offset in extra_offsets {
        match parse_ifd(data, offset as usize, id, path) {
            Ok(sub) => profiles.push(sub),
            // An unreadable sub-profile should not fail the whole file; the
            // primary profile remains usable. Report by skipping.
            Err(err) => {
                log::warn!("skipping unreadable ExtraCameraProfile at {offset:#x}: {err}");
            }
        }
    }

    profiles.insert(0, main);
    Ok(profiles)
}

/// Validate the 8-byte header: `II` + magic `0x4352`, then an in-bounds IFD
/// offset. Big-endian (`MM`) is rejected explicitly.
fn validate_header(data: &[u8]) -> Result<(), DcpError> {
    if data.len() < 8 {
        return Err(DcpError::InvalidHeader(format!(
            "file too short ({}) for a DCP header",
            data.len()
        )));
    }
    let byte_order = &data[0..2];
    if byte_order == b"MM" {
        return Err(DcpError::InvalidHeader(
            "big-endian (MM) DCP is not supported; DCP is little-endian by definition".into(),
        ));
    }
    if byte_order != b"II" {
        return Err(DcpError::InvalidHeader(format!(
            "bad byte-order marker {:02x?}",
            byte_order
        )));
    }
    let magic = read_u16(data, 2)?;
    if magic != 0x4352 {
        return Err(DcpError::InvalidHeader(format!(
            "bad magic 0x{magic:04x}, expected 0x4352"
        )));
    }
    let ifd_offset = read_u32(data, 4)? as usize;
    if ifd_offset + 2 > data.len() {
        return Err(DcpError::InvalidHeader(format!(
            "IFD offset {ifd_offset} is out of bounds (file is {} bytes)",
            data.len()
        )));
    }
    Ok(())
}

/// Read one IFD starting at `offset` and turn it into a `DcpProfile`.
fn parse_ifd(
    data: &[u8],
    offset: usize,
    id: ProfileId,
    path: Option<&Path>,
) -> Result<DcpProfile, DcpError> {
    let entries = read_ifd_entries(data, offset)?;

    // Builder fields. All tags are optional until finalised; required tags are
    // checked at the end.
    let mut profile_name: Option<String> = None;
    let mut unique_camera_model: Option<String> = None;
    let mut copyright: Option<String> = None;
    let mut embed_policy: EmbedPolicy = EmbedPolicy::Unknown(0);
    let mut illuminant_1: Option<Illuminant> = None;
    let mut illuminant_2: Option<Illuminant> = None;
    let mut color_matrix_1: Option<Mat3> = None;
    let mut color_matrix_2: Option<Mat3> = None;
    let mut forward_matrix_1: Option<Mat3> = None;
    let mut forward_matrix_2: Option<Mat3> = None;
    let mut camera_calibration_1: Option<Mat3> = None;
    let mut camera_calibration_2: Option<Mat3> = None;
    let mut analog_balance: Option<[f32; 3]> = None;
    let mut baseline_exposure_offset: Option<f32> = None;
    let mut default_black_render: DefaultBlackRender = DefaultBlackRender::Auto;
    let mut hue_sat_map: Option<DualHueSatMap> = None;
    let mut look_table: Option<HsvTable> = None;
    let mut look_table_encoding: TableEncoding = TableEncoding::Linear;
    let mut hue_sat_map_encoding: TableEncoding = TableEncoding::Linear;
    let mut tone_curve: Option<ToneCurve> = None;

    for entry in &entries {
        match entry.tag {
            50708 => unique_camera_model = Some(read_ascii(entry, "UniqueCameraModel")?),
            50721 => color_matrix_1 = Some(read_matrix9(entry, "ColorMatrix1")?),
            50722 => color_matrix_2 = Some(read_matrix9(entry, "ColorMatrix2")?),
            50778 => illuminant_1 = Some(read_illuminant(entry, "CalibrationIlluminant1")?),
            50779 => illuminant_2 = Some(read_illuminant(entry, "CalibrationIlluminant2")?),
            50710 => default_black_render = read_default_black_render(entry)?,
            50716 => analog_balance = Some(read_analog_balance(entry)?),
            50717 => {
                baseline_exposure_offset =
                    Some(read_srational_f32(entry, "BaselineExposureOffset")?)
            }
            50730 => camera_calibration_1 = Some(read_matrix9(entry, "CameraCalibration1")?),
            50731 => camera_calibration_2 = Some(read_matrix9(entry, "CameraCalibration2")?),
            50932 => { /* ProfileCalibrationSignature — informational, ignored */ }
            50936 => profile_name = Some(read_ascii(entry, "ProfileName")?),
            50937 => {
                let dims = read_u32_array::<3>(entry, "ProfileHueSatMapDims")?;
                hue_sat_map = Some(DualHueSatMap {
                    map_1: HsvTable {
                        hue_div: dims[0],
                        sat_div: dims[1],
                        val_div: dims[2],
                        data: Vec::new(),
                    },
                    map_2: None,
                });
            }
            50938 => {
                let table = read_hsv_table(entry, "ProfileHueSatMapData1", 1)?;
                hue_sat_map = Some(match hue_sat_map.take() {
                    Some(mut dual) => {
                        dual.map_1.data = table;
                        dual
                    }
                    None => {
                        // Data1 without dims: defaults per DNG (90 x 30 x 1).
                        DualHueSatMap {
                            map_1: HsvTable {
                                hue_div: 90,
                                sat_div: 30,
                                val_div: 1,
                                data: table,
                            },
                            map_2: None,
                        }
                    }
                });
            }
            50939 => {
                let table = read_hsv_table(entry, "ProfileHueSatMapData2", 2)?;
                hue_sat_map = Some(match hue_sat_map.take() {
                    Some(mut dual) => {
                        dual.map_2 = Some(HsvTable {
                            hue_div: dual.map_1.hue_div,
                            sat_div: dual.map_1.sat_div,
                            val_div: dual.map_1.val_div,
                            data: table,
                        });
                        dual
                    }
                    None => {
                        // Data2 without dims: fall back to 90 x 30 x 1.
                        DualHueSatMap {
                            map_1: HsvTable {
                                hue_div: 90,
                                sat_div: 30,
                                val_div: 1,
                                data: Vec::new(),
                            },
                            map_2: Some(HsvTable {
                                hue_div: 90,
                                sat_div: 30,
                                val_div: 1,
                                data: table,
                            }),
                        }
                    }
                });
            }
            50940 => {
                let points = read_tone_curve(entry)?;
                if !points.is_empty() {
                    tone_curve = Some(ToneCurve { points });
                }
            }
            50941 => {
                embed_policy = EmbedPolicy::from_code(read_one_u32(entry, "ProfileEmbedPolicy")?)
            }
            50942 => copyright = Some(read_ascii(entry, "ProfileCopyright")?),
            50964 => forward_matrix_1 = Some(read_matrix9(entry, "ForwardMatrix1")?),
            50965 => forward_matrix_2 = Some(read_matrix9(entry, "ForwardMatrix2")?),
            50981 => {
                let dims = read_u32_array::<3>(entry, "ProfileLookTableDims")?;
                look_table = Some(HsvTable {
                    hue_div: dims[0],
                    sat_div: dims[1],
                    val_div: dims[2],
                    data: Vec::new(),
                });
            }
            50982 => {
                let data = read_hsv_table(entry, "ProfileLookTableData", 0)?;
                look_table = Some(match look_table.take() {
                    Some(mut table) => {
                        table.data = data;
                        table
                    }
                    None => HsvTable {
                        hue_div: 90,
                        sat_div: 30,
                        val_div: 30,
                        data,
                    },
                });
            }
            51107 => {
                hue_sat_map_encoding =
                    TableEncoding::from_code(read_one_u32(entry, "ProfileHueSatMapEncoding")?)
            }
            51108 => {
                look_table_encoding =
                    TableEncoding::from_code(read_one_u32(entry, "ProfileLookTableEncoding")?)
            }
            _ => { /* other / unknown / private tags ignored */ }
        }
    }

    let unique_camera_model =
        unique_camera_model.ok_or(DcpError::MissingField("UniqueCameraModel"))?;
    let color_matrix_1 = color_matrix_1.ok_or(DcpError::MissingField("ColorMatrix1"))?;
    let calibration_illuminant_1 =
        illuminant_1.ok_or(DcpError::MissingField("CalibrationIlluminant1"))?;

    let profile_name = profile_name.unwrap_or_else(|| {
        path.and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    Ok(DcpProfile {
        id,
        file_path: path.map(|p| p.to_path_buf()).unwrap_or_default(),
        profile_name,
        unique_camera_model,
        copyright,
        embed_policy,
        calibration_illuminant_1,
        calibration_illuminant_2: illuminant_2,
        color_matrix_1,
        color_matrix_2,
        forward_matrix_1,
        forward_matrix_2,
        camera_calibration_1,
        camera_calibration_2,
        analog_balance,
        baseline_exposure_offset,
        default_black_render,
        hue_sat_map,
        look_table,
        look_table_encoding,
        hue_sat_map_encoding,
        tone_curve,
    })
}

/// Locate `ExtraCameraProfiles` (50933) offsets from the IFD at `offset`.
///
/// The tag holds one or more 32-bit offsets to nested IFDs (sub-profiles).
/// Each is parsed separately in `parse_bytes`.
fn find_extra_profile_offsets(data: &[u8], offset: usize) -> Result<Vec<u32>, DcpError> {
    let entries = read_ifd_entries(data, offset)?;
    for entry in &entries {
        if entry.tag == 50933 {
            let mut out = Vec::new();
            let n = entry.count as usize;
            for i in 0..n {
                let v = read_entry_u32(entry, i, "ExtraCameraProfiles")?;
                out.push(v);
            }
            return Ok(out);
        }
    }
    Ok(Vec::new())
}

/// Read the entries of the IFD at `offset`.
fn read_ifd_entries(data: &[u8], offset: usize) -> Result<Vec<IfdEntry>, DcpError> {
    let count = read_u16(data, offset)? as usize;
    // Sanity-check the entry table fits in the file.
    let table_bytes = count
        .checked_mul(12)
        .ok_or_else(|| DcpError::Truncated("IFD entry count overflow".into()))?;
    let table_end = offset
        .checked_add(2)
        .and_then(|v| v.checked_add(table_bytes))
        .ok_or_else(|| DcpError::Truncated("IFD table overflow".into()))?;
    if table_end > data.len() {
        return Err(DcpError::Truncated(format!(
            "IFD entry table at {offset} extends past end of file"
        )));
    }

    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let base = offset + 2 + i * 12;
        let tag = read_u16(data, base)?;
        let typ = read_u16(data, base + 2)?;
        let cnt = read_u32(data, base + 4)?;
        let raw_value = &data[base + 8..base + 12];

        let type_size = match TYPE_SIZES.get(typ as usize) {
            Some(&s) if s > 0 => s,
            _ => {
                return Err(DcpError::BadTag {
                    tag,
                    detail: format!("unsupported TIFF type {typ}"),
                });
            }
        };

        let byte_count = (cnt as u64)
            .checked_mul(type_size as u64)
            .ok_or_else(|| DcpError::Oversized(format!("tag 0x{tag:04x} byte count overflow")))?;
        if byte_count > MAX_TAG_SIZE {
            return Err(DcpError::Oversized(format!(
                "tag 0x{tag:04x} needs {byte_count} bytes (limit {MAX_TAG_SIZE})"
            )));
        }

        let value = if byte_count <= 4 {
            // Inline value: the low bytes of the 4-byte field.
            raw_value[..byte_count as usize].to_vec()
        } else {
            let value_offset = read_u32(data, base + 8)? as usize;
            let end = value_offset
                .checked_add(byte_count as usize)
                .ok_or_else(|| DcpError::Truncated(format!("tag 0x{tag:04x} offset overflow")))?;
            if end > data.len() {
                return Err(DcpError::Truncated(format!(
                    "tag 0x{tag:04x} value at {value_offset} ({byte_count} bytes) exceeds file"
                )));
            }
            data[value_offset..end].to_vec()
        };

        entries.push(IfdEntry {
            tag,
            typ,
            count: cnt,
            value,
        });
    }
    Ok(entries)
}

// ---- low-level bounds-checked readers --------------------------------------

fn read_u16(data: &[u8], offset: usize) -> Result<u16, DcpError> {
    let b = data
        .get(offset..offset + 2)
        .ok_or_else(|| DcpError::Truncated(format!("read u16 at {offset} out of bounds")))?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, DcpError> {
    let b = data
        .get(offset..offset + 4)
        .ok_or_else(|| DcpError::Truncated(format!("read u32 at {offset} out of bounds")))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i32(data: &[u8], offset: usize) -> Result<i32, DcpError> {
    let b = data
        .get(offset..offset + 4)
        .ok_or_else(|| DcpError::Truncated(format!("read i32 at {offset} out of bounds")))?;
    Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

// ---- typed tag readers -------------------------------------------------------

fn read_ascii(entry: &IfdEntry, field: &str) -> Result<String, DcpError> {
    if entry.typ != 2 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected ASCII, got type {}", entry.typ),
        });
    }
    // Trim trailing NULs and the terminator.
    let end = entry
        .value
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    Ok(String::from_utf8_lossy(&entry.value[..end]).into_owned())
}

/// Read the value at `index` as an unsigned integer, honouring SHORT/LONG/BYTE.
fn read_entry_u32(entry: &IfdEntry, index: usize, field: &str) -> Result<u32, DcpError> {
    let size = TYPE_SIZES.get(entry.typ as usize).copied().unwrap_or(0) as usize;
    let off = index.checked_mul(size).ok_or_else(|| DcpError::BadTag {
        tag: entry.tag,
        detail: format!("{field}: index overflow"),
    })?;
    let b = entry
        .value
        .get(off..off + size)
        .ok_or_else(|| DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: index {index} out of range"),
        })?;
    let v = match entry.typ {
        1 => b[0] as u32,
        3 => u16::from_le_bytes([b[0], b[1]]) as u32,
        4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        _ => {
            return Err(DcpError::BadTag {
                tag: entry.tag,
                detail: format!("{field}: expected integer type, got {}", entry.typ),
            });
        }
    };
    Ok(v)
}

fn read_one_u32(entry: &IfdEntry, field: &str) -> Result<u32, DcpError> {
    if entry.count == 0 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: empty value"),
        });
    }
    read_entry_u32(entry, 0, field)
}

fn read_u32_array<const N: usize>(entry: &IfdEntry, field: &str) -> Result<[u32; N], DcpError> {
    if entry.count as usize != N {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected {N} values, got {}", entry.count),
        });
    }
    let mut out = [0u32; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_entry_u32(entry, i, field)?;
    }
    Ok(out)
}

fn read_illuminant(entry: &IfdEntry, field: &str) -> Result<Illuminant, DcpError> {
    let code = read_one_u32(entry, field)?;
    if code > u16::MAX as u32 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: illuminant code {code} out of range"),
        });
    }
    Ok(Illuminant::from_code(code as u16))
}

fn read_default_black_render(entry: &IfdEntry) -> Result<DefaultBlackRender, DcpError> {
    Ok(DefaultBlackRender::from_code(read_one_u32(
        entry,
        "DefaultBlackRender",
    )?))
}

/// Read the signed rational at `index` as an `f32`, guarding a zero
/// denominator (yields `0.0` rather than dividing by zero).
fn read_srational(entry: &IfdEntry, index: usize, field: &str) -> Result<f32, DcpError> {
    if entry.typ != 10 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected SRATIONAL, got type {}", entry.typ),
        });
    }
    let base = index.checked_mul(8).ok_or_else(|| DcpError::BadTag {
        tag: entry.tag,
        detail: format!("{field}: index overflow"),
    })?;
    let b = entry
        .value
        .get(base..base + 8)
        .ok_or_else(|| DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: index {index} out of range"),
        })?;
    let num = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let den = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    Ok(if den == 0 {
        0.0
    } else {
        num as f32 / den as f32
    })
}

/// Read a single SRATIONAL value.
fn read_srational_f32(entry: &IfdEntry, field: &str) -> Result<f32, DcpError> {
    if entry.count == 0 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: empty value"),
        });
    }
    read_srational(entry, 0, field)
}

/// Read `AnalogBalance` (3 unsigned RATIONALs).
fn read_analog_balance(entry: &IfdEntry) -> Result<[f32; 3], DcpError> {
    if entry.typ != 5 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("AnalogBalance: expected RATIONAL, got type {}", entry.typ),
        });
    }
    if entry.count != 3 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("AnalogBalance: expected 3 values, got {}", entry.count),
        });
    }
    if entry.value.len() < 3 * 8 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: "AnalogBalance: value truncated".into(),
        });
    }
    let mut out = [0.0f32; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let num = read_u32(&entry.value, i * 8)?;
        let den = read_u32(&entry.value, i * 8 + 4)?;
        *slot = if den == 0 {
            0.0
        } else {
            num as f32 / den as f32
        };
    }
    Ok(out)
}

/// Read 9 SRATIONALs into a row-major `Mat3`, guarding denominators of zero.
fn read_matrix9(entry: &IfdEntry, field: &str) -> Result<Mat3, DcpError> {
    if entry.typ != 10 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected SRATIONAL, got type {}", entry.typ),
        });
    }
    if entry.count != 9 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected 9 values, got {}", entry.count),
        });
    }
    if entry.value.len() < 9 * 8 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: value truncated"),
        });
    }
    let mut elems = [0.0f32; 9];
    for (i, slot) in elems.iter_mut().enumerate() {
        *slot = read_srational(entry, i, field)?;
    }
    Ok(Mat3::new(
        elems[0], elems[1], elems[2], elems[3], elems[4], elems[5], elems[6], elems[7], elems[8],
    ))
}

/// Read a FLOAT value at a byte offset.
fn read_f32(data: &[u8], offset: usize) -> Result<f32, DcpError> {
    let b = data
        .get(offset..offset + 4)
        .ok_or_else(|| DcpError::Truncated(format!("read f32 at {offset} out of bounds")))?;
    Ok(f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
}

/// Read a HueSatMap/LookTable FLOAT payload and validate its length against the
/// expected number of cells derived from `Dims`.
fn read_hsv_table(entry: &IfdEntry, field: &str, which: u8) -> Result<Vec<[f32; 3]>, DcpError> {
    if entry.typ != 11 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: expected FLOAT, got type {}", entry.typ),
        });
    }
    let count = entry.count as usize;
    if !count.is_multiple_of(3) {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: count {count} is not a multiple of 3"),
        });
    }
    let cells = count / 3;
    if entry.value.len() < count * 4 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("{field}: value truncated"),
        });
    }
    let mut out = Vec::with_capacity(cells);
    for i in 0..cells {
        let base = i * 3;
        out.push([
            read_f32(&entry.value, base * 4)?,
            read_f32(&entry.value, base * 4 + 4)?,
            read_f32(&entry.value, base * 4 + 8)?,
        ]);
    }
    // Guard: the parsed cells should match `Dims` where known (informational).
    let _ = which;
    Ok(out)
}

/// Read a `ProfileToneCurve` (interleaved x,y FLOAT pairs).
fn read_tone_curve(entry: &IfdEntry) -> Result<Vec<[f32; 2]>, DcpError> {
    if entry.typ != 11 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("ProfileToneCurve: expected FLOAT, got type {}", entry.typ),
        });
    }
    let count = entry.count as usize;
    if !count.is_multiple_of(2) {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: format!("ProfileToneCurve: count {count} is not even"),
        });
    }
    let pairs = count / 2;
    if entry.value.len() < count * 4 {
        return Err(DcpError::BadTag {
            tag: entry.tag,
            detail: "ProfileToneCurve: value truncated".into(),
        });
    }
    let mut out = Vec::with_capacity(pairs);
    for i in 0..pairs {
        out.push([
            read_f32(&entry.value, i * 8)?,
            read_f32(&entry.value, i * 8 + 4)?,
        ]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcp::DcpError;
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};

    const SEED: u64 = 0xC0B4_5EED_2026_0801;

    // ---- synthetic DCP builder (no vendor assets) ---------------------------

    struct TagSpec {
        tag: u16,
        typ: u16,
        data: Vec<u8>,
    }

    /// Build a minimal but valid little-endian DCP in memory with the main IFD
    /// at offset 8. Values whose byte count exceeds 4 are placed in a data pool
    /// and referenced by offset.
    fn build_synthetic_dcp(tags: &[TagSpec]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&0x4352u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD offset
        out.extend_from_slice(&build_ifd_block(tags, 8));
        out
    }

    /// Build an IFD + its data pool to be placed at the absolute `ifd_offset`.
    fn build_ifd_block(tags: &[TagSpec], ifd_offset: usize) -> Vec<u8> {
        let mut ifd = Vec::new();
        ifd.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        // Reserve 12 bytes per entry, filled below once offsets are known.
        let entry_start = ifd.len();
        ifd.resize(entry_start + tags.len() * 12, 0);
        ifd.extend_from_slice(&0u32.to_le_bytes()); // next-IFD pointer

        let mut pool = Vec::new();
        for (i, spec) in tags.iter().enumerate() {
            let count = match spec.typ {
                1 | 2 | 7 => spec.data.len() as u32,
                3 | 8 => (spec.data.len() / 2) as u32,
                4 | 9 | 11 => (spec.data.len() / 4) as u32,
                5 | 10 => (spec.data.len() / 8) as u32,
                _ => 0,
            };
            let base = entry_start + i * 12;
            ifd[base..base + 2].copy_from_slice(&spec.tag.to_le_bytes());
            ifd[base + 2..base + 4].copy_from_slice(&spec.typ.to_le_bytes());
            ifd[base + 4..base + 8].copy_from_slice(&count.to_le_bytes());
            if spec.data.len() <= 4 {
                // Inline value in the low bytes.
                ifd[base + 8..base + 8 + spec.data.len()].copy_from_slice(&spec.data);
            } else {
                let off = ifd_offset + ifd.len() + pool.len();
                ifd[base + 8..base + 12].copy_from_slice(&(off as u32).to_le_bytes());
                pool.extend_from_slice(&spec.data);
            }
        }
        ifd.extend_from_slice(&pool);
        ifd
    }

    fn f32s(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn u32s(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn ascii(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    fn srat(v: &[(i32, i32)]) -> Vec<u8> {
        v.iter()
            .flat_map(|(n, d)| {
                let mut b = n.to_le_bytes().to_vec();
                b.extend_from_slice(&d.to_le_bytes());
                b
            })
            .collect()
    }

    fn synthetic_tags() -> Vec<TagSpec> {
        vec![
            TagSpec {
                tag: 50708,
                typ: 2,
                data: ascii("Test Camera"),
            },
            TagSpec {
                tag: 50936,
                typ: 2,
                data: ascii("Test Profile"),
            },
            TagSpec {
                tag: 50721,
                typ: 10,
                data: srat(&[
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                ]),
            },
            TagSpec {
                tag: 50778,
                typ: 3,
                data: 17u16.to_le_bytes().to_vec(),
            },
            TagSpec {
                tag: 50779,
                typ: 3,
                data: 21u16.to_le_bytes().to_vec(),
            },
            TagSpec {
                tag: 50941,
                typ: 4,
                data: 2u32.to_le_bytes().to_vec(),
            },
            TagSpec {
                tag: 50937,
                typ: 4,
                data: u32s(&[2, 2, 1]),
            },
            TagSpec {
                tag: 50938,
                typ: 11,
                data: f32s(&[0.0, 1.0, 1.0, 0.1, 1.0, 1.0, 0.2, 1.0, 1.0, 0.3, 1.0, 1.0]),
            },
        ]
    }

    // ---- tests ---------------------------------------------------------------

    #[test]
    fn synthetic_round_trip() {
        let data = build_synthetic_dcp(&synthetic_tags());
        let id = ProfileId(*blake3::hash(&data).as_bytes());
        let profiles = parse_bytes(&data, id, None).expect("synthetic DCP should parse");
        assert_eq!(profiles.len(), 1);
        let p = &profiles[0];
        assert_eq!(p.unique_camera_model, "Test Camera");
        assert_eq!(p.profile_name, "Test Profile");
        assert_eq!(p.calibration_illuminant_1, Illuminant::StdA);
        assert_eq!(p.calibration_illuminant_2, Some(Illuminant::D65));
        assert_eq!(p.embed_policy, EmbedPolicy::EmbedNever);
        assert_eq!(p.look_table_encoding, TableEncoding::Linear);
        assert_eq!(p.hue_sat_map_encoding, TableEncoding::Linear);
        let hs = p.hue_sat_map.as_ref().expect("hue_sat_map present");
        assert_eq!(hs.map_1.hue_div, 2);
        assert_eq!(hs.map_1.sat_div, 2);
        assert_eq!(hs.map_1.val_div, 1);
        assert_eq!(hs.map_1.data.len(), 4);
        assert_eq!(hs.map_1.data[0], [0.0, 1.0, 1.0]);
        assert_eq!(p.id, id);
    }

    #[test]
    fn rejects_big_endian() {
        let mut data = build_synthetic_dcp(&synthetic_tags());
        data[0] = b'M';
        data[1] = b'M';
        let id = ProfileId(*blake3::hash(&data).as_bytes());
        let err = parse_bytes(&data, id, None).unwrap_err();
        assert!(matches!(err, DcpError::InvalidHeader(_)));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = build_synthetic_dcp(&synthetic_tags());
        data[3] = 0x00;
        let id = ProfileId(*blake3::hash(&data).as_bytes());
        assert!(matches!(
            parse_bytes(&data, id, None).unwrap_err(),
            DcpError::InvalidHeader(_)
        ));
    }

    #[test]
    fn rejects_oversized_tag() {
        // A tag claiming a count that implies > 64 MB must be rejected up front,
        // before any allocation/slice.
        let mut data = build_synthetic_dcp(&synthetic_tags());
        // Find the ProfileHueSatMapData1 entry (tag 50938) and inflate its count.
        // Entry starts at 8 (IFD) + 2 (count) + index*12.
        let ifd = 8usize;
        let count = data[ifd] as u16 as usize;
        let entry_base = ifd + 2;
        for i in 0..count {
            let base = entry_base + i * 12;
            let tag = u16::from_le_bytes([data[base], data[base + 1]]);
            if tag == 50938 {
                // FLOAT, count 20_000_000 -> 80 MB > 64 MB limit.
                data[base + 4..base + 8].copy_from_slice(&20_000_000u32.to_le_bytes());
                break;
            }
        }
        let id = ProfileId(*blake3::hash(&data).as_bytes());
        assert!(matches!(
            parse_bytes(&data, id, None).unwrap_err(),
            DcpError::Oversized(_)
        ));
    }

    #[test]
    fn rejects_truncated_offsets() {
        // Build a valid DCP then cut it short; must return an error, never panic.
        let data = build_synthetic_dcp(&synthetic_tags());
        for len in 0..data.len() {
            let slice = &data[..len];
            let id = ProfileId(*blake3::hash(slice).as_bytes());
            let _ = parse_bytes(slice, id, None);
        }
    }

    #[test]
    fn extra_camera_profiles_round_trip() {
        let mut main_tags = synthetic_tags();
        let sub_tags = vec![
            TagSpec {
                tag: 50708,
                typ: 2,
                data: ascii("Sub Camera"),
            },
            TagSpec {
                tag: 50936,
                typ: 2,
                data: ascii("Sub Profile"),
            },
            TagSpec {
                tag: 50721,
                typ: 10,
                data: srat(&[
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                ]),
            },
            TagSpec {
                tag: 50778,
                typ: 3,
                data: 21u16.to_le_bytes().to_vec(),
            },
        ];

        // Layout: header(8) + main IFD + main pool + sub-IFD block.
        let main_n = main_tags.len() + 1; // +1 for ExtraCameraProfiles
        let main_ifd_len = 2 + main_n * 12 + 4;
        let main_pool_len: usize = main_tags
            .iter()
            .map(|t| if t.data.len() > 4 { t.data.len() } else { 0 })
            .sum();
        let sub_ifd_offset = 8 + main_ifd_len + main_pool_len;

        main_tags.push(TagSpec {
            tag: 50933,
            typ: 4,
            data: (sub_ifd_offset as u32).to_le_bytes().to_vec(),
        });

        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&0x4352u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        let main_block = build_ifd_block(&main_tags, 8);
        out.extend_from_slice(&main_block);
        out.extend_from_slice(&build_ifd_block(&sub_tags, sub_ifd_offset));

        assert_eq!(sub_ifd_offset, 8 + main_block.len());

        let id = ProfileId(*blake3::hash(&out).as_bytes());
        let profiles = parse_bytes(&out, id, None).expect("DCP with extra profiles should parse");
        assert_eq!(profiles.len(), 2, "primary + one extra profile");
        assert_eq!(profiles[0].profile_name, "Test Profile");
        assert_eq!(profiles[0].unique_camera_model, "Test Camera");
        assert_eq!(profiles[1].profile_name, "Sub Profile");
        assert_eq!(profiles[1].unique_camera_model, "Sub Camera");
    }

    #[test]
    fn property_mutation_fuzz_no_panic() {
        let base = build_synthetic_dcp(&synthetic_tags());
        let mut rng = StdRng::seed_from_u64(SEED);
        let iterations = 10_000u32;
        let mut panicked = 0u32;

        for _ in 0..iterations {
            let mut buf = base.clone();
            let mode = rng.random_range(0u32..5);
            match mode {
                // Flip a handful of random bytes.
                0 => {
                    let flips = rng.random_range(1..=8);
                    for _ in 0..flips {
                        if buf.is_empty() {
                            break;
                        }
                        let idx = rng.random_range(0..buf.len());
                        buf[idx] ^= rng.random_range(0x01u8..=0xFF);
                    }
                }
                // Truncate to a random prefix.
                1 => {
                    let len = rng.random_range(0..=buf.len());
                    buf.truncate(len);
                }
                // Overwrite a random slice with random bytes.
                2 => {
                    if !buf.is_empty() {
                        let start = rng.random_range(0..buf.len());
                        let end = rng.random_range(start..=buf.len());
                        for b in &mut buf[start..end] {
                            *b = rng.random::<u8>();
                        }
                    }
                }
                // Append random trailing bytes.
                3 => {
                    let extra = rng.random_range(0..64);
                    for _ in 0..extra {
                        buf.push(rng.random::<u8>());
                    }
                }
                // Whole random buffer of arbitrary content.
                4 => {
                    let len = rng.random_range(0..1024);
                    buf = (0..len).map(|_| rng.random::<u8>()).collect();
                }
                _ => unreachable!(),
            }

            let id = ProfileId(*blake3::hash(&buf).as_bytes());
            // Catch panics so the harness can report instead of aborting.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = parse_bytes(&buf, id, None);
            }));
            if result.is_err() {
                panicked += 1;
                break;
            }
        }

        assert_eq!(
            panicked, 0,
            "parser panicked on {panicked} of {iterations} mutated inputs"
        );
    }

    // ---- env-gated tests against vendor assets (never committed) -----------

    /// Parse the supplied Cobalt DCP and assert every value in §1.3 exactly.
    #[test]
    #[ignore = "requires vendor DCP; enable with RAPIDRAW_TEST_ASSETS=1"]
    #[allow(clippy::approx_constant)] // real DCP matrix value near FRAC_PI_6
    fn parses_supplied_cobalt_dcp_exact() {
        let path =
            Path::new("/Users/harrisontucker/Downloads/Fujifilm X-Pro2 Cobalt Flat v3.0.dcp");
        if std::env::var("RAPIDRAW_TEST_ASSETS").as_deref() != Ok("1") {
            return;
        }
        assert!(path.exists(), "supplied DCP not present: {path:?}");

        let profiles = parse_dcp(path).expect("supplied DCP should parse");
        assert_eq!(profiles.len(), 1, "expected exactly one profile");
        let p = &profiles[0];

        // String / count tags.
        assert_eq!(p.unique_camera_model, "Fujifilm X-Pro2");
        assert_eq!(p.profile_name, "Cobalt Flat");
        assert_eq!(p.copyright.as_deref(), Some("(c)Cobalt-Image 2024"));
        assert_eq!(p.embed_policy, EmbedPolicy::EmbedNever);
        assert_eq!(p.calibration_illuminant_1, Illuminant::StdA);
        assert_eq!(p.calibration_illuminant_2, Some(Illuminant::D65));
        assert_eq!(p.look_table_encoding, TableEncoding::Srgb);
        assert_eq!(p.hue_sat_map_encoding, TableEncoding::Linear);

        // Matrices to 1e-6 (row-major, file order).
        let cm1 = [
            1.339f32, -0.731, 0.0216, -0.3983, 1.1994, 0.2238, -0.0435, 0.1035, 0.6328,
        ];
        let cm2 = [
            1.1434, -0.4948, -0.121, -0.3746, 1.2042, 0.1903, -0.0666, 0.1479, 0.5235,
        ];
        let fm1 = [
            0.5852, 0.2478, 0.1314, 0.2148, 0.7488, 0.0364, 0.0075, 0.0195, 0.7981,
        ];
        let fm2 = [
            0.525, 0.2687, 0.1706, 0.198, 0.752, 0.05, 0.0001, 0.0111, 0.8139,
        ];
        assert_matrix(p.color_matrix_1, &cm1);
        assert_matrix(p.color_matrix_2.expect("dual illuminant"), &cm2);
        assert_matrix(p.forward_matrix_1.expect("forward matrix 1"), &fm1);
        assert_matrix(p.forward_matrix_2.expect("forward matrix 2"), &fm2);

        // Rendering tables: counts + dims.
        let hs = p.hue_sat_map.as_ref().expect("hue_sat_map present");
        assert_eq!(
            (hs.map_1.hue_div, hs.map_1.sat_div, hs.map_1.val_div),
            (90, 30, 1)
        );
        assert_eq!(
            hs.map_1.data.len(),
            2700,
            "HueSatMap1 must be 2700 triplets"
        );
        let hs2 = hs.map_2.as_ref().expect("hue_sat_map 2 present");
        assert_eq!(hs2.data.len(), 2700, "HueSatMap2 must be 2700 triplets");

        let lt = p.look_table.as_ref().expect("look_table present");
        assert_eq!((lt.hue_div, lt.sat_div, lt.val_div), (90, 30, 30));
        assert_eq!(lt.data.len(), 81_000, "LookTable must be 81000 triplets");

        // Tone curve: 8192 (x,y) pairs.
        let tc = p.tone_curve.as_ref().expect("tone_curve present");
        assert_eq!(tc.points.len(), 8192, "tone curve must be 8192 pairs");
    }

    /// Parse additional vendor DCPs from a directory to prove generality.
    /// Looks for `test-assets/dcp/*.dcp` (see test-assets/README.md).
    #[test]
    #[ignore = "requires additional vendor DCPs; enable with RAPIDRAW_TEST_ASSETS=1"]
    fn round_trip_additional_vendor_dcps() {
        if std::env::var("RAPIDRAW_TEST_ASSETS").as_deref() != Ok("1") {
            return;
        }
        let dir = Path::new("test-assets/dcp");
        if !dir.is_dir() {
            eprintln!("no test-assets/dcp directory; skipping");
            return;
        }
        let mut parsed = 0;
        for entry in fs::read_dir(dir).expect("read test-assets/dcp") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("dcp") {
                continue;
            }
            let profiles = parse_dcp(&path).expect("vendor DCP should parse");
            assert!(!profiles.is_empty(), "no profiles parsed from {path:?}");
            for p in &profiles {
                assert!(!p.unique_camera_model.is_empty());
                assert!(!p.profile_name.is_empty());
            }
            parsed += profiles.len();
        }
        assert!(parsed >= 3, "expected >= 3 vendor DCPs, parsed {parsed}");
    }

    fn assert_matrix(actual: Mat3, expected: &[f32; 9]) {
        for i in 0..3 {
            for j in 0..3 {
                let a = actual[(i, j)];
                let e = expected[i * 3 + j];
                assert!(
                    (a - e).abs() < 1e-6,
                    "matrix mismatch at ({i},{j}): got {a}, expected {e}"
                );
            }
        }
    }
}
