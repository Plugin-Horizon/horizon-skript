//! Loads a real `variables.csv` with the port and writes it back, printing only counts: rows in,
//! rows decoded, raw rows kept, problems, and how many data lines come back unchanged (as a set,
//! since Skript's own order is not kept). No names or values are printed.
//!
//! Usage: `cargo run --example check_variables -- <variables.csv> [fold=true|false]`
use horizon_skript_core::vars::{load_csv, save_csv};
use std::collections::BTreeSet;

fn main() {
    let path = std::env::args().nth(1).expect("usage: check_variables <variables.csv> [fold]");
    let fold = std::env::args().nth(2).is_none_or(|f| f != "false");
    let text = std::fs::read_to_string(&path).expect("read");
    let rows: BTreeSet<&str> =
        text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let loaded = load_csv(&text, fold);
    let (out, skipped) = save_csv(&loaded.vars, &loaded.raw, &|_| true);
    let back: BTreeSet<&str> =
        out.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let null_rows = rows.iter().filter(|l| l.contains(", null,")).count();
    println!("rows in: {} (null rows: {null_rows})", rows.len());
    println!("decoded: {}, kept raw: {}, problems: {}, not encodable on save: {skipped}", loaded.vars.len(), loaded.raw.len(), loaded.problems.len());
    println!("rows out: {}", back.len());
    println!("rows unchanged: {}", rows.intersection(&back).count());
    println!("rows only in original: {}", rows.difference(&back).count());
    println!("rows only in output: {}", back.difference(&rows).count());
    let kinds = |s: &BTreeSet<&str>| -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for l in s { if let Some(k) = l.rsplitn(3, ", ").nth(1) { *m.entry(k.to_owned()).or_insert(0) += 1; } }
        m
    };
    for p in &loaded.problems {
        println!("problem: {p}");
    }
    println!("kinds only-in-original: {:?}", kinds(&rows.difference(&back).copied().collect()));
}
