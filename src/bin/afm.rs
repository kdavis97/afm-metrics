use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("glyph") {
        args.next();
        run_glyph(args)
    } else {
        run_summary(args)
    }
}

fn run_summary(args: impl Iterator<Item = String>) -> ExitCode {
    let mut json = false;
    let mut path: Option<String> = None;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("usage: afm [--json] <font.afm>");
            return ExitCode::from(2);
        }
    }
    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("usage: afm [--json] <font.afm>");
            return ExitCode::from(2);
        }
    };

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("{}: {}", path, err);
            return ExitCode::from(1);
        }
    };

    match afm_metrics::parse(&contents) {
        Ok(metrics) => {
            if json {
                println!("{}", metrics.to_json());
                return ExitCode::SUCCESS;
            }
            println!("font: {}", metrics.font_name.as_deref().unwrap_or("(unknown)"));
            println!("full name: {}", metrics.full_name.as_deref().unwrap_or("(unknown)"));
            println!("glyphs: {}", metrics.glyphs.len());
            println!("kern pairs: {}", metrics.kern_pairs.len());
            if let Some(angle) = metrics.italic_angle {
                println!("italic angle: {}", angle);
            }
            if let Some(bbox) = &metrics.font_bbox {
                println!("bounding box: [{} {} {} {}]", bbox.llx, bbox.lly, bbox.urx, bbox.ury);
            }
            if let Some(avg) = metrics.average_width() {
                println!("average width: {:.1}", avg);
            }
            if let Some(widest) = metrics.widest_glyph() {
                println!("widest glyph: {} ({} units)", widest.name, widest.width);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                println!("{}", err.to_json());
            } else {
                eprintln!("{}:{}:{}: {}", path, err.line, err.column, err.message);
            }
            ExitCode::from(1)
        }
    }
}

/// `afm glyph [--json] <font.afm> <name-or-code>`
///
/// The query is tried as a glyph name first, since names are the more
/// common way to ask for a glyph and names that happen to look like
/// integers (there aren't any in practice) would otherwise be ambiguous.
fn run_glyph(args: impl Iterator<Item = String>) -> ExitCode {
    let mut json = false;
    let mut path: Option<String> = None;
    let mut query: Option<String> = None;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg);
        } else if query.is_none() {
            query = Some(arg);
        } else {
            eprintln!("usage: afm glyph [--json] <font.afm> <name-or-code>");
            return ExitCode::from(2);
        }
    }
    let (path, query) = match (path, query) {
        (Some(p), Some(q)) => (p, q),
        _ => {
            eprintln!("usage: afm glyph [--json] <font.afm> <name-or-code>");
            return ExitCode::from(2);
        }
    };

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("{}: {}", path, err);
            return ExitCode::from(1);
        }
    };

    let metrics = match afm_metrics::parse(&contents) {
        Ok(m) => m,
        Err(err) => {
            if json {
                println!("{}", err.to_json());
            } else {
                eprintln!("{}:{}:{}: {}", path, err.line, err.column, err.message);
            }
            return ExitCode::from(1);
        }
    };

    let glyph = metrics
        .glyph_named(&query)
        .or_else(|| query.parse::<i32>().ok().and_then(|code| metrics.glyph_by_code(code)));

    match glyph {
        Some(g) => {
            if json {
                println!("{}", g.to_json());
            } else {
                println!("name: {}", g.name);
                println!("code: {}", g.code);
                println!("width: {}", g.width);
                for l in &g.ligatures {
                    println!("ligature: {} + {} -> {}", g.name, l.successor, l.ligature);
                }
                for p in &g.composite_parts {
                    println!("part: {} at ({}, {})", p.name, p.x, p.y);
                }
            }
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("{}: no glyph named or coded '{}'", path, query);
            ExitCode::from(1)
        }
    }
}
