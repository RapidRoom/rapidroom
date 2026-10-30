//! Sony maker-note focus distance, matching exiftool's `FocusDistance2`.
//!
//! The metres are shown in the metadata panel. They are not written to
//! `SubjectDistance`, which lens correction already reads.

use std::ffi::CStr;

/// Exiftool `Decipher` for tags 0x2010 and 0x94xx: byte `b` becomes `(b³ mod 249)`,
/// and bytes 249–255 stay put. Built the same way as `Sony.pm`'s translation table.
const DECIPHER: [u8; 256] = decipher_table();

const fn decipher_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut unchanged = 249u16;
    while unchanged < 256 {
        table[unchanged as usize] = unchanged as u8;
        unchanged += 1;
    }
    let mut plain = 0u32;
    while plain < 249 {
        let enciphered = (plain * plain * plain) % 249;
        table[enciphered as usize] = plain as u8;
        plain += 1;
    }
    table
}

/// Printed focus distance for a Sony raw, or nothing when exiftool would omit it.
pub(crate) fn focus_distance(file_bytes: &[u8]) -> Option<String> {
    let tiff = Tiff::open(file_bytes)?;
    let ifd0 = tiff
        .u32(4)
        .and_then(|offset| usize::try_from(offset).ok())?;
    let make = ascii_tag(&tiff, ifd0, 0x010F)?;
    if !make.eq_ignore_ascii_case("SONY") {
        return None;
    }
    let model = ascii_tag(&tiff, ifd0, 0x0110).unwrap_or_default();
    // Tag9402's FocusPosition2 byte. SLT, Hasselblad HV and ILCA use another
    // block; DSC and Stellar do not keep the distance at this offset.
    if model.starts_with("SLT-")
        || model.starts_with("HV")
        || model.starts_with("ILCA-")
        || model.starts_with("DSC-")
        || model.starts_with("Stellar")
    {
        return None;
    }

    let exif_ifd = long_tag(&tiff, ifd0, 0x8769)?;
    let focal = short_tag(&tiff, exif_ifd, 0xA405)?;
    if focal == 0 {
        return None;
    }
    let maker = tag_bytes(&tiff, exif_ifd, 0x927C)?;
    let maker_ifd = if maker_note_has_header(file_bytes, maker.0) {
        maker.0.checked_add(12)?
    } else {
        maker.0
    };

    let mut passes = 1u8;
    let mut focus_byte = None;
    for_entries(&tiff, maker_ifd, |tag, kind, count, value_at| {
        if tag == 0x9400
            && let Some((offset, _)) = located(&tiff, kind, count, value_at)
            && let Some(&first) = file_bytes.get(offset)
            && matches!(first, 0x5E | 0xE7 | 0x04)
        {
            // Exiftool sets DoubleCipher when Tag9400's first byte is already
            // one of these enciphered markers, then deciphers the block twice.
            passes = 2;
        }
        if tag == 0x9402
            && count > 0x2D
            && let Some((offset, size)) = located(&tiff, kind, count, value_at)
            && size > 0x2D
            && let Some(&first) = file_bytes.get(offset)
            && first != 0x05
            && first != 0xFF
            && let Some(&byte) = file_bytes.get(offset + 0x2D)
        {
            focus_byte = Some(byte);
        }
        false
    })?;

    let mut position = focus_byte?;
    for _ in 0..passes {
        position = DECIPHER[usize::from(position)];
    }
    if position == 0 {
        return None;
    }
    if position == 255 {
        return Some("inf".to_string());
    }

    let metres = (f64::exp2(f64::from(position) / 16.0 - 5.0) + 1.0) * f64::from(focal) / 1000.0;
    Some(format!("{} m", format_sig4(metres)))
}

/// exiftool prints finite distances with Perl `sprintf("%.4g")`, which follows
/// libc on this platform. Infinity is the string `inf`, with no unit.
fn format_sig4(value: f64) -> String {
    let mut buf = [0u8; 32];
    // Safety: `buf` is a 32-byte writable buffer, the format is a fixed
    // NUL-terminated literal, and snprintf stops within `buf.len()` bytes.
    let written = unsafe {
        libc::snprintf(
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            c"%.4g".as_ptr(),
            value,
        )
    };
    if written <= 0 {
        return "0".to_string();
    }
    let end = usize::try_from(written).unwrap_or(0).min(buf.len() - 1);
    CStr::from_bytes_until_nul(&buf[..=end])
        .map(|text| text.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "0".to_string())
}

fn maker_note_has_header(bytes: &[u8], offset: usize) -> bool {
    let Some(head) = bytes.get(offset..offset.saturating_add(12)) else {
        return false;
    };
    head.starts_with(b"SONY DSC \0")
        || head.starts_with(b"SONY CAM \0")
        || head.starts_with(b"SONY MOBILE\0")
        || head.starts_with(b"\0\0SONY PIC\0")
        || head.starts_with(b"VHAB     \0")
}

struct Tiff<'a> {
    data: &'a [u8],
    little: bool,
}

impl<'a> Tiff<'a> {
    fn open(data: &'a [u8]) -> Option<Self> {
        let little = match data.get(..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let tiff = Self { data, little };
        if tiff.u16(2)? != 42 {
            return None;
        }
        Some(tiff)
    }

    fn u16(&self, at: usize) -> Option<u16> {
        let bytes: [u8; 2] = self.data.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if self.little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let bytes: [u8; 4] = self.data.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if self.little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }
}

fn for_entries(
    tiff: &Tiff<'_>,
    ifd: usize,
    mut visit: impl FnMut(u16, u16, u32, usize) -> bool,
) -> Option<()> {
    let count = usize::from(tiff.u16(ifd)?);
    if count > 512 {
        return None;
    }
    let entries_end = ifd.checked_add(2)?.checked_add(count.checked_mul(12)?)?;
    if entries_end > tiff.data.len() {
        return None;
    }
    for index in 0..count {
        let at = ifd + 2 + index * 12;
        if visit(tiff.u16(at)?, tiff.u16(at + 2)?, tiff.u32(at + 4)?, at + 8) {
            break;
        }
    }
    Some(())
}

fn located(tiff: &Tiff<'_>, kind: u16, count: u32, value_at: usize) -> Option<(usize, usize)> {
    let unit = match kind {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => return None,
    };
    let size = usize::try_from(count).ok()?.checked_mul(unit)?;
    if size <= 4 {
        return Some((value_at, size));
    }
    let offset = usize::try_from(tiff.u32(value_at)?).ok()?;
    if offset.checked_add(size)? > tiff.data.len() {
        return None;
    }
    Some((offset, size))
}

fn ascii_tag(tiff: &Tiff<'_>, ifd: usize, wanted: u16) -> Option<String> {
    let mut found = None;
    for_entries(tiff, ifd, |tag, kind, count, value_at| {
        if tag != wanted || kind != 2 {
            return false;
        }
        let Some((offset, size)) = located(tiff, kind, count, value_at) else {
            return false;
        };
        let size = size.min(64);
        let Some(bytes) = tiff.data.get(offset..offset + size) else {
            return false;
        };
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        found = Some(String::from_utf8_lossy(&bytes[..end]).trim().to_string());
        true
    })?;
    found.filter(|text| !text.is_empty())
}

fn long_tag(tiff: &Tiff<'_>, ifd: usize, wanted: u16) -> Option<usize> {
    let mut found = None;
    for_entries(tiff, ifd, |tag, kind, count, value_at| {
        if tag == wanted && kind == 4 && count >= 1 {
            found = tiff
                .u32(value_at)
                .and_then(|offset| usize::try_from(offset).ok());
            return true;
        }
        false
    })?;
    found
}

fn short_tag(tiff: &Tiff<'_>, ifd: usize, wanted: u16) -> Option<u16> {
    let mut found = None;
    for_entries(tiff, ifd, |tag, kind, count, value_at| {
        if tag == wanted && kind == 3 && count >= 1 {
            found = tiff.u16(value_at);
            return true;
        }
        false
    })?;
    found
}

fn tag_bytes(tiff: &Tiff<'_>, ifd: usize, wanted: u16) -> Option<(usize, usize)> {
    let mut found = None;
    for_entries(tiff, ifd, |tag, kind, count, value_at| {
        if tag == wanted {
            found = located(tiff, kind, count, value_at);
            return true;
        }
        false
    })?;
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal ARW-shaped TIFF. The focus byte is the enciphered Tag9402
    /// offset 0x2d taken from a real ILCE file, not a value made up to fit the formula.
    fn sample(model: &str, focal: u16, focus_byte: u8) -> Vec<u8> {
        let make = b"SONY\0";
        let mut model_bytes = model.as_bytes().to_vec();
        model_bytes.push(0);
        let mut blob = vec![0x39u8; 0x2E];
        blob[0x2D] = focus_byte;

        let mut data = Vec::new();
        data.extend_from_slice(b"II*\0");
        data.extend_from_slice(&8u32.to_le_bytes());

        data.extend_from_slice(&3u16.to_le_bytes());
        let make_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        let model_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        let exif_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&0u32.to_le_bytes());

        let make_at = data.len();
        data.extend_from_slice(make);
        let model_at = data.len();
        data.extend_from_slice(&model_bytes);

        let exif_at = data.len();
        data.extend_from_slice(&2u16.to_le_bytes());
        let focal_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        let maker_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&0u32.to_le_bytes());

        let maker_at = data.len();
        data.extend_from_slice(&1u16.to_le_bytes());
        let tag_entry = data.len();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&0u32.to_le_bytes());
        let blob_at = data.len();
        data.extend_from_slice(&blob);

        let put = |data: &mut [u8], at: usize, tag: u16, kind: u16, count: u32, value: u32| {
            data[at..at + 2].copy_from_slice(&tag.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&kind.to_le_bytes());
            data[at + 4..at + 8].copy_from_slice(&count.to_le_bytes());
            data[at + 8..at + 12].copy_from_slice(&value.to_le_bytes());
        };
        put(
            &mut data,
            make_entry,
            0x010F,
            2,
            make.len() as u32,
            make_at as u32,
        );
        put(
            &mut data,
            model_entry,
            0x0110,
            2,
            model_bytes.len() as u32,
            model_at as u32,
        );
        put(&mut data, exif_entry, 0x8769, 4, 1, exif_at as u32);
        put(&mut data, focal_entry, 0xA405, 3, 1, u32::from(focal));
        put(&mut data, maker_entry, 0x927C, 7, 18, maker_at as u32);
        put(
            &mut data,
            tag_entry,
            0x9402,
            7,
            blob.len() as u32,
            blob_at as u32,
        );
        data
    }

    #[test]
    fn matches_exiftool_focus_distance2_strings() {
        // Enciphered bytes and printed distances from exiftool 13.50 on ILCE-7CM2 and ILCE-7CR raws.
        let cases = [
            ("ILCE-7CM2", 24, 0x09u8, Some("5.19 m")),
            ("ILCE-7CM2", 36, 0x43, Some("26.1 m")),
            ("ILCE-7CM2", 24, 0x0D, Some("10.36 m")),
            ("ILCE-7CM2", 24, 0x80, Some("4.368 m")),
            ("ILCE-7CM2", 36, 0x9D, Some("10.53 m")),
            ("ILCE-7CR", 49, 0x89, Some("37.1 m")),
            ("ILCE-7CR", 74, 0x89, Some("56.03 m")),
            ("ILCE-7CM2", 24, 0xFF, Some("inf")),
            ("ILCE-7CM2", 24, 0x00, None),
        ];
        for (model, focal, byte, expected) in cases {
            assert_eq!(
                focus_distance(&sample(model, focal, byte)).as_deref(),
                expected,
                "{model} focal {focal} byte {byte:#x}"
            );
        }
    }

    #[test]
    fn skips_models_whose_focus_byte_is_not_at_this_offset() {
        assert_eq!(focus_distance(&sample("DSC-RX100", 24, 0x09)), None);
        assert_eq!(focus_distance(&sample("SLT-A99", 24, 0x09)), None);
        assert_eq!(focus_distance(&sample("ILCA-77M2", 24, 0x09)), None);
        assert_eq!(focus_distance(&sample("Stellar", 24, 0x09)), None);
    }

    #[test]
    fn matches_exiftool_on_corpus_files_when_present() {
        let Ok(dir) = std::env::var("RAPIDROOM_FOCUS_CORPUS") else {
            return;
        };
        let mut saw = 0usize;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.to_ascii_lowercase().ends_with(".arw") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let got = focus_distance(&bytes);
            let output = std::process::Command::new("exiftool")
                .args(["-s3", "-FocusDistance2"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{name}");
            let printed = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let expected = if printed.is_empty() {
                None
            } else {
                Some(printed)
            };
            assert_eq!(got, expected, "{name}");
            if saw == 0 {
                let map = crate::exif_processing::extract_metadata(&bytes).unwrap();
                assert_eq!(
                    map.get("FocusDistance").map(String::as_str),
                    expected.as_deref()
                );
                assert!(!map.contains_key("SubjectDistance"), "{name}");
            }
            saw += 1;
        }
        assert!(saw > 0, "corpus directory had no ARW files");
    }
}
