//! Parses every `.sk` file of a folder with the real parser and prints per file `OK` or the
//! parse errors as `file:line: message`. Script text is never printed.
//!
//! Usage: `cargo run --example load_scripts -- <dir>`

use horizon_skript_core::host::Recorder;
use horizon_skript_core::{Registry, parse};
use std::fs;
use std::path::PathBuf;

fn main() {
    let dir = match std::env::args().nth(1) {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("usage: load_scripts <dir>");
            std::process::exit(2);
        }
    };
    let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(e) => {
            eprintln!("cannot read {}: {e}", dir.display());
            std::process::exit(2);
        }
    };
    files.sort();
    let reg = Registry::new();
    let host = Recorder::default();
    let mut failed = 0usize;
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("sk") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_owned();
        if name.starts_with('-') {
            println!("{name}: disabled, skipped");
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                println!("{name}: cannot read: {e}");
                failed += 1;
                continue;
            }
        };
        match parse::parse_script(&reg, &host, &name, &text) {
            Ok(_) => println!("{name}: OK"),
            Err(errors) => {
                failed += 1;
                println!("{name}: {} error(s)", errors.len());
                for e in errors {
                    println!("  {}:{}: {}", e.file, e.line, e.message);
                }
            }
        }
    }
    println!("files with errors: {failed}");
    if failed > 0 {
        std::process::exit(1);
    }
}
