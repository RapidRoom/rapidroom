//! Old-style Lightroom `.lrtemplate` presets are Lua source of the form
//! `s = { title = "...", value = { settings = { ... } } }`. This module parses
//! a bounded subset of Lua table syntax (no evaluation) and rewrites the
//! develop settings as XMP text, so `preset_converter` stays the only mapping.

const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_VALUES: usize = 200_000;

const CURVE_KEYS: [&str; 8] = [
    "ToneCurvePV2012",
    "ToneCurvePV2012Red",
    "ToneCurvePV2012Green",
    "ToneCurvePV2012Blue",
    "ToneCurve",
    "ToneCurveRed",
    "ToneCurveGreen",
    "ToneCurveBlue",
];

const MASK_KEYS: [&str; 4] = [
    "MaskGroupBasedCorrections",
    "PaintBasedCorrections",
    "GradientBasedCorrections",
    "CircularGradientBasedCorrections",
];

#[derive(Debug, Clone, PartialEq)]
enum LuaValue {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table(LuaTable),
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LuaTable {
    fields: Vec<(String, LuaValue)>,
    items: Vec<LuaValue>,
}

impl LuaTable {
    fn get(&self, key: &str) -> Option<&LuaValue> {
        self.fields
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.items.is_empty()
    }
}

#[derive(Debug, PartialEq)]
pub struct LrtemplateXmp {
    pub xmp: String,
    pub unsupported: Vec<String>,
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    values: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            values: 0,
        }
    }

    fn line(&self) -> usize {
        self.src[..self.pos.min(self.src.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1
    }

    fn err<T>(&self, message: &str) -> Result<T, String> {
        Err(format!(
            "Malformed .lrtemplate (line {}): {}",
            self.line(),
            message
        ))
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                self.pos += 1;
            }
            if self.src[self.pos..].starts_with(b"--") {
                while self.peek().is_some_and(|b| b != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_trivia();
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Option<String> {
        self.skip_trivia();
        let start = self.pos;
        if !self
            .peek()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        {
            return None;
        }
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            self.pos += 1;
        }
        Some(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
    }

    fn parse_chunk(&mut self) -> Result<LuaTable, String> {
        match self.ident() {
            Some(name) if name != "return" => {
                if !self.eat(b'=') {
                    return self.err("expected `=` after the preset variable");
                }
            }
            Some(_) => {}
            None => return self.err("expected `s = {`"),
        }
        let table = match self.parse_value(0)? {
            LuaValue::Table(table) => table,
            _ => return self.err("the preset must be a table"),
        };
        self.skip_trivia();
        let rest_start = self.pos;
        if self.ident().as_deref() == Some("return") {
            if self.ident().is_none() {
                return self.err("expected a variable after `return`");
            }
        } else {
            self.pos = rest_start;
        }
        self.skip_trivia();
        if self.pos != self.src.len() {
            return self.err("unexpected content after the preset table");
        }
        Ok(table)
    }

    fn parse_value(&mut self, depth: usize) -> Result<LuaValue, String> {
        self.values += 1;
        if self.values > MAX_VALUES {
            return self.err("too many values");
        }
        self.skip_trivia();
        match self.peek() {
            Some(b'{') => {
                if depth >= MAX_DEPTH {
                    return self.err("tables nested too deeply");
                }
                self.pos += 1;
                self.parse_table_body(depth + 1).map(LuaValue::Table)
            }
            Some(b'"' | b'\'') => self.parse_string().map(LuaValue::Str),
            Some(b'-' | b'.' | b'0'..=b'9') => self.parse_number().map(LuaValue::Num),
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                let word = self.ident().unwrap_or_default();
                match word.as_str() {
                    "true" => Ok(LuaValue::Bool(true)),
                    "false" => Ok(LuaValue::Bool(false)),
                    "nil" => Ok(LuaValue::Nil),
                    // Lightroom wraps localisable titles as `LOC "$$$/Key=Text"`.
                    "LOC" | "ZSTR" => {
                        self.skip_trivia();
                        if matches!(self.peek(), Some(b'"' | b'\'')) {
                            self.parse_string().map(LuaValue::Str)
                        } else {
                            self.err("expected a string after LOC")
                        }
                    }
                    _ => self.err(&format!("unsupported expression `{}`", word)),
                }
            }
            Some(_) => self.err("unexpected character"),
            None => self.err("unexpected end of file"),
        }
    }

    fn parse_table_body(&mut self, depth: usize) -> Result<LuaTable, String> {
        let mut table = LuaTable::default();
        loop {
            if self.eat(b'}') {
                return Ok(table);
            }
            self.skip_trivia();
            let start = self.pos;
            let key = if self.peek() == Some(b'[') {
                self.pos += 1;
                self.skip_trivia();
                let key = match self.peek() {
                    Some(b'"' | b'\'') => self.parse_string()?,
                    _ => return self.err("only string keys are supported in `[...]`"),
                };
                if !self.eat(b']') {
                    return self.err("expected `]`");
                }
                if !self.eat(b'=') {
                    return self.err("expected `=` after `]`");
                }
                Some(key)
            } else if let Some(name) = self.ident() {
                if self.eat(b'=') {
                    Some(name)
                } else {
                    self.pos = start;
                    None
                }
            } else {
                None
            };

            let value = self.parse_value(depth)?;
            match key {
                Some(key) => table.fields.push((key, value)),
                None => table.items.push(value),
            }

            if !self.eat(b',') && !self.eat(b';') {
                if self.eat(b'}') {
                    return Ok(table);
                }
                return self.err("expected `,` or `}` in table");
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        let quote = self.peek().unwrap_or(b'"');
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return self.err("unterminated string");
            };
            self.pos += 1;
            match b {
                b'\n' => return self.err("unterminated string"),
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        return self.err("unterminated string");
                    };
                    self.pos += 1;
                    match escaped {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'\n' => out.push(b'\n'),
                        b'\\' | b'"' | b'\'' => out.push(escaped),
                        b'0'..=b'9' => {
                            let mut code = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'9') => {
                                        code = code * 10 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            match u8::try_from(code) {
                                Ok(byte) => out.push(byte),
                                Err(_) => return self.err("invalid escape in string"),
                            }
                        }
                        _ => return self.err("invalid escape in string"),
                    }
                }
                _ if b == quote => break,
                _ => out.push(b),
            }
        }
        String::from_utf8(out).or_else(|_| self.err("string is not valid UTF-8"))
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
            self.skip_trivia();
        }
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'.') {
            self.pos += 1;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return self.err("invalid number");
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        let has_digit = text
            .split(['e', 'E'])
            .next()
            .is_some_and(|mantissa| mantissa.bytes().any(|b| b.is_ascii_digit()));
        match text.parse::<f64>() {
            Ok(value) if has_digit && value.is_finite() => {
                Ok(if negative { -value } else { value })
            }
            _ => self.err("invalid number"),
        }
    }
}

fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{}", value)
    }
}

fn localized_title(raw: &str) -> &str {
    if raw.starts_with("$$$/") {
        raw.split_once('=').map_or(raw, |(_, text)| text)
    } else {
        raw
    }
}

fn curve_points(key: &str, table: &LuaTable) -> Result<Vec<(u32, u32)>, String> {
    let malformed = |reason: &str| Err(format!("Malformed .lrtemplate: {} {}", key, reason));
    if !table.fields.is_empty() {
        return malformed("has named entries; expected a flat list of x, y numbers");
    }
    if !table.items.len().is_multiple_of(2) {
        return malformed("has an odd number of values; expected x, y pairs");
    }
    let mut coords = Vec::with_capacity(table.items.len());
    for item in &table.items {
        match item {
            LuaValue::Num(n) if n.fract() == 0.0 && (0.0..=255.0).contains(n) => {
                coords.push(*n as u32)
            }
            LuaValue::Num(_) => return malformed("has a value outside 0-255"),
            _ => return malformed("has a non-numeric value"),
        }
    }
    Ok(coords.chunks(2).map(|pair| (pair[0], pair[1])).collect())
}

/// Converts the Lua-table form of a `.lrtemplate` into XMP text readable by
/// `preset_converter::convert_xmp_to_preset`. Settings the converter cannot
/// represent are listed in `unsupported`, using the identifiers of
/// `preset_converter::lightroom_settings_not_transferred` where one exists.
pub fn lrtemplate_to_xmp(source: &str) -> Result<LrtemplateXmp, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("Malformed .lrtemplate: file is too large".to_string());
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let root = Parser::new(source).parse_chunk()?;

    if let Some(LuaValue::Str(kind)) = root.get("type")
        && !kind.eq_ignore_ascii_case("Develop")
    {
        return Err(format!(
            "Unsupported .lrtemplate: `{}` template, not a Develop preset",
            kind
        ));
    }
    let Some(LuaValue::Table(value)) = root.get("value") else {
        return Err("Malformed .lrtemplate: missing `value` table".to_string());
    };
    let Some(LuaValue::Table(settings)) = value.get("settings") else {
        return Err("Malformed .lrtemplate: missing `value.settings` table".to_string());
    };

    settings_to_xmp(settings, root.get("title"), false)
}

/// Catalog develop rows contain a settings table directly, rather than the
/// preset wrapper. They use the same bounded parser and XMP encoder.
pub fn catalog_develop_to_xmp(
    source: &str,
    as_shot: Option<(f64, f64)>,
) -> Result<LrtemplateXmp, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("Catalog develop row is too large".into());
    }
    let mut settings =
        Parser::new(source.strip_prefix('\u{feff}').unwrap_or(source)).parse_chunk()?;
    if let Some((temperature, tint)) = as_shot {
        if !temperature.is_finite() || temperature <= 0.0 || !tint.is_finite() {
            return Err("Invalid catalog as-shot white balance".into());
        }
        settings
            .fields
            .retain(|(key, _)| key != "AsShotTemperature" && key != "AsShotTint");
        settings
            .fields
            .push(("AsShotTemperature".into(), LuaValue::Num(temperature)));
        settings
            .fields
            .push(("AsShotTint".into(), LuaValue::Num(tint)));
    }
    if settings.get("HasCrop").is_none()
        && ["CropLeft", "CropRight", "CropTop", "CropBottom"]
            .iter()
            .all(|key| matches!(settings.get(key), Some(LuaValue::Num(_))))
    {
        settings
            .fields
            .push(("HasCrop".into(), LuaValue::Bool(true)));
    }
    settings_to_xmp(&settings, None, true)
}

/// Only an explicitly as-shot history row supplies a white-balance baseline.
/// No estimates are made from other photos in the folder.
pub fn catalog_as_shot_white_balance(source: &str) -> Result<Option<(f64, f64)>, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("Catalog develop row is too large".into());
    }
    let settings = Parser::new(source.strip_prefix('\u{feff}').unwrap_or(source)).parse_chunk()?;
    if matches!(settings.get("WhiteBalance"), Some(LuaValue::Str(value)) if value.eq_ignore_ascii_case("As Shot"))
        && let (Some(LuaValue::Num(temperature)), Some(LuaValue::Num(tint))) =
            (settings.get("Temperature"), settings.get("Tint"))
        && *temperature > 0.0
    {
        return Ok(Some((*temperature, *tint)));
    }
    Ok(None)
}

fn settings_to_xmp(
    settings: &LuaTable,
    title: Option<&LuaValue>,
    report_scalars: bool,
) -> Result<LrtemplateXmp, String> {
    let mut attributes = String::new();
    let mut curves = String::new();
    let mut unsupported = Vec::new();

    let mut report = |item: &str| {
        if !unsupported.iter().any(|existing| existing == item) {
            unsupported.push(item.to_string());
        }
    };

    for (key, val) in &settings.fields {
        let valid_name = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric());
        if !valid_name {
            report(key);
            continue;
        }
        match (key.as_str(), val) {
            ("CameraProfile", LuaValue::Str(profile))
                if !profile.is_empty() && profile != "Adobe Standard" =>
            {
                report("cameraProfile")
            }
            ("LensProfileEnable", LuaValue::Num(n)) if *n != 0.0 => report("lensProfile"),
            ("LensProfileEnable", LuaValue::Bool(true)) => report("lensProfile"),
            _ => {}
        }
        let scalar = match val {
            LuaValue::Nil => continue,
            LuaValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            LuaValue::Num(n) => format_number(*n),
            LuaValue::Str(s) => s.clone(),
            LuaValue::Table(table) => {
                if CURVE_KEYS.contains(&key.as_str()) {
                    let points = curve_points(key, table)?;
                    if !points.is_empty() {
                        curves.push_str(&format!("<crs:{}><rdf:Seq>", key));
                        for (x, y) in points {
                            curves.push_str(&format!("<rdf:li>{}, {}</rdf:li>", x, y));
                        }
                        curves.push_str(&format!("</rdf:Seq></crs:{}>", key));
                    }
                } else if table.is_empty() {
                } else if key == "Look" {
                    let disabled =
                        matches!(table.get("Amount"), Some(LuaValue::Num(n)) if *n <= 0.0);
                    if !disabled {
                        report("profileLook");
                        // Reuse the mapper's profile-look luma composition while
                        // reporting unsupported profile color and LUT settings.
                        if report_scalars
                            && let Some(LuaValue::Table(parameters)) = table.get("Parameters")
                            && let Some(LuaValue::Table(curve)) = parameters.get("ToneCurvePV2012")
                        {
                            let points = curve_points("ToneCurvePV2012", curve)?;
                            if !points.is_empty() {
                                let amount = match table.get("Amount") {
                                    Some(LuaValue::Num(value)) => *value,
                                    _ => 1.0,
                                };
                                curves.push_str(&format!(
                                    "<crs:Look><rdf:Description crs:Amount=\"{}\"><crs:Parameters><rdf:Description><crs:ToneCurvePV2012><rdf:Seq>",
                                    format_number(amount)
                                ));
                                for (x, y) in points {
                                    curves.push_str(&format!("<rdf:li>{x}, {y}</rdf:li>"));
                                }
                                curves.push_str("</rdf:Seq></crs:ToneCurvePV2012></rdf:Description></crs:Parameters></rdf:Description></crs:Look>");
                            }
                        }
                    }
                } else if MASK_KEYS.contains(&key.as_str()) {
                    report("masks");
                } else if key == "PointColors" {
                    report("pointColor");
                } else {
                    report(key);
                }
                continue;
            }
        };
        if report_scalars && !crate::preset_converter::is_mapped_xmp_scalar(key) {
            report(key);
        }
        attributes.push_str(&format!(" crs:{}=\"{}\"", key, escape_xml(&scalar)));
    }

    let mut xmp = format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\"{}>",
        attributes
    );
    if let Some(LuaValue::Str(title)) = title {
        let title = localized_title(title).trim();
        if !title.is_empty() {
            xmp.push_str(&format!(
                "<crs:Name><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></crs:Name>",
                escape_xml(title)
            ));
        }
    }
    xmp.push_str(&curves);
    xmp.push_str("</rdf:Description></rdf:RDF></x:xmpmeta>");

    Ok(LrtemplateXmp { xmp, unsupported })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset_converter::convert_xmp_to_preset;
    use serde_json::json;

    #[test]
    fn catalog_rows_use_the_shared_mapper_and_report_nested_settings() {
        let converted = catalog_develop_to_xmp(
            r#"s = { Exposure2012 = 0.5, Sharpness = 40, ColorNoiseReduction = 25,
                ToneCurvePV2012 = { 0, 0, 255, 255 },
                Look = { Parameters = { Exposure2012 = 9 } },
                GradientBasedCorrections = { { LocalExposure = 1 } }, FutureScalar = 7 }"#,
            None,
        )
        .unwrap();
        let preset = convert_xmp_to_preset(&converted.xmp).unwrap();
        assert_eq!(preset.adjustments["exposure"], json!(0.5));
        let reference = convert_xmp_to_preset(
            r#"<rdf:Description crs:Sharpness="40" crs:ColorNoiseReduction="25"/>"#,
        )
        .unwrap();
        assert_eq!(
            preset.adjustments["sharpness"],
            reference.adjustments["sharpness"]
        );
        assert_eq!(
            preset.adjustments["colorNoiseReduction"],
            reference.adjustments["colorNoiseReduction"]
        );
        assert_eq!(
            preset.adjustments["curves"]["luma"],
            json!([{ "x": 0, "y": 0 }, { "x": 255, "y": 255 }])
        );
        assert_eq!(
            converted.unsupported,
            ["profileLook", "masks", "FutureScalar"]
        );
        assert!(catalog_develop_to_xmp("s = { Exposure2012 = os.execute('cmd') }", None).is_err());
    }

    #[test]
    fn catalog_nested_look_curves_reuse_the_existing_composition_mapper() {
        let converted = catalog_develop_to_xmp(
            "s = { Exposure2012 = 0.5, ToneCurvePV2012 = {0, 0, 255, 255}, Look = { Amount = 0.5, Parameters = { Exposure2012 = 9, ToneCurvePV2012 = {0, 0, 64, 48, 192, 208, 255, 255} } } }",
            None,
        ).unwrap();
        let preset = convert_xmp_to_preset(&converted.xmp).unwrap();
        let expected = convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="0.5"><crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012><crs:Look><rdf:Description crs:Amount="0.5"><crs:Parameters><rdf:Description><crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>64, 48</rdf:li><rdf:li>192, 208</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012></rdf:Description></crs:Parameters></rdf:Description></crs:Look></rdf:Description>"#,
        ).unwrap();
        assert_eq!(preset.adjustments, expected.adjustments);
        assert_eq!(preset.adjustments["exposure"], json!(0.5));
        assert_eq!(converted.unsupported, ["profileLook"]);
    }

    #[test]
    fn catalog_white_balance_uses_an_explicit_as_shot_history_row() {
        let baseline = catalog_as_shot_white_balance(
            "s = { WhiteBalance = 'As Shot', Temperature = 4440, Tint = -5 }",
        )
        .unwrap();
        assert_eq!(baseline, Some((4440.0, -5.0)));
        assert_eq!(
            catalog_as_shot_white_balance(
                "s = { WhiteBalance = 'Custom', Temperature = 5550, Tint = 12 }"
            )
            .unwrap(),
            None
        );
        let converted = catalog_develop_to_xmp(
            "s = { WhiteBalance = 'Custom', Temperature = 5550, Tint = 12, AsShotTemperature = 1, AsShotTint = 1 }",
            baseline,
        )
        .unwrap();
        assert_eq!(converted.xmp.matches("crs:AsShotTemperature=").count(), 1);
        let preset =
            crate::preset_converter::convert_xmp_sidecar_to_preset(&converted.xmp).unwrap();
        let expected_temperature = (1_000_000.0 / 4440.0 - 1_000_000.0 / 5550.0) / 150.0 * 100.0;
        assert!(
            (preset.adjustments["temperature"].as_f64().unwrap() - expected_temperature).abs()
                < 1e-6
        );
        assert!((preset.adjustments["tint"].as_f64().unwrap() - 17.0 / 150.0 * 100.0).abs() < 1e-6);
    }

    #[test]
    fn catalog_crops_go_through_the_existing_sidecar_geometry_mapper() {
        let converted = catalog_develop_to_xmp(
            "s = { CropLeft = 0.125, CropRight = 0.25, CropTop = 0, CropBottom = 0.25 }",
            None,
        )
        .unwrap();
        let xmp = converted.xmp.replace(
            "<rdf:Description ",
            "<rdf:Description xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\" tiff:ImageWidth=\"8\" tiff:ImageLength=\"4\" tiff:Orientation=\"8\" ",
        );
        let preset = crate::preset_converter::convert_xmp_sidecar_to_preset(&xmp).unwrap();
        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 0.0, "y": 6.0, "width": 1.0, "height": 1.0 })
        );
    }

    // Synthetic preset in Lightroom's Lua-table layout.
    const SYNTHETIC: &str = r#"s = {
	id = "00000000-0000-0000-0000-000000000000",
	internalName = "Synthetic",
	title = "Test Preset",
	type = "Develop",
	value = {
		settings = {
			Blacks2012 = 25,
			ConvertToGrayscale = false,
			HueAdjustmentGreen = 20,
			SaturationAdjustmentGreen = -55,
			Shadows2012 = 10,
			ToneCurvePV2012 = {
				0,
				6,
				255,
				255,
			},
			ToneCurvePV2012Red = {
				0,
				0,
				116,
				133,
				255,
				255,
			},
		},
		uuid = "00000000-0000-0000-0000-000000000001",
	},
	version = 0,
}
"#;

    fn convert(source: &str) -> (crate::file_management::Preset, Vec<String>) {
        let converted = lrtemplate_to_xmp(source).unwrap();
        (
            convert_xmp_to_preset(&converted.xmp).unwrap(),
            converted.unsupported,
        )
    }

    #[test]
    fn converts_lua_table_preset_through_xmp_mapping() {
        let raw = convert_xmp_to_preset(SYNTHETIC).unwrap();
        assert_eq!(
            raw.adjustments,
            json!({}),
            "raw Lua should not parse as XMP"
        );

        let (preset, unsupported) = convert(SYNTHETIC);
        let a = &preset.adjustments;
        assert_eq!(preset.name, "Test Preset");
        assert_eq!(a["blacks"], 25);
        assert_eq!(a["shadows"], 10);
        assert_eq!(a["hsl"]["greens"], json!({"hue": 15.0, "saturation": -55}));
        assert_eq!(
            a["curves"]["luma"],
            json!([{"x": 0, "y": 6}, {"x": 255, "y": 255}])
        );
        assert_eq!(a["curves"]["red"][1], json!({"x": 116, "y": 133}));
        assert!(a.get("saturation").is_none());
        assert!(unsupported.is_empty());
        assert_eq!(preset.include_masks, Some(false));
        assert_eq!(preset.include_crop_transform, Some(false));
    }

    #[test]
    fn escapes_apostrophes_ampersands_and_markup() {
        let source = r#"s = { title = 'Tom\'s "B&W" <look>', type = "Develop",
            value = { settings = { WhiteBalance = "A&B \"x\" >", Exposure2012 = 1 } } }"#;
        let converted = lrtemplate_to_xmp(source).unwrap();
        assert!(
            converted
                .xmp
                .contains("Tom&apos;s &quot;B&amp;W&quot; &lt;look&gt;")
        );
        assert!(
            converted
                .xmp
                .contains(r#"crs:WhiteBalance="A&amp;B &quot;x&quot; &gt;""#)
        );
        let preset = convert_xmp_to_preset(&converted.xmp).unwrap();
        assert_eq!(preset.name, r#"Tom's "B&W" <look>"#);
        assert_eq!(preset.adjustments["exposure"], 1);
    }

    #[test]
    fn parses_exponents_and_negative_numbers() {
        let source = r#"s = { title = "Exp", value = { settings = {
            Exposure2012 = 2.5e-1, Contrast2012 = -1E1, Clarity2012 = - 7, Vibrance = 1.5E+1,
        } } }"#;
        let (preset, _) = convert(source);
        let a = &preset.adjustments;
        assert_eq!(a["exposure"], 0.25);
        assert_eq!(a["contrast"], -10);
        assert_eq!(a["clarity"], -7);
        assert_eq!(a["vibrance"], 15);
    }

    #[test]
    fn ignores_settings_from_nested_tables() {
        let source = r#"s = {
            title = "Nested",
            type = "Develop",
            value = {
                settings = {
                    Exposure2012 = 0.5,
                    Look = {
                        Name = "Some Look",
                        Parameters = { Exposure2012 = 3, Contrast2012 = 80 },
                    },
                    GradientBasedCorrections = {
                        { CorrectionAmount = 1, LocalExposure2012 = 1 },
                    },
                    PaintBasedCorrections = {},
                    CircularGradientBasedCorrections = { { CorrectionAmount = 1 } },
                    PointColors = { { SrcHue = 0.5 } },
                    RetouchInfo = { "x" },
                    CameraProfile = "Camera Standard",
                    LensProfileEnable = 1,
                },
            },
        }"#;
        let (preset, unsupported) = convert(source);
        assert_eq!(preset.adjustments["exposure"], 0.5);
        assert!(preset.adjustments.get("contrast").is_none());
        assert_eq!(
            unsupported,
            vec![
                "profileLook",
                "masks",
                "pointColor",
                "RetouchInfo",
                "cameraProfile",
                "lensProfile"
            ]
        );
    }

    #[test]
    fn skips_disabled_looks_and_adobe_standard() {
        let source = r#"s = { value = { settings = {
            CameraProfile = "Adobe Standard",
            LensProfileEnable = 0,
            Look = { Amount = 0, Name = "Off" },
        } } }"#;
        assert!(lrtemplate_to_xmp(source).unwrap().unsupported.is_empty());
    }

    #[test]
    fn reads_multiple_tables_and_localized_titles() {
        let source = r#"-- exported by Lightroom
s = {
	title = LOC "$$$/Develop/Presets/Sample=Sample Look",
	type = "Develop",
	value = {
		settings = {
			ToneCurvePV2012Blue = { 0, 10, 255, 245, },
			ToneCurvePV2012Green = { 0, 0, 128, 140, 255, 255 },
			["Highlights2012"] = -30;
		},
	},
}
return s
"#;
        let (preset, unsupported) = convert(source);
        let a = &preset.adjustments;
        assert_eq!(preset.name, "Sample Look");
        assert_eq!(a["highlights"], -30);
        assert_eq!(
            a["curves"]["blue"],
            json!([{"x": 0, "y": 10}, {"x": 255, "y": 245}])
        );
        assert_eq!(a["curves"]["green"][1], json!({"x": 128, "y": 140}));
        assert!(unsupported.is_empty());
    }

    #[test]
    fn rejects_malformed_curves() {
        for curve in [
            "{ 0, 0, 255 }",
            "{ 0, 0, 255, \"255\" }",
            "{ 0, 0, 255, 300 }",
            "{ 0, 0, 127.5, 128 }",
            "{ 0, 0, { 1, 2 } }",
            "{ x = 0, y = 0 }",
        ] {
            let source = format!(
                "s = {{ value = {{ settings = {{ ToneCurvePV2012 = {} }} }} }}",
                curve
            );
            let error = lrtemplate_to_xmp(&source).unwrap_err();
            assert!(error.contains("ToneCurvePV2012"), "{}: {}", curve, error);
        }
    }

    #[test]
    fn rejects_malformed_files() {
        for source in [
            "",
            "s = ",
            "s = { title = \"unterminated }",
            "s = { value = { settings = { Exposure2012 = 1 } }",
            "s = { value = { settings = { Exposure2012 = os.exit() } } }",
            "s = { value = { settings = { Exposure2012 = 1e999 } } }",
            "s = { value = { settings = { Exposure2012 = 0x10 } } }",
            "s = { value = { settings = { Exposure2012 = 1 } } } print(s)",
            "s = { title = \"no settings\" }",
            "s = { type = \"Metadata\", value = { settings = {} } }",
            "<x:xmpmeta></x:xmpmeta>",
        ] {
            assert!(lrtemplate_to_xmp(source).is_err(), "accepted: {}", source);
        }
    }

    #[test]
    fn bounds_nesting_depth() {
        let deep = format!(
            "s = {{ value = {{ settings = {{ X = {}{} }} }} }}",
            "{".repeat(MAX_DEPTH + 1),
            "}".repeat(MAX_DEPTH + 1)
        );
        let error = lrtemplate_to_xmp(&deep).unwrap_err();
        assert!(error.contains("nested too deeply"), "{}", error);
    }
}
