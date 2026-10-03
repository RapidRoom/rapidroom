//! Adobe DNG Camera Profile (DCP) pipeline.
//!
//! Assembled by several workstreams of the Cobalt DCP project. This branch
//! (W1) adds the binary `IIRC` DCP reader: `parser` + `model`. A parallel
//! workstream (W7) adds `look_xmp` (Cobalt Look XMP parsing) on a separate
//! branch; when the two PRs merge, this file's module list and `DcpError`
//! variants are reconciled by combining them (the declarations below are
//! deliberately additive).
//!
//! Re-exports and `DcpError` live here so later workstreams (W2, W5, W6) build
//! on them rather than redefining them.

// The reader API is consumed by W2/W5/W6; until those land, no path reaches it
// from the binary crate, so suppress dead-code for the module rather than
// sprinkling `#[allow]` per item.
#![allow(dead_code)]

pub mod model;
pub mod parser;

// Added by W7 on its own branch; reconcile at merge by uncommenting.
// pub mod look_xmp;

// Re-exports are the public API surface for W2/W5/W6; unused until they land.
#[allow(unused_imports)]
pub use model::*;
#[allow(unused_imports)]
pub use parser::parse_dcp;

use std::fmt;

/// Errors produced while reading Adobe camera profiles and Cobalt Looks.
///
/// The `NotACobaltLook` / `MissingField` / `InvalidValue` / `TableDecode`
/// variants are shared with W7 (Cobalt Look XMP parsing); the remaining
/// variants cover the binary DCP (`IIRC`) reader added by W1.
#[derive(Debug)]
pub enum DcpError {
    // W7 (Cobalt Look XMP) variants.
    /// The XMP was not a Cobalt Look (`crs:PresetType != "Look"`) or was not
    /// parseable XML.
    NotACobaltLook(String),
    /// A required Cobalt Look field was absent.
    MissingField(&'static str),
    /// A supplied value could not be interpreted (boolean, table, ...).
    InvalidValue { field: &'static str, detail: String },
    /// The embedded look table (dng_big_table / dng_rgb_table) failed to decode.
    TableDecode(String),

    // W1 (binary DCP / IFD) variants.
    /// The file is not a DCP: bad `II` byte order or `0x4352` magic, or it is
    /// big-endian (`MM`), which DCP never is.
    InvalidHeader(String),
    /// A structural read went out of bounds — the file is truncated or an
    /// offset/length is corrupt.
    Truncated(String),
    /// A tag could not be parsed (wrong type, wrong count, unsupported value).
    BadTag { tag: u16, detail: String },
    /// A file or single tag exceeds a safety size limit.
    Oversized(String),
    /// Underlying filesystem/IO error.
    Io(std::io::Error),
}

impl fmt::Display for DcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DcpError::NotACobaltLook(msg) => write!(f, "not a Cobalt Look XMP: {msg}"),
            DcpError::MissingField(field) => {
                write!(f, "missing required Cobalt Look field `{field}`")
            }
            DcpError::InvalidValue { field, detail } => {
                write!(f, "invalid value for Cobalt Look field `{field}`: {detail}")
            }
            DcpError::TableDecode(msg) => write!(f, "failed to decode Cobalt look table: {msg}"),
            DcpError::InvalidHeader(msg) => write!(f, "invalid DCP header: {msg}"),
            DcpError::Truncated(msg) => write!(f, "truncated or corrupt DCP: {msg}"),
            DcpError::BadTag { tag, detail } => write!(f, "bad DCP tag 0x{tag:04x}: {detail}"),
            DcpError::Oversized(msg) => write!(f, "oversized DCP input: {msg}"),
            DcpError::Io(err) => write!(f, "I/O error reading DCP: {err}"),
        }
    }
}

impl std::error::Error for DcpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DcpError::Io(err) => Some(err),
            _ => None,
        }
    }
}
