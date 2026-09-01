# afm-metrics

A parser and CLI for Adobe Font Metrics (AFM) files.

## Why

AFM is a plain-text format for describing a font's glyph widths, kerning,
and general metadata. It's older than most tooling that touches it today,
which means files still get generated or hand-patched by scripts, and those
scripts occasionally emit something slightly wrong: a missing semicolon, a
width that didn't get formatted as a number, a glyph count that doesn't
match the glyphs that follow it.

Most tools that read AFM either accept the file or reject it with something
like "invalid font metrics file." That's not useful when the file is a few
hundred lines long and you don't know which one is broken. This library
tracks line and column through the whole parse, so an error tells you
exactly where to look:

```
broken.afm:14:23: advance width 'WX oops' is not a valid number
```

## Library usage

```rust
let contents = std::fs::read_to_string("Helvetica.afm")?;
match afm_metrics::parse(&contents) {
    Ok(metrics) => {
        println!("{} glyphs", metrics.glyphs.len());
        if let Some(a) = metrics.glyph_named("A") {
            println!("A has advance width {}", a.width);
        }
    }
    Err(err) => {
        eprintln!("Helvetica.afm:{}:{}: {}", err.line, err.column, err.message);
    }
}
```

## CLI usage

```
$ afm Helvetica.afm
font: Helvetica
full name: Helvetica
glyphs: 314
kern pairs: 2705
average width: 532.1
widest glyph: emdash (1000 units)

$ afm broken.afm
broken.afm:14:23: advance width 'WX oops' is not a valid number
```

Pass `--json` for machine-readable output. On success it prints the full
parsed `FontMetrics` as a JSON object; on failure it prints `{"line":
..., "column": ..., "message": ...}` instead of the `path:line:col:
message` string, and the process still exits non-zero:

```
$ afm --json broken.afm
{"line":14,"column":23,"message":"advance width 'WX oops' is not a valid number"}
```

## What's parsed right now

The full set of global font metadata keys: `FontName`, `FullName`,
`FamilyName`, `Weight`, `Version`, `Notice`, `EncodingScheme`,
`ItalicAngle`, `IsFixedPitch`, `FontBBox`, `UnderlinePosition`,
`UnderlineThickness`, `CapHeight`, `XHeight`, `Ascender`, `Descender`,
`StdHW`, and `StdVW`; the full `StartCharMetrics` / `EndCharMetrics` block,
including each glyph's code, advance width, and PostScript name, its `L`
ligature substitutions, and its `CC`/`PCC` composite glyph parts; and the
full `StartKernPairs` / `EndKernPairs` block (`KPX` horizontal kerning
adjustments between glyph name pairs). Unrecognized header keys are skipped
rather than rejected, since a file with fields this library doesn't
understand yet is still a valid file.

## Status

Early. See the issue tracker for what's next.

## License

MIT, see [LICENSE](LICENSE).
