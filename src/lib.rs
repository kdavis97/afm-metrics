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
    pub glyphs: Vec<GlyphMetric>,
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
}

/// A single glyph's character code, advance width, and PostScript name.
#[derive(Debug, Clone)]
pub struct GlyphMetric {
    pub code: i32,
    pub width: f64,
    pub name: String,
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
    let mut in_char_metrics = false;
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

        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let key_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let key = &trimmed[..key_end];
        let rest = &trimmed[key_end..];
        let value = rest.trim();

        match key {
            "FontName" => metrics.font_name = Some(value.to_string()),
            "FullName" => metrics.full_name = Some(value.to_string()),
            "FamilyName" => metrics.family_name = Some(value.to_string()),
            "StartCharMetrics" => {
                let value_offset = indent + key_end + skip_ws(rest);
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
}
