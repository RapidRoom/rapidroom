//! Data model for Adobe DNG Camera Profiles (DCP).
//!
//! Normative shape from `COBALT_DCP_SPEC.md` §5.1. These types are produced by
//! `parser.rs` and consumed by the render chain (W2), the registry/pairing
//! logic (W5) and the UI (W6).

use std::path::PathBuf;

/// A 3x3 colour matrix. Stored row-major matching the file order: the DCP
/// `ColorMatrix`/`ForwardMatrix` tags carry 9 rationals laid out row-by-row
/// (`[r0c0, r0c1, r0c2, r1c0, ...]`).
pub type Mat3 = nalgebra::Matrix3<f32>;

/// Stable content-derived identity for a profile: the blake3 hash of the raw
/// file bytes. Two paths that point at identical bytes share one `ProfileId`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProfileId(pub [u8; 32]);

impl ProfileId {
    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The illuminant a profile's calibration matrices were computed for.
///
/// `CalibrationIlluminant1`/`2` map to these. The correlated colour
/// temperatures follow `COBALT_DCP_SPEC.md` §4.2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Illuminant {
    Daylight,
    Fluorescent,
    Tungsten,
    Flash,
    StdA,
    StdB,
    StdC,
    D55,
    D65,
    D75,
    D50,
    IsoStudioTungsten,
    Unknown(u16),
}

impl Illuminant {
    pub fn from_code(code: u16) -> Self {
        match code {
            1 => Self::Daylight,
            2 => Self::Fluorescent,
            3 => Self::Tungsten,
            4 => Self::Flash,
            10 => Self::Flash,
            17 => Self::StdA,
            18 => Self::StdB,
            19 => Self::StdC,
            20 => Self::D55,
            21 => Self::D65,
            22 => Self::D75,
            23 => Self::D50,
            24 => Self::IsoStudioTungsten,
            other => Self::Unknown(other),
        }
    }

    pub fn code(&self) -> u16 {
        match self {
            Self::Daylight => 1,
            Self::Fluorescent => 2,
            Self::Tungsten => 3,
            Self::Flash => 4,
            Self::StdA => 17,
            Self::StdB => 18,
            Self::StdC => 19,
            Self::D55 => 20,
            Self::D65 => 21,
            Self::D75 => 22,
            Self::D50 => 23,
            Self::IsoStudioTungsten => 24,
            Self::Unknown(code) => *code,
        }
    }

    /// Correlated colour temperature in Kelvin per §4.2. Returns `None` for
    /// codes outside the supported set.
    pub fn correlated_temp(&self) -> Option<f64> {
        match self {
            Self::Daylight => Some(5500.0),
            Self::Fluorescent => Some(4200.0),
            Self::Tungsten => Some(2856.0),
            Self::Flash => Some(5500.0),
            Self::StdA => Some(2856.0),
            Self::StdB => Some(4874.0),
            Self::StdC => Some(6774.0),
            Self::D55 => Some(5500.0),
            Self::D65 => Some(6504.0),
            Self::D75 => Some(7504.0),
            Self::D50 => Some(5003.0),
            Self::IsoStudioTungsten => Some(3200.0),
            Self::Unknown(_) => None,
        }
    }
}

/// The colour-space encoding a rendering table's RGB values are stored in.
///
/// `ProfileHueSatMapEncoding` and `ProfileLookTableEncoding`. Per §1.3 the
/// HueSatMap defaults to `Linear` when the tag is absent, while the LookTable
/// here is `Srgb` — the two tables in one pipeline use different encodings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableEncoding {
    Linear,
    Srgb,
}

impl TableEncoding {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Linear,
            1 => Self::Srgb,
            _ => Self::Linear,
        }
    }
}

/// `ProfileEmbedPolicy` (tag 50941). `EmbedNever` must be honoured by any
/// DNG/TIFF export path (§8.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmbedPolicy {
    AllowCopying,
    EmbedIfUsed,
    EmbedNever,
    NoRestrictions,
    Unknown(u32),
}

impl EmbedPolicy {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::AllowCopying,
            1 => Self::EmbedIfUsed,
            2 => Self::EmbedNever,
            3 => Self::NoRestrictions,
            other => Self::Unknown(other),
        }
    }
}

/// `DefaultBlackRender` (tag 50710). `Auto` is the DNG default when the tag is
/// absent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefaultBlackRender {
    Auto,
    None,
}

impl DefaultBlackRender {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            _ => Self::Auto,
        }
    }
}

/// A single rendering table (HueSatMap or LookTable) with its divisions and
/// data. `data` holds one `[hue_shift, sat_scale, val_scale]` triplet per
/// `hue_div * sat_div * val_div` cell.
#[derive(Clone, PartialEq, Debug)]
pub struct HsvTable {
    pub hue_div: u32,
    pub sat_div: u32,
    pub val_div: u32,
    pub data: Vec<[f32; 3]>,
}

impl HsvTable {
    pub fn cell_count(&self) -> u64 {
        self.hue_div as u64 * self.sat_div as u64 * self.val_div as u64
    }
}

/// A dual-illuminant HueSatMap: the interpolated map for illuminant 1 plus an
/// optional second map for illuminant 2.
#[derive(Clone, PartialEq, Debug)]
pub struct DualHueSatMap {
    pub map_1: HsvTable,
    pub map_2: Option<HsvTable>,
}

/// The profile tone curve: a list of `(x, y)` points in [0, 1] domain.
#[derive(Clone, PartialEq, Debug)]
pub struct ToneCurve {
    pub points: Vec<[f32; 2]>,
}

/// A fully parsed Adobe DNG Camera Profile.
///
/// Required tags (a usable profile cannot exist without them) are plain
/// fields; everything else is `Option` with a documented default.
#[derive(Clone, PartialEq, Debug)]
pub struct DcpProfile {
    pub id: ProfileId,
    pub file_path: PathBuf,
    pub profile_name: String,
    pub unique_camera_model: String,
    pub copyright: Option<String>,
    pub embed_policy: EmbedPolicy,
    pub calibration_illuminant_1: Illuminant,
    pub calibration_illuminant_2: Option<Illuminant>,
    pub color_matrix_1: Mat3,
    pub color_matrix_2: Option<Mat3>,
    pub forward_matrix_1: Option<Mat3>,
    pub forward_matrix_2: Option<Mat3>,
    pub camera_calibration_1: Option<Mat3>,
    pub camera_calibration_2: Option<Mat3>,
    pub analog_balance: Option<[f32; 3]>,
    pub baseline_exposure_offset: Option<f32>,
    pub default_black_render: DefaultBlackRender,
    pub hue_sat_map: Option<DualHueSatMap>,
    pub look_table: Option<HsvTable>,
    pub look_table_encoding: TableEncoding,
    pub hue_sat_map_encoding: TableEncoding,
    pub tone_curve: Option<ToneCurve>,
}
