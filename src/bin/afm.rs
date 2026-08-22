use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: afm <font.afm>");
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
            eprintln!("{}:{}:{}: {}", path, err.line, err.column, err.message);
            ExitCode::from(1)
        }
    }
}
