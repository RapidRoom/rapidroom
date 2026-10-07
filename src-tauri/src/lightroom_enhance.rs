//! Metadata-only decoding of Lightroom Enhance settings. No processing is run.
//! Adobe big-table format reference: dng_big_table ASCIItoBinary/DecodeFromBinary.

use flate2::read::ZlibDecoder;
use serde::Serialize;
use std::io::Read;

const MAX_SETTINGS_BYTES: usize = 1024 * 1024;
const ALPHABET: &[u8; 85] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnhanceSettings {
    pub denoise: bool,
    pub denoise_luma_amount: Option<f64>,
}

pub(crate) fn decode_referenced_settings(xmp: &str, reference: &str) -> Option<EnhanceSettings> {
    if reference.len() != 32 || !reference.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // Table_<fingerprint> is not an adjustment key; read only this named table.
    let encoded = crate::preset_converter::extract_namespaced_scalar(
        xmp,
        "crs",
        &format!("Table_{reference}"),
    )?;
    decode_settings(&encoded, reference)
}

fn base85(encoded: &str) -> Option<Vec<u8>> {
    if encoded.len() > MAX_SETTINGS_BYTES * 2 {
        return None;
    }
    let mut out = Vec::new();
    let mut block = 0u64;
    let mut multiplier = 1u64;
    let mut count = 0;
    for byte in encoded.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let digit = ALPHABET.iter().position(|&c| c == byte)? as u64;
        block += digit * multiplier;
        count += 1;
        if count == 5 {
            out.extend_from_slice(&u32::try_from(block).ok()?.to_le_bytes());
            (block, multiplier, count) = (0, 1, 0);
        } else {
            multiplier *= 85;
        }
    }
    if count == 1 {
        return None;
    }
    if count > 1 {
        let bytes = u32::try_from(block).ok()?.to_le_bytes();
        out.extend_from_slice(&bytes[..count - 1]);
    }
    Some(out)
}

pub(crate) fn decode_settings(encoded: &str, reference: &str) -> Option<EnhanceSettings> {
    if reference.len() != 32 || !reference.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let compressed = base85(encoded)?;
    let expected = u32::from_le_bytes(compressed.get(..4)?.try_into().ok()?) as usize;
    if !(16..=MAX_SETTINGS_BYTES).contains(&expected) {
        return None;
    }
    let mut decoder = ZlibDecoder::new(&compressed[4..]);
    let mut data = Vec::new();
    decoder
        .by_ref()
        .take(expected as u64 + 1)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() != expected || decoder.total_in() as usize != compressed.len() - 4 {
        return None;
    }
    if !format!("{:x}", md5::compute(&data)).eq_ignore_ascii_case(reference) {
        return None;
    }

    // Only the settings envelope identified by the local Enhance probe is
    // supported. Unknown types/versions remain unsupported, without guessing.
    let word = |i: usize| {
        data.get(i * 4..i * 4 + 4)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_le_bytes)
    };
    if [word(0)?, word(1)?, word(2)?] != [3, 1, 1] || word(3)? as usize != data.len() - 16 {
        return None;
    }
    let xml = std::str::from_utf8(&data[16..]).ok()?;
    let attrs = crate::preset_converter::parse_xmp_attributes(xml).ok()?;
    let denoise = match attrs.get("Denoise")?.as_str() {
        value if value.eq_ignore_ascii_case("true") || value == "1" => true,
        value if value.eq_ignore_ascii_case("false") || value == "0" => false,
        _ => return None,
    };
    let denoise_luma_amount = attrs
        .get("DenoiseLumaAmount")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v));
    Some(EnhanceSettings {
        denoise,
        denoise_luma_amount,
    })
}

// Independent Python struct/zlib/MD5/Base85 fixtures; no private photo data.
#[cfg(test)]
pub(crate) const ENABLED: &str = r#"n2000]Pz-J|e1VfRFOmuhKHigH14+IQX=%e^}9FQ!CS++PE0o+cc`K5wkti'g%4/I{$$5E^A#?)1/P*a!.POUusnEXCo+K=$AEra`:t5TAY#W)Af5c2['$2Hw%:*YzI[Esia'?(88CY.?ifqj?Yw!xKI`NWo]LNfE=93q:`K-Z0STT`{3K#q*[3KL|X|bl]KQ@-U4TBj:2"#;
#[cfg(test)]
pub(crate) const ENABLED_REF: &str = "e714a66ca2c13000de7bb00bcba198eb";
#[cfg(test)]
pub(crate) const DISABLED: &str = r#"o2000]Pz-J`e1Vf`cXyzzQq][R)DgGqq+z'eU{zyi31}CxM^[A:Y`w1!i(nXO9b8d1#T=c-@n]qbbFcvA{woMYprrwR+%D-^y(ZLAl2u[T:?:(g*0Dxn+meO=udg+[5EU(@YlHH]S|kaQw@'k[!$^`M'CqcSpI{kt#DNwTI1A]x1hQzj)PK|WmlG7uiy=jH+2j=67k(jS0"#;
#[cfg(test)]
pub(crate) const DISABLED_REF: &str = "21788e8d3cc701c33393268952a726ad";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_verified_settings_and_amount() {
        assert_eq!(
            decode_settings(ENABLED, &ENABLED_REF.to_uppercase()),
            Some(EnhanceSettings {
                denoise: true,
                denoise_luma_amount: Some(37.0)
            })
        );
        assert!(!decode_settings(DISABLED, DISABLED_REF).unwrap().denoise);
        assert_eq!(
            decode_settings(&format!(" \n{ENABLED}\t"), ENABLED_REF),
            decode_settings(ENABLED, ENABLED_REF)
        );
    }

    #[test]
    fn rejects_corruption_truncation_and_mismatched_reference() {
        assert!(decode_settings(ENABLED, DISABLED_REF).is_none());
        assert!(decode_settings(&ENABLED[..ENABLED.len() - 5], ENABLED_REF).is_none());
        assert!(decode_settings(&format!("{ENABLED}&"), ENABLED_REF).is_none());
        assert!(decode_settings(ENABLED, "not-a-fingerprint").is_none());
        assert!(base85("$").is_none());
        assert!(base85("$$$$$").is_none());
    }

    #[test]
    fn rejects_oversized_inputs_before_decompression() {
        assert!(decode_settings(&"0".repeat(MAX_SETTINGS_BYTES * 2 + 1), ENABLED_REF).is_none());
        // All-ones size prefix, encoded with the same independent fixture generator.
        assert!(decode_settings("0cSn%", ENABLED_REF).is_none());
    }

    #[test]
    #[ignore = "requires RAPIDROOM_TEST_ENHANCE_XMP pointing to a local Enhance sidecar"]
    fn local_enhance_sidecar_probe() {
        let path = std::env::var("RAPIDROOM_TEST_ENHANCE_XMP").unwrap();
        let xml = std::fs::read_to_string(path).unwrap();
        let reference =
            crate::preset_converter::extract_namespaced_scalar(&xml, "crs", "CompressedSettings")
                .unwrap();
        let settings = decode_referenced_settings(&xml, &reference).unwrap();
        assert!(settings.denoise);
        assert!(settings.denoise_luma_amount.is_some());
        println!(
            "Verified local Enhance settings: {}",
            serde_json::to_string(&settings).unwrap()
        );
    }
}
