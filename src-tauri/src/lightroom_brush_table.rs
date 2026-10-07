//! Bounded Lightroom brush tables from ACR objects.
//! Decoder/cursor/record rules adapted from skymanbp/autoshade@cbca12b5 (MIT).
//! Full notice: resources/licenses/autoshade-mask-semantics.txt.

use crate::acr::{AcrError, AcrFile};
use std::path::{Path, PathBuf};

const MAX_MASK_BRUSH_UNCOMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MAX_MASK_BRUSH_RECORDS: usize = 256;
const MAX_MASK_BRUSH_D_COUNT: usize = 65_536;
const MAX_MASK_BRUSH_TOKENS: usize = 65_536;
const MAX_CONVERSION_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrushTableErrorKind {
    Unavailable,
    ContainerInvalid,
    ReferenceMismatch,
    DigestMismatch,
    SourceChanged,
    LimitExceeded,
    EncodingUnsupported,
    Corrupt,
    LengthMismatch,
    PayloadInvalid,
    PayloadUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrushTableError {
    pub(crate) class: BrushTableErrorKind,
    detail: String,
}

impl BrushTableError {
    fn new(class: BrushTableErrorKind, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for BrushTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl From<AcrError> for BrushTableError {
    fn from(error: AcrError) -> Self {
        let class = match error {
            AcrError::Unavailable => BrushTableErrorKind::Unavailable,
            AcrError::ContainerInvalid => BrushTableErrorKind::ContainerInvalid,
            AcrError::ReferenceMismatch => BrushTableErrorKind::ReferenceMismatch,
            AcrError::DigestMismatch => BrushTableErrorKind::DigestMismatch,
            AcrError::SourceChanged => BrushTableErrorKind::SourceChanged,
            AcrError::LimitExceeded => BrushTableErrorKind::LimitExceeded,
        };
        Self::new(class, error.to_string())
    }
}

#[derive(Debug)]
pub(crate) struct BrushStroke {
    pub(crate) active: bool,
    pub(crate) value: f64,
    pub(crate) radius: f64,
    pub(crate) flow: f64,
    pub(crate) center_weight: f64,
    pub(crate) dabs: String,
}

/// Scoped to one import, including failures. Repeated hostile references
/// cannot reset byte/token budgets or open the companion more than once.
pub(crate) struct BrushTableReader {
    path: Option<PathBuf>,
    file: Option<Result<AcrFile, BrushTableError>>,
    requests: usize,
    bytes_left: usize,
    tokens_left: usize,
}

impl BrushTableReader {
    pub(crate) fn new(photo: Option<&Path>) -> Self {
        Self {
            path: photo.map(|p| p.with_extension("acr")),
            file: None,
            requests: 0,
            bytes_left: MAX_CONVERSION_BYTES,
            tokens_left: MAX_MASK_BRUSH_TOKENS,
        }
    }

    pub(crate) fn read_table(
        &mut self,
        token: &str,
        expected: usize,
    ) -> Result<Vec<BrushStroke>, BrushTableError> {
        self.requests += 1;
        if self.requests > 256 || expected > self.bytes_left {
            return Err(BrushTableError::new(
                BrushTableErrorKind::LimitExceeded,
                "brush tables exceed the per-import safety limits",
            ));
        }
        if !(8..=MAX_MASK_BRUSH_UNCOMPRESSED_BYTES).contains(&expected) {
            return Err(BrushTableError::new(
                BrushTableErrorKind::LengthMismatch,
                "advertised brush table size is invalid",
            ));
        }
        self.bytes_left -= expected;
        crate::acr::parse_key(token)?;
        if self.file.is_none() {
            self.file = Some(match &self.path {
                Some(path) => AcrFile::open(path).map_err(Into::into),
                None => Err(BrushTableError::new(
                    BrushTableErrorKind::Unavailable,
                    "no image path is available for companion discovery",
                )),
            });
        }
        let file = match self.file.as_mut().expect("initialized companion") {
            Ok(file) => file,
            Err(error) => return Err(error.clone()),
        };
        let blob = file.read_object(token)?;
        if blob.len() < 16
            || [
                le_u32_at(&blob, 0),
                le_u32_at(&blob, 4),
                le_u32_at(&blob, 8),
                le_u32_at(&blob, 12),
            ] != [4, 1, 64_000, (blob.len() - 16) as u32]
        {
            return Err(BrushTableError::new(
                BrushTableErrorKind::EncodingUnsupported,
                "brush table encoding is unsupported",
            ));
        }
        let payload = decode_mask_brush_brotli(&blob[16..], expected)?;
        let strokes = parse_mask_brush_payload(&payload)?;
        let tokens = strokes
            .iter()
            .map(|s| s.dabs.lines().count())
            .sum::<usize>();
        if tokens > self.tokens_left {
            return Err(BrushTableError::new(
                BrushTableErrorKind::LimitExceeded,
                "brush dab tokens exceed the per-import safety limit",
            ));
        }
        self.tokens_left -= tokens;
        Ok(strokes)
    }
}

fn le_u32_at(bytes: &[u8], at: usize) -> u32 {
    crate::acr::u32_at(bytes, at)
}

fn decode_mask_brush_brotli(stream: &[u8], expected: usize) -> Result<Vec<u8>, BrushTableError> {
    use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};
    if expected > MAX_MASK_BRUSH_UNCOMPRESSED_BYTES {
        return Err(BrushTableError::new(
            BrushTableErrorKind::LengthMismatch,
            "advertised output exceeds the size limit",
        ));
    }
    // Standard Brotli caps the history window at 2^24 bytes. The crate's
    // default constructor also accepts an extended, much larger window.
    let mut state = BrotliState::new_strict(
        StandardAlloc::default(),
        StandardAlloc::default(),
        StandardAlloc::default(),
    );
    let mut available_in = stream.len();
    let mut input_offset = 0usize;
    let mut buffer = [0u8; 4_096];
    let mut available_out = buffer.len();
    let mut output_offset = 0usize;
    let mut total_out = 0usize;
    let mut output = Vec::new();
    output.try_reserve_exact(expected).map_err(|_| {
        BrushTableError::new(BrushTableErrorKind::Corrupt, "output allocation refused")
    })?;
    loop {
        let previous_input = input_offset;
        let result = BrotliDecompressStream(
            &mut available_in,
            &mut input_offset,
            stream,
            &mut available_out,
            &mut output_offset,
            &mut buffer,
            &mut total_out,
            &mut state,
        );
        if output.len().saturating_add(output_offset) > expected
            || output.len().saturating_add(output_offset) > MAX_MASK_BRUSH_UNCOMPRESSED_BYTES
        {
            return Err(BrushTableError::new(
                BrushTableErrorKind::LengthMismatch,
                "Brotli output exceeded the advertised or implementation limit",
            ));
        }
        if !matches!(result, BrotliResult::ResultSuccess)
            && previous_input == input_offset
            && output_offset == 0
        {
            return Err(BrushTableError::new(
                BrushTableErrorKind::Corrupt,
                "Brotli decoder made no progress",
            ));
        }
        output.extend_from_slice(&buffer[..output_offset]);
        output_offset = 0;
        available_out = buffer.len();
        match result {
            BrotliResult::ResultSuccess => {
                if available_in != 0 || input_offset != stream.len() {
                    return Err(BrushTableError::new(
                        BrushTableErrorKind::Corrupt,
                        "Brotli stream has trailing input",
                    ));
                }
                break;
            }
            BrotliResult::NeedsMoreInput if available_in == 0 => {
                return Err(BrushTableError::new(
                    BrushTableErrorKind::Corrupt,
                    "Brotli stream is truncated",
                ));
            }
            BrotliResult::ResultFailure => {
                return Err(BrushTableError::new(
                    BrushTableErrorKind::Corrupt,
                    "Brotli decoder rejected the stream",
                ));
            }
            BrotliResult::NeedsMoreInput | BrotliResult::NeedsMoreOutput => {}
        }
    }
    if output.len() != expected {
        return Err(BrushTableError::new(
            BrushTableErrorKind::LengthMismatch,
            format!("decoded {} bytes, XMP advertises {expected}", output.len()),
        ));
    }
    Ok(output)
}

struct MaskBrushCursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> MaskBrushCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], BrushTableError> {
        let end = self.at.checked_add(len).ok_or_else(|| {
            BrushTableError::new(
                BrushTableErrorKind::PayloadInvalid,
                "payload offset overflow",
            )
        })?;
        let out = self.bytes.get(self.at..end).ok_or_else(|| {
            BrushTableError::new(
                BrushTableErrorKind::PayloadInvalid,
                "truncated payload field",
            )
        })?;
        self.at = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16, BrushTableError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("two-byte slice");
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, BrushTableError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four-byte slice");
        Ok(u32::from_le_bytes(bytes))
    }
}

fn parse_mask_brush_payload(bytes: &[u8]) -> Result<Vec<BrushStroke>, BrushTableError> {
    if bytes.len() > MAX_MASK_BRUSH_UNCOMPRESSED_BYTES {
        return Err(BrushTableError::new(
            BrushTableErrorKind::LengthMismatch,
            "payload exceeds the size limit",
        ));
    }
    let mut cursor = MaskBrushCursor::new(bytes);
    if cursor.u32()? != 1 {
        return Err(BrushTableError::new(
            BrushTableErrorKind::PayloadUnsupported,
            "table word is not 1",
        ));
    }
    let record_count = cursor.u32()? as usize;
    if record_count > MAX_MASK_BRUSH_RECORDS {
        return Err(BrushTableError::new(
            BrushTableErrorKind::PayloadInvalid,
            format!("record count exceeds the {MAX_MASK_BRUSH_RECORDS}-record limit"),
        ));
    }
    if record_count
        .checked_mul(70)
        .and_then(|n| n.checked_add(8))
        .is_none_or(|minimum| minimum > bytes.len())
    {
        return Err(BrushTableError::new(
            BrushTableErrorKind::PayloadInvalid,
            "record count cannot fit in the payload",
        ));
    }
    let mut records = Vec::new();
    records.try_reserve_exact(record_count).map_err(|_| {
        BrushTableError::new(
            BrushTableErrorKind::PayloadInvalid,
            "record allocation refused",
        )
    })?;
    let mut table_tokens = 0usize;
    for _ in 0..record_count {
        // Measured tables put an optional UUID before the stroke flags;
        // AutoShade's legacy synthetic fixtures encode an empty prefix.
        match cursor.u32()? {
            0 => {}
            36 => {
                let prefix = cursor.take(36)?;
                if !prefix.iter().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        *b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                }) {
                    return Err(BrushTableError::new(
                        BrushTableErrorKind::PayloadUnsupported,
                        "brush record UUID prefix is invalid",
                    ));
                }
            }
            _ => {
                return Err(BrushTableError::new(
                    BrushTableErrorKind::PayloadUnsupported,
                    "brush record prefix is unsupported",
                ));
            }
        }
        let active = cursor.u32()?;
        let blend = cursor.u32()?;
        let inverted = cursor.u16()?;
        let id_len = cursor.u32()? as usize;
        if active > 1 || blend != 0 || inverted != 0 || id_len != 32 {
            return Err(BrushTableError::new(
                BrushTableErrorKind::PayloadUnsupported,
                format!(
                    "unsupported record fields active={active}, blend={blend}, inverted={inverted}, id_len={id_len}"
                ),
            ));
        }
        let id = cursor.take(id_len)?;
        if !id.is_ascii() {
            return Err(BrushTableError::new(
                BrushTableErrorKind::PayloadUnsupported,
                "MaskSyncID is not ASCII",
            ));
        }
        let value = cursor.u32()?;
        let radius = cursor.u32()?;
        let flow = cursor.u32()?;
        let center_weight = cursor.u32()?;
        let d_count = cursor.u32()? as usize;
        if value > 1_000_000 || radius > 1_000_000 || flow > 1_000_000 || center_weight > 1_000_000
        {
            return Err(BrushTableError::new(
                BrushTableErrorKind::PayloadUnsupported,
                "stroke fields are outside the measured normalized ranges",
            ));
        }
        if d_count > MAX_MASK_BRUSH_D_COUNT {
            return Err(BrushTableError::new(
                BrushTableErrorKind::PayloadInvalid,
                format!("d-count exceeds the {MAX_MASK_BRUSH_D_COUNT}-dab limit"),
            ));
        }
        let mut d_seen = 0usize;
        let mut dabs = String::new();
        while d_seen < d_count {
            if table_tokens >= MAX_MASK_BRUSH_TOKENS {
                return Err(BrushTableError::new(
                    BrushTableErrorKind::PayloadInvalid,
                    format!("token count exceeds the {MAX_MASK_BRUSH_TOKENS}-token limit"),
                ));
            }
            let opcode = cursor.take(1)?[0];
            let token = match opcode {
                0x01 => format!("r {}", fixed_decimal(cursor.u32()?, 6)),
                0x02 => format!("f {}", fixed_decimal(cursor.u32()?, 4)),
                0x03 => format!("h {}", fixed_decimal(cursor.u32()?, 4)),
                0x06 => {
                    d_seen += 1;
                    format!(
                        "d {} {}",
                        fixed_decimal(cursor.u32()?, 6),
                        fixed_decimal(cursor.u32()?, 6)
                    )
                }
                _ => {
                    return Err(BrushTableError::new(
                        BrushTableErrorKind::PayloadUnsupported,
                        format!("unsupported opcode 0x{opcode:02X}"),
                    ));
                }
            };
            if !dabs.is_empty() {
                dabs.push('\n');
            }
            dabs.push_str(&token);
            table_tokens += 1;
        }
        records.push(BrushStroke {
            // Keep inactive distinct from a zero-value erase stroke.
            active: active != 0,
            value: value as f64 / 1_000_000.0,
            radius: radius as f64 / 1_000_000.0,
            flow: flow as f64 / 1_000_000.0,
            center_weight: center_weight as f64 / 1_000_000.0,
            dabs,
        });
    }
    if cursor.at != bytes.len() {
        return Err(BrushTableError::new(
            BrushTableErrorKind::PayloadInvalid,
            format!("{} trailing payload byte(s)", bytes.len() - cursor.at),
        ));
    }
    Ok(records)
}

fn fixed_decimal(value: u32, places: usize) -> String {
    let scale = 10u64.pow(places as u32);
    let value = u64::from(value);
    let whole = value / scale;
    let fraction = value % scale;
    if fraction == 0 {
        return whole.to_string();
    }
    let mut out = format!("{whole}.{fraction:0places$}");
    while out.ends_with('0') {
        out.pop();
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::acr::tests::container;
    use std::fs;
    // AutoShade's authored public synthetic fixtures, compressed independently
    // with Python Brotli. No user specimen bytes are included.
    pub(crate) const MB_GOOD_A_LEN: usize = 185;
    pub(crate) const MB_GOOD_A: &[u8] = &[
        0x1B, 0xB8, 0x00, 0xF8, 0x8F, 0xC2, 0xB6, 0xB5, 0x73, 0x94, 0x79, 0x28, 0xD3, 0x42, 0xF8,
        0xC9, 0x20, 0x88, 0x9B, 0xDF, 0xC6, 0x02, 0xEA, 0x3A, 0x0F, 0x6C, 0x2C, 0x91, 0x28, 0x0E,
        0x3C, 0xF0, 0x31, 0x51, 0xD6, 0x46, 0xAC, 0x01, 0x14, 0x4E, 0xC4, 0xC3, 0x06, 0x9C, 0xA8,
        0x07, 0x1E, 0xA0, 0x47, 0x32, 0xDD, 0x01, 0x20, 0x10, 0xC7, 0x27, 0x96, 0x08, 0x80, 0x08,
        0x00, 0x08, 0x00, 0x00, 0x58, 0x10, 0xF9, 0xEE, 0xDC, 0x49, 0x6B, 0xC2, 0x58, 0x07, 0x20,
        0x02, 0x42, 0x78, 0x81, 0x98, 0x81, 0x5A, 0xD8, 0xAA, 0xBC, 0x89, 0xFA, 0x9B, 0xAA, 0x71,
        0x28, 0x13, 0x13, 0xC2, 0x58, 0xB7, 0xC6, 0x30, 0x36, 0x54, 0xBF, 0x44, 0x93, 0x3B, 0x1A,
    ];
    pub(crate) const MB_GOOD_B_LEN: usize = 87;
    pub(crate) const MB_GOOD_B: &[u8] = &[
        0x1B, 0x56, 0x00, 0xF8, 0x9F, 0x07, 0x76, 0x0C, 0x99, 0x22, 0x68, 0xF8, 0x02, 0xE9, 0xA5,
        0x10, 0x26, 0xF7, 0x24, 0xE1, 0x08, 0xDB, 0x12, 0x4C, 0x23, 0xA8, 0x84, 0xA0, 0x93, 0xE0,
        0x81, 0xBA, 0x12, 0x66, 0x61, 0x03, 0x4E, 0x38, 0x0D, 0x14, 0x47, 0x5A, 0x66, 0xBF, 0x1C,
        0x20, 0xC1, 0x40, 0x03, 0xB5, 0x8C, 0xFB, 0xE9, 0xD1, 0x02, 0x81, 0xBC, 0x35, 0x01,
    ];

    // Authored UUID/hardness fixture; contains no private source bytes.
    pub(crate) const MB_UUID_HARDNESS_LEN: usize = 262;
    pub(crate) const MB_UUID_HARDNESS: &[u8] = &[
        0x17, 0x05, 0x01, 0x00, 0x04, 0xa2, 0x9d, 0xf7, 0xbe, 0x98, 0x44, 0x19, 0x16, 0x83, 0x32,
        0x29, 0x0c, 0x93, 0xec, 0x83, 0x2c, 0x23, 0x10, 0xb3, 0xe3, 0x44, 0x42, 0x0f, 0xf5, 0xb6,
        0x6a, 0x41, 0xaa, 0x49, 0x4b, 0xe9, 0xed, 0xf5, 0x86, 0x2d, 0x8f, 0xf2, 0x58, 0x8a, 0xc6,
        0x21, 0x9e, 0x5f, 0x4a, 0x77, 0xdf, 0x5a, 0x38, 0x74, 0x08, 0x95, 0x71, 0x19, 0x78, 0xe5,
        0xef, 0x78, 0x47, 0x58, 0x3b, 0x4c, 0xa2, 0xf6, 0x95, 0xe5, 0x15, 0x73, 0x13, 0x68, 0xac,
        0x60, 0xfb, 0xe0, 0x2c, 0x26, 0xd6, 0xb1, 0x93, 0xcb, 0x4c, 0xf4, 0xa2, 0x8e, 0xc1, 0x1c,
        0x93, 0xb2,
    ];

    pub(crate) fn object(stream: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        for word in [4, 1, 64_000, stream.len() as u32] {
            b.extend(word.to_le_bytes());
        }
        b.extend(stream);
        b
    }

    #[test]
    fn decodes_fixed_point_fields_tokens_and_keeps_inactive_separate() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.ARW");
        let (store, refs) = container(&[object(MB_GOOD_A), object(MB_GOOD_B)]);
        fs::write(raw.with_extension("acr"), store).unwrap();
        let mut r = BrushTableReader::new(Some(&raw));
        let a = r.read_table(&refs[0], MB_GOOD_A_LEN).unwrap();
        let b = r.read_table(&refs[1], MB_GOOD_B_LEN).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 1);
        assert!(a[0].active);
        assert_eq!(a[0].value, 0.051402);
        assert_eq!(a[0].radius, 0.036957);
        assert_eq!(a[0].flow, 1.0);
        assert_eq!(a[0].center_weight, 0.0);
        assert_eq!(
            a[0].dabs,
            "r 0.123456\nf 0.0103\nd 0.404621 0.692602\nd 0.401151 0.693698"
        );
        let mut payload = decode_mask_brush_brotli(MB_GOOD_A, MB_GOOD_A_LEN).unwrap();
        payload[12..16].copy_from_slice(&0u32.to_le_bytes());
        let records = parse_mask_brush_payload(&payload).unwrap();
        assert!(!records[0].active);
        assert_eq!(records[0].value, 0.051402);
    }

    #[test]
    fn rejects_truncation_trailing_input_bad_lengths_and_unknown_payload_fields() {
        // Reserved window marker 0x11 requests the nonstandard large-window
        // extension; refuse before any history buffer can be allocated.
        assert!(decode_mask_brush_brotli(&[0x11, 30, 0], 8).is_err());
        for expected in [
            MB_GOOD_A_LEN - 1,
            MB_GOOD_A_LEN + 1,
            MAX_MASK_BRUSH_UNCOMPRESSED_BYTES + 1,
        ] {
            assert!(decode_mask_brush_brotli(MB_GOOD_A, expected).is_err());
        }
        for n in [0, 1, MB_GOOD_A.len() - 1] {
            assert!(decode_mask_brush_brotli(&MB_GOOD_A[..n], MB_GOOD_A_LEN).is_err());
        }
        let mut stream = MB_GOOD_A.to_vec();
        stream.push(0);
        assert!(decode_mask_brush_brotli(&stream, MB_GOOD_A_LEN).is_err());
        let payload = decode_mask_brush_brotli(MB_GOOD_A, MB_GOOD_A_LEN).unwrap();
        for (offset, value) in [
            (0, 2),
            (4, 257),
            (8, 1),
            (12, 2),
            (16, 1),
            (22, 31),
            (58, 1_000_001),
            (74, 65_537),
        ] {
            let mut bad = payload.clone();
            bad[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
            assert!(
                parse_mask_brush_payload(&bad).is_err(),
                "accepted mutated field at {offset}"
            );
        }
        let mut bad = payload.clone();
        bad[78] = 4;
        assert!(parse_mask_brush_payload(&bad).is_err());
        let mut bad = payload.clone();
        bad.push(0);
        assert!(parse_mask_brush_payload(&bad).is_err());
        for n in 0..78 {
            assert!(parse_mask_brush_payload(&payload[..n]).is_err());
        }
    }

    #[test]
    fn decodes_uuid_prefixes_and_four_place_hardness_updates() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.ARW");
        let (bytes, refs) = container(&[object(MB_UUID_HARDNESS)]);
        fs::write(raw.with_extension("acr"), bytes).unwrap();
        let strokes = BrushTableReader::new(Some(&raw))
            .read_table(&refs[0], MB_UUID_HARDNESS_LEN)
            .unwrap();
        assert_eq!(strokes.len(), 2);
        assert!(strokes[0].active);
        assert_eq!(strokes[0].value, 0.5);
        assert_eq!(strokes[0].center_weight, 0.0);
        assert_eq!(strokes[0].dabs, "r 0.2\nf 0.5\nh 1\nd 0.5 0.5\nd 0.5 0.5");
        assert!(!strokes[1].active);
        assert_eq!(strokes[1].value, 0.0);
        let payload = decode_mask_brush_brotli(MB_UUID_HARDNESS, MB_UUID_HARDNESS_LEN).unwrap();
        for bad in [
            {
                let mut b = payload.clone();
                b[12] = b'G';
                b
            },
            {
                let mut b = payload.clone();
                b[8..12].copy_from_slice(&35u32.to_le_bytes());
                b
            },
            {
                let mut b = payload.clone();
                assert_eq!(b[124], 3);
                b[124] = 4;
                b
            },
        ] {
            assert!(parse_mask_brush_payload(&bad).is_err());
        }
    }

    #[test]
    fn requires_companion_and_bounds_all_import_attempts_even_failures() {
        let mut r = BrushTableReader::new(None);
        assert_eq!(
            r.read_table(&"0".repeat(32), MB_GOOD_A_LEN)
                .unwrap_err()
                .class,
            BrushTableErrorKind::Unavailable
        );
        r.requests = 256;
        assert_eq!(
            r.read_table(&"0".repeat(32), MB_GOOD_A_LEN)
                .unwrap_err()
                .class,
            BrushTableErrorKind::LimitExceeded
        );
        r.requests = 0;
        r.bytes_left = 1;
        assert_eq!(
            r.read_table(&"0".repeat(32), MB_GOOD_A_LEN)
                .unwrap_err()
                .class,
            BrushTableErrorKind::LimitExceeded
        );
    }
}
