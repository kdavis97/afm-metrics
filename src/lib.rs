//! Parsing and inspection of Adobe Font Metrics (AFM) files.
//!
//! AFM is a plain-text format that predates most modern font tooling and is
//! still produced by some PDF and PostScript pipelines. Files are hand
//! written or hand patched often enough that a parser needs to say exactly
//! where it gave up, not just that it gave up.

use std::fmt;

/// A parsed AFM file's font-level metadata and per-glyph metrics.
#[derive(Debug, Default, Clone)]
pub struct FontMetrics {
    pub font_name: Option<String>,
    pub full_name: Option<String>,
    pub family_name: Option<String>,
    pub weight: Option<String>,
    pub version: Option<String>,
    pub notice: Option<String>,
    pub encoding_scheme: Option<String>,
    pub italic_angle: Option<f64>,
    pub is_fixed_pitch: Option<bool>,
    pub font_bbox: Option<FontBBox>,
    pub underline_position: Option<f64>,
    pub underline_thickness: Option<f64>,
    pub cap_height: Option<f64>,
    pub x_height: Option<f64>,
    pub ascender: Option<f64>,
    pub descender: Option<f64>,
    pub std_hw: Option<f64>,
    pub std_vw: Option<f64>,
    pub glyphs: Vec<GlyphMetric>,
    pub kern_pairs: Vec<KernPair>,
}

impl FontMetrics {
    /// Mean advance width across all glyphs, in the font's design units.
    pub fn average_width(&self) -> Option<f64> {
        if self.glyphs.is_empty() {
            return None;
        }
        let total: f64 = self.glyphs.iter().map(|g| g.width).sum();
        Some(total / self.glyphs.len() as f64)
    }

    /// The glyph with the largest advance width, if any glyphs were parsed.
    pub fn widest_glyph(&self) -> Option<&GlyphMetric> {
        self.glyphs
            .iter()
            .max_by(|a, b| a.width.partial_cmp(&b.width).unwrap())
    }

    /// Look up a glyph by its PostScript name (e.g. "space", "A").
    pub fn glyph_named(&self, name: &str) -> Option<&GlyphMetric> {
        self.glyphs.iter().find(|g| g.name == name)
    }

    /// The horizontal kerning adjustment between two glyph names, if the
    /// pair appears in the file's `StartKernPairs` block.
    pub fn kerning_between(&self, first: &str, second: &str) -> Option<f64> {
        self.kern_pairs
            .iter()
            .find(|k| k.first == first && k.second == second)
            .map(|k| k.adjustment)
    }
}

/// The font's bounding box, as declared by a `FontBBox` header line: the
/// lower-left and upper-right corners of the smallest rectangle enclosing
/// every glyph, in the font's design units.
#[derive(Debug, Clone, Copy)]
pub struct FontBBox {
    pub llx: f64,
    pub lly: f64,
    pub urx: f64,
    pub ury: f64,
}

/// A single glyph's character code, advance width, and PostScript name.
#[derive(Debug, Clone)]
pub struct GlyphMetric {
    pub code: i32,
    pub width: f64,
    pub name: String,
}

/// A single horizontal kerning adjustment between two glyphs, as found in a
/// `KPX` line within a `StartKernPairs` / `EndKernPairs` block.
#[derive(Debug, Clone)]
pub struct KernPair {
    pub first: String,
    pub second: String,
    pub adjustment: f64,
}

/// An error produced while parsing an AFM file, located to a line and
/// column so a caller can point a user at the exact spot that's wrong
/// instead of just saying the file failed to parse.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl ParseError {
    fn new(line: usize, column: usize, message: impl Into<String>) -> Self {
        ParseError { line, column, message: message.into() }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse the contents of an AFM file.
///
/// On failure this returns the first error encountered, carrying the line
/// and column of the offending token rather than a generic "malformed
/// file" message.
pub fn parse(input: &str) -> Result<FontMetrics, ParseError> {
    let mut metrics = FontMetrics::default();
    let mut lines = input.lines().enumerate();

    let (first_line_no, first_line) = loop {
        match lines.next() {
            Some((_, l)) if l.trim().is_empty() => continue,
            Some((i, l)) => break (i + 1, l),
            None => {
                return Err(ParseError::new(1, 1, "file is empty; expected 'StartFontMetrics'"))
            }
        }
    };
    if !first_line.trim_start().starts_with("StartFontMetrics") {
        return Err(ParseError::new(
            first_line_no,
            1,
            format!(
                "expected 'StartFontMetrics' as the first line, found '{}'",
                first_line.trim()
            ),
        ));
    }

    let mut declared_glyph_count: Option<usize> = None;
    let mut declared_kern_count: Option<usize> = None;
    let mut in_char_metrics = false;
    let mut in_kern_pairs = false;
    let mut saw_end_font_metrics = false;
    let mut last_line_no = first_line_no;

    for (i, raw_line) in lines {
        let line_no = i + 1;
        last_line_no = line_no;
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            continue;
        }

        if in_char_metrics {
            if line.trim_start().starts_with("EndCharMetrics") {
                in_char_metrics = false;
                if let Some(expected) = declared_glyph_count {
                    if metrics.glyphs.len() != expected {
                        return Err(ParseError::new(
                            line_no,
                            1,
                            format!(
                                "StartCharMetrics declared {} glyphs but {} were found",
                                expected,
                                metrics.glyphs.len()
                            ),
                        ));
                    }
                }
                continue;
            }
            metrics.glyphs.push(parse_char_metrics_line(line, line_no)?);
            continue;
        }

        if in_kern_pairs {
            if line.trim_start().starts_with("EndKernPairs") {
                in_kern_pairs = false;
                if let Some(expected) = declared_kern_count {
                    if metrics.kern_pairs.len() != expected {
                        return Err(ParseError::new(
                            line_no,
                            1,
                            format!(
                                "StartKernPairs declared {} pairs but {} were found",
                                expected,
                                metrics.kern_pairs.len()
                            ),
                        ));
                    }
                }
                continue;
            }
            metrics.kern_pairs.push(parse_kern_pair_line(line, line_no)?);
            continue;
        }

        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let key_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let key = &trimmed[..key_end];
        let rest = &trimmed[key_end..];
        let value = rest.trim();
        let value_offset = indent + key_end + skip_ws(rest);

        match key {
            "FontName" => metrics.font_name = Some(value.to_string()),
            "FullName" => metrics.full_name = Some(value.to_string()),
            "FamilyName" => metrics.family_name = Some(value.to_string()),
            "Weight" => metrics.weight = Some(value.to_string()),
            "Version" => metrics.version = Some(value.to_string()),
            "Notice" => metrics.notice = Some(value.to_string()),
            "EncodingScheme" => metrics.encoding_scheme = Some(value.to_string()),
            "ItalicAngle" => {
                metrics.italic_angle = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "UnderlinePosition" => {
                metrics.underline_position =
                    Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "UnderlineThickness" => {
                metrics.underline_thickness =
                    Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "CapHeight" => {
                metrics.cap_height = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "XHeight" => {
                metrics.x_height = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "Ascender" => {
                metrics.ascender = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "Descender" => {
                metrics.descender = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "StdHW" => {
                metrics.std_hw = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "StdVW" => {
                metrics.std_vw = Some(parse_number_field(key, value, line, value_offset, line_no)?)
            }
            "IsFixedPitch" => {
                metrics.is_fixed_pitch = Some(value.parse::<bool>().map_err(|_| {
                    ParseError::new(
                        line_no,
                        char_column(line, value_offset),
                        format!("'IsFixedPitch' value '{}' is not 'true' or 'false'", value),
                    )
                })?);
            }
            "FontBBox" => {
                metrics.font_bbox = Some(parse_font_bbox(line, value_offset, line_no)?);
            }
            "StartCharMetrics" => {
                let count = value.parse::<usize>().map_err(|_| {
                    ParseError::new(
                        line_no,
                        char_column(line, value_offset),
                        format!(
                            "expected a glyph count after 'StartCharMetrics', found '{}'",
                            value
                        ),
                    )
                })?;
                declared_glyph_count = Some(count);
                in_char_metrics = true;
            }
            "StartKernPairs" => {
                let count = value.parse::<usize>().map_err(|_| {
                    ParseError::new(
                        line_no,
                        char_column(line, value_offset),
                        format!(
                            "expected a pair count after 'StartKernPairs', found '{}'",
                            value
                        ),
                    )
                })?;
                declared_kern_count = Some(count);
                in_kern_pairs = true;
            }
            "EndFontMetrics" => {
                saw_end_font_metrics = true;
                break;
            }
            _ => {}
        }
    }

    if !saw_end_font_metrics {
        return Err(ParseError::new(
            last_line_no,
            1,
            "file is missing a closing 'EndFontMetrics' line",
        ));
    }

    Ok(metrics)
}

/// Byte offset of the first non-whitespace character in `s`.
fn skip_ws(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

/// Convert a byte offset within `line` to a 1-based column number.
fn char_column(line: &str, byte_offset: usize) -> usize {
    line[..byte_offset.min(line.len())].chars().count() + 1
}

/// Parse a single-value numeric header field (e.g. `ItalicAngle -12.5`),
/// producing an error located at the value's column if it isn't a number.
fn parse_number_field(
    key: &str,
    value: &str,
    line: &str,
    value_offset: usize,
    line_no: usize,
) -> Result<f64, ParseError> {
    value.parse::<f64>().map_err(|_| {
        ParseError::new(
            line_no,
            char_column(line, value_offset),
            format!("'{}' value '{}' is not a valid number", key, value),
        )
    })
}

/// Parse a `FontBBox` header line's four numbers (llx, lly, urx, ury).
fn parse_font_bbox(line: &str, start: usize, line_no: usize) -> Result<FontBBox, ParseError> {
    const FIELDS: [&str; 4] = ["llx", "lly", "urx", "ury"];
    let mut values = [0.0f64; 4];
    let mut offset = start;
    for (i, field_name) in FIELDS.iter().enumerate() {
        let (tok_start, tok) = next_token(line, offset).ok_or_else(|| {
            ParseError::new(
                line_no,
                char_column(line, line.len()),
                format!("'FontBBox' is missing its {} value", field_name),
            )
        })?;
        values[i] = tok.parse::<f64>().map_err(|_| {
            ParseError::new(
                line_no,
                char_column(line, tok_start),
                format!("'FontBBox' {} value '{}' is not a valid number", field_name, tok),
            )
        })?;
        offset = tok_start + tok.len();
    }
    Ok(FontBBox { llx: values[0], lly: values[1], urx: values[2], ury: values[3] })
}

fn parse_char_metrics_line(line: &str, line_no: usize) -> Result<GlyphMetric, ParseError> {
    let mut code: Option<i32> = None;
    let mut width: Option<f64> = None;
    let mut name: Option<String> = None;

    let mut byte_offset = 0usize;
    for segment in line.split(';') {
        let seg_start = byte_offset;
        byte_offset += segment.len() + 1;

        let field = segment.trim();
        if field.is_empty() {
            continue;
        }
        let field_start = seg_start + skip_ws(segment);
        let field_column = char_column(line, field_start);

        let key_end = field.find(char::is_whitespace).unwrap_or(field.len());
        let key = &field[..key_end];
        let value = field[key_end..].trim();

        match key {
            "C" => {
                code = Some(value.parse::<i32>().map_err(|_| {
                    ParseError::new(
                        line_no,
                        field_column,
                        format!("glyph code 'C {}' is not a valid integer", value),
                    )
                })?);
            }
            "WX" => {
                width = Some(value.parse::<f64>().map_err(|_| {
                    ParseError::new(
                        line_no,
                        field_column,
                        format!("advance width 'WX {}' is not a valid number", value),
                    )
                })?);
            }
            "N" => {
                if value.is_empty() {
                    return Err(ParseError::new(
                        line_no,
                        field_column,
                        "glyph name field 'N' has no value",
                    ));
                }
                name = Some(value.to_string());
            }
            _ => {}
        }
    }

    let end_column = char_column(line, line.len());
    let code = code.ok_or_else(|| {
        ParseError::new(line_no, end_column, "char metrics line is missing a 'C' (code) field")
    })?;
    let width = width.ok_or_else(|| {
        ParseError::new(line_no, end_column, "char metrics line is missing a 'WX' (width) field")
    })?;
    let name = name.ok_or_else(|| {
        ParseError::new(line_no, end_column, "char metrics line is missing an 'N' (name) field")
    })?;

    Ok(GlyphMetric { code, width, name })
}

/// Byte offset and text of the next whitespace-delimited token in `s`
/// starting at or after `from`, if any remain.
fn next_token(s: &str, from: usize) -> Option<(usize, &str)> {
    let rest = &s[from..];
    let start = from + skip_ws(rest);
    if start >= s.len() {
        return None;
    }
    let after = &s[start..];
    let end = after.find(char::is_whitespace).unwrap_or(after.len());
    Some((start, &s[start..start + end]))
}

fn parse_kern_pair_line(line: &str, line_no: usize) -> Result<KernPair, ParseError> {
    let mut tokens = Vec::new();
    let mut offset = 0usize;
    while let Some((start, tok)) = next_token(line, offset) {
        offset = start + tok.len();
        tokens.push((start, tok));
        if tokens.len() == 4 {
            break;
        }
    }

    match tokens.first() {
        Some(&(_, "KPX")) => {}
        _ => {
            return Err(ParseError::new(
                line_no,
                1,
                format!("expected a 'KPX' kerning pair, found '{}'", line.trim()),
            ));
        }
    }

    if tokens.len() < 4 {
        let end_column = char_column(line, line.len());
        return Err(ParseError::new(
            line_no,
            end_column,
            "'KPX' line is missing the second glyph name or an adjustment value",
        ));
    }

    let first = tokens[1].1.to_string();
    let second = tokens[2].1.to_string();
    let (adj_start, adj_tok) = tokens[3];
    let adjustment = adj_tok.parse::<f64>().map_err(|_| {
        ParseError::new(
            line_no,
            char_column(line, adj_start),
            format!("kerning adjustment '{}' is not a valid number", adj_tok),
        )
    })?;

    Ok(KernPair { first, second, adjustment })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "StartFontMetrics 4.1\nFontName Helvetica\nFullName Helvetica\nStartCharMetrics 2\nC 32 ; WX 278 ; N space ;\nC 65 ; WX 667 ; N A ;\nEndCharMetrics\nEndFontMetrics\n";

    #[test]
    fn parses_a_well_formed_file() {
        let metrics = parse(SAMPLE).expect("sample should parse");
        assert_eq!(metrics.font_name.as_deref(), Some("Helvetica"));
        assert_eq!(metrics.glyphs.len(), 2);
        assert_eq!(metrics.glyph_named("A").unwrap().width, 667.0);
    }

    #[test]
    fn rejects_missing_header() {
        let err = parse("FontName Helvetica\n").unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn reports_bad_width_with_location() {
        let input = "StartFontMetrics 4.1\nStartCharMetrics 1\nC 65 ; WX abc ; N A ;\nEndCharMetrics\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 3);
        // "C 65 ; " is 7 characters, so the WX field starts at column 8.
        assert_eq!(err.column, 8);
    }

    #[test]
    fn reports_glyph_count_mismatch() {
        let input = "StartFontMetrics 4.1\nStartCharMetrics 2\nC 65 ; WX 667 ; N A ;\nEndCharMetrics\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 4);
    }

    #[test]
    fn parses_kern_pairs() {
        let input = "StartFontMetrics 4.1\nStartKernPairs 2\nKPX A V -70\nKPX A W -50\nEndKernPairs\nEndFontMetrics\n";
        let metrics = parse(input).expect("sample should parse");
        assert_eq!(metrics.kern_pairs.len(), 2);
        assert_eq!(metrics.kerning_between("A", "V"), Some(-70.0));
        assert_eq!(metrics.kerning_between("A", "Z"), None);
    }

    #[test]
    fn reports_bad_kerning_adjustment_with_location() {
        let input = "StartFontMetrics 4.1\nStartKernPairs 1\nKPX A V oops\nEndKernPairs\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 3);
        // "KPX A V " is 8 characters, so the adjustment token starts at column 9.
        assert_eq!(err.column, 9);
    }

    #[test]
    fn reports_kern_pair_count_mismatch() {
        let input = "StartFontMetrics 4.1\nStartKernPairs 2\nKPX A V -70\nEndKernPairs\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 4);
    }

    #[test]
    fn reports_kern_line_missing_kpx() {
        let input = "StartFontMetrics 4.1\nStartKernPairs 1\nKP A V -70 0\nEndKernPairs\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 3);
        assert_eq!(err.column, 1);
    }

    #[test]
    fn parses_header_fields() {
        let input = "StartFontMetrics 4.1\nWeight Bold\nItalicAngle -12.5\nIsFixedPitch false\nFontBBox -166 -225 1000 931\nUnderlinePosition -100\nUnderlineThickness 50\nCapHeight 718\nXHeight 523\nAscender 718\nDescender -207\nStdHW 80\nStdVW 88\nEndFontMetrics\n";
        let metrics = parse(input).expect("sample should parse");
        assert_eq!(metrics.weight.as_deref(), Some("Bold"));
        assert_eq!(metrics.italic_angle, Some(-12.5));
        assert_eq!(metrics.is_fixed_pitch, Some(false));
        let bbox = metrics.font_bbox.expect("bbox should be set");
        assert_eq!((bbox.llx, bbox.lly, bbox.urx, bbox.ury), (-166.0, -225.0, 1000.0, 931.0));
        assert_eq!(metrics.underline_position, Some(-100.0));
        assert_eq!(metrics.underline_thickness, Some(50.0));
        assert_eq!(metrics.cap_height, Some(718.0));
        assert_eq!(metrics.x_height, Some(523.0));
        assert_eq!(metrics.ascender, Some(718.0));
        assert_eq!(metrics.descender, Some(-207.0));
        assert_eq!(metrics.std_hw, Some(80.0));
        assert_eq!(metrics.std_vw, Some(88.0));
    }

    #[test]
    fn reports_bad_italic_angle_with_location() {
        let input = "StartFontMetrics 4.1\nItalicAngle sideways\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 2);
        // "ItalicAngle " is 12 characters, so the value starts at column 13.
        assert_eq!(err.column, 13);
    }

    #[test]
    fn reports_incomplete_font_bbox() {
        let input = "StartFontMetrics 4.1\nFontBBox -166 -225 1000\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn reports_bad_is_fixed_pitch() {
        let input = "StartFontMetrics 4.1\nIsFixedPitch yes\nEndFontMetrics\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, 2);
        // "IsFixedPitch " is 13 characters, so the value starts at column 14.
        assert_eq!(err.column, 14);
    }
}
