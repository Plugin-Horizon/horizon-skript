//! Variable storage. `{name}`, `{name::sub}` and list variables are all keys of one ordered map:
//! a list is the set of keys below its name, `name::1`, `name::2`, ...
//!
//! Names are case-insensitive by default (`case-insensitive variables` in `config.sk`). A name
//! starting with `-` (`{-cache}`) is memory only: it is never written to `variables.csv`.
//! The CSV functions at the end read and write Skript's flat-file format (`name, type, HEX`).

use crate::value::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Variables {
    map: BTreeMap<String, Value>,
    fold: bool,
    /// Saved changes (sets and deletes of names that are saved) since the last save.
    changes: u64,
    /// Saved names that changed since the variables were loaded.
    touched: BTreeSet<String>,
}

impl Default for Variables {
    fn default() -> Self {
        Self::new()
    }
}

impl Variables {
    /// Empty variables with case-insensitive names.
    pub fn new() -> Self {
        Self::with_case_folding(true)
    }

    /// Empty variables; `fold` says whether names are lower-cased (`case-insensitive variables`).
    pub fn with_case_folding(fold: bool) -> Self {
        Variables {
            map: BTreeMap::new(),
            fold,
            changes: 0,
            touched: BTreeSet::new(),
        }
    }

    fn key<'a>(&self, name: &'a str) -> Cow<'a, str> {
        if self.fold && name.chars().any(char::is_uppercase) {
            Cow::Owned(name.to_lowercase())
        } else {
            Cow::Borrowed(name)
        }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.map.get(self.key(name).as_ref())
    }

    /// Sets one variable. `Null` removes it, as Skript does for `set {x} to nothing`.
    pub fn set(&mut self, name: &str, value: Value) {
        let k = self.key(name).into_owned();
        self.note_change(&k);
        if value.is_null() {
            self.map.remove(&k);
        } else {
            self.map.insert(k, value);
        }
    }

    /// Removes a variable and everything below it (`{x}` takes `{x::1}` with it).
    /// Returns how many values went.
    pub fn delete(&mut self, name: &str) -> usize {
        let name = self.key(name).into_owned();
        let below = format!("{name}::");
        let keys: Vec<String> = self
            .map
            .keys()
            .filter(|k| **k == name || k.starts_with(&below))
            .cloned()
            .collect();
        for k in &keys {
            self.note_change(k);
            self.map.remove(k);
        }
        keys.len()
    }

    /// Removes the values below a name, keeping the name itself. Returns how many went.
    pub fn delete_children(&mut self, base: &str) -> usize {
        let below = format!("{}::", self.key(base));
        let keys: Vec<String> = self
            .map
            .keys()
            .filter(|k| k.starts_with(&below))
            .cloned()
            .collect();
        for k in &keys {
            self.note_change(k);
            self.map.remove(k);
        }
        keys.len()
    }

    /// Removes the first value below a list name that equals `value`. Returns whether one went.
    pub fn remove_matching(&mut self, base: &str, value: &Value) -> bool {
        let below = format!("{}::", self.key(base));
        let key = self
            .map
            .iter()
            .find(|(k, v)| k.starts_with(&below) && *v == value)
            .map(|(k, _)| k.clone());
        match key {
            Some(k) => {
                self.note_change(&k);
                self.map.remove(&k);
                true
            }
            None => false,
        }
    }

    /// Values below a list name (`{x::*}` is `values_below("x")`), in key order.
    pub fn values_below(&self, base: &str) -> Vec<Value> {
        let below = format!("{}::", self.key(base));
        self.map
            .range(below.clone()..)
            .take_while(|(k, _)| k.starts_with(&below))
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// The next free integer index of a list (`1` for an empty list).
    pub fn next_index(&self, base: &str) -> i64 {
        let below = format!("{}::", self.key(base));
        let max = self
            .map
            .range(below.clone()..)
            .take_while(|(k, _)| k.starts_with(&below))
            .filter_map(|(k, _)| k[below.len()..].parse::<i64>().ok())
            .max()
            .unwrap_or(0);
        max + 1
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.map.iter()
    }

    /// Saved changes since the last save (see `variable changes until save`).
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// The saved names changed since the variables were loaded.
    pub fn touched(&self) -> &BTreeSet<String> {
        &self.touched
    }

    /// Called after a save: the changes count from zero again and the saved names are in the file.
    pub fn saved(&mut self) {
        self.changes = 0;
        self.touched.clear();
    }

    /// Records a change of `key`. Memory-only names (`-` first) are not counted.
    fn note_change(&mut self, key: &str) {
        if !key.starts_with('-') {
            self.changes += 1;
            self.touched.insert(key.to_owned());
        }
    }

    /// Stores a value read from a file: no change is counted.
    fn load_one(&mut self, name: String, value: Value) {
        self.map.insert(name, value);
    }
}

/// One line of `variables.csv` this port cannot decode (a type it does not read, such as
/// `date`). It is kept as text and written back unchanged, so a save loses nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawRow {
    pub name: String,
    pub kind: String,
    pub hex: String,
}

/// The result of reading `variables.csv`.
#[derive(Debug, Default)]
pub struct Loaded {
    pub vars: Variables,
    pub raw: Vec<RawRow>,
    /// Lines that could not be read at all (wrong number of fields, bad hex in a known type).
    pub problems: Vec<String>,
}

/// The header Skript writes at the top of the file.
pub const CSV_HEADER: [&str; 4] = [
    "# === Skript's variable storage ===",
    "# Please do not modify this file manually!",
    "#",
    "# version: 2.17.1",
];

/// Reads `variables.csv` text. Lines that start with `#` and blank lines are skipped; a `null`
/// type loads nothing. Values of an unknown type are kept as [`RawRow`]s.
pub fn load_csv(text: &str, fold: bool) -> Loaded {
    let mut out = Loaded {
        vars: Variables::with_case_folding(fold),
        ..Loaded::default()
    };
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = match split_csv(line) {
            Some(f) if f.len() == 3 => f,
            _ => {
                out.problems
                    .push(format!("line {}: not a name, type and value", n + 1));
                continue;
            }
        };
        let [name, kind, hex] = <[String; 3]>::try_from(fields).expect("three fields");
        if kind == "null" {
            continue;
        }
        match decode(&kind, &hex) {
            Some(value) => {
                let key = if fold { name.to_lowercase() } else { name };
                out.vars.load_one(key, value);
            }
            None if is_known_kind(&kind) => {
                out.problems
                    .push(format!("line {}: a {kind} value is not valid hex", n + 1));
            }
            None => out.raw.push(RawRow { name, kind, hex }),
        }
    }
    out
}

/// Writes `variables.csv` text: the header, then every saved value of `vars` in name order, then
/// the raw rows whose name is not in `vars`. Names that `keep` refuses are left out, as are
/// memory-only names. Returns the text and how many values had no encoding.
pub fn save_csv(vars: &Variables, raw: &[RawRow], keep: &dyn Fn(&str) -> bool) -> (String, usize) {
    let mut out = String::new();
    for h in CSV_HEADER {
        out.push_str(h);
        out.push('\n');
    }
    let mut skipped = 0;
    for (name, value) in vars.iter() {
        if name.starts_with('-') || !keep(name) {
            continue;
        }
        match encode(value) {
            Some((kind, hex)) => push_row(&mut out, name, kind, &hex),
            None => skipped += 1,
        }
    }
    for r in raw {
        if r.name.starts_with('-') || !keep(&r.name) || vars.get(&r.name).is_some() {
            continue;
        }
        push_row(&mut out, &r.name, &r.kind, &r.hex);
    }
    (out, skipped)
}

fn push_row(out: &mut String, name: &str, kind: &str, hex: &str) {
    out.push_str(&csv_field(name));
    out.push_str(", ");
    out.push_str(kind);
    out.push_str(", ");
    out.push_str(hex);
    out.push('\n');
}

fn csv_field(v: &str) -> String {
    let needs_quotes =
        v.contains(',') || v.contains('"') || v.contains('#') || v.chars().any(char::is_whitespace);
    if needs_quotes {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_owned()
    }
}

/// Splits one CSV line into its fields. A quoted field may hold commas and doubled quotes.
fn split_csv(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut field = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next()? {
                    '"' if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    '"' => break,
                    c => field.push(c),
                }
            }
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            match chars.next() {
                Some(',') => fields.push(field),
                None => {
                    fields.push(field);
                    return Some(fields);
                }
                Some(_) => return None,
            }
        } else {
            let mut done = false;
            for c in chars.by_ref() {
                if c == ',' {
                    done = true;
                    break;
                }
                field.push(c);
            }
            fields.push(field.trim_end().to_owned());
            if !done {
                return Some(fields);
            }
        }
    }
}

fn is_known_kind(kind: &str) -> bool {
    matches!(kind, "long" | "double" | "boolean" | "string")
}

/// A text as Skript's variable storage writes it: the UTF-8 length as an "unsigned int" of its
/// binary format (two bytes `0x8000 | len` up to 0x7FFF, else four plain bytes), then the bytes.
fn skript_string_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() + 4);
    if b.len() <= 0x7FFF {
        out.extend_from_slice(&(0x8000u16 | b.len() as u16).to_be_bytes());
    } else {
        out.extend_from_slice(&(b.len() as u32).to_be_bytes());
    }
    out.extend_from_slice(b);
    out
}

/// The text of a stored `string` value (see [`skript_string_bytes`]); `None` if the length
/// does not match.
fn skript_string(b: &[u8]) -> Option<String> {
    let (len, rest) = if *b.first()? & 0x80 != 0 {
        let len = u16::from_be_bytes([b[0], *b.get(1)?]) & 0x7FFF;
        (len as usize, &b[2..])
    } else {
        let len = u32::from_be_bytes(b.get(..4)?.try_into().ok()?);
        (len as usize, &b[4..])
    };
    (rest.len() == len).then(|| String::from_utf8(rest.to_vec()).ok())?
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// The CSV type name and the hex of a value, for the types the port writes: `long` (integers),
/// `double` (numbers, 8 bytes big-endian IEEE), `boolean` (one byte, 1 for true) and `string`
/// (UTF-8). Other values have no encoding here.
pub fn encode(v: &Value) -> Option<(&'static str, String)> {
    match v {
        Value::Int(i) => Some(("long", hex_encode(&i.to_be_bytes()))),
        Value::Num(n) => Some(("double", hex_encode(&n.to_bits().to_be_bytes()))),
        Value::Bool(b) => Some(("boolean", hex_encode(&[u8::from(*b)]))),
        Value::Text(s) => Some(("string", hex_encode(&skript_string_bytes(s)))),
        _ => None,
    }
}

/// The value of a CSV type and hex, for the types [`encode`] writes.
pub fn decode(kind: &str, hex: &str) -> Option<Value> {
    let bytes = hex_decode(hex)?;
    match kind {
        "long" => Some(Value::Int(i64::from_be_bytes(bytes.try_into().ok()?))),
        "double" => Some(Value::Num(f64::from_bits(u64::from_be_bytes(
            bytes.try_into().ok()?,
        )))),
        "boolean" => match bytes.as_slice() {
            [0] => Some(Value::Bool(false)),
            [1] => Some(Value::Bool(true)),
            _ => None,
        },
        "string" => skript_string(&bytes).map(Value::Text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_keys_below_a_name() {
        let mut v = Variables::new();
        v.set("list::1", Value::Int(10));
        v.set("list::2", Value::Int(20));
        v.set("listy", Value::Int(99));
        assert_eq!(v.values_below("list"), vec![Value::Int(10), Value::Int(20)]);
        assert_eq!(v.next_index("list"), 3);
        assert_eq!(v.next_index("none"), 1);
    }

    #[test]
    fn delete_takes_children_but_not_siblings() {
        let mut v = Variables::new();
        v.set("a", Value::Int(1));
        v.set("a::b", Value::Int(2));
        v.set("ab", Value::Int(3));
        assert_eq!(v.delete("a"), 2);
        assert_eq!(v.get("ab"), Some(&Value::Int(3)));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn null_removes() {
        let mut v = Variables::new();
        v.set("x", Value::Int(1));
        v.set("x", Value::Null);
        assert!(v.is_empty());
    }

    #[test]
    fn names_are_case_insensitive_by_default() {
        let mut v = Variables::new();
        v.set("Npc::Assigned", Value::Text("a".into()));
        assert_eq!(v.get("npc::assigned"), Some(&Value::Text("a".into())));
        v.set("list::1", Value::Int(4));
        assert_eq!(v.values_below("LIST"), vec![Value::Int(4)]);
        let mut exact = Variables::with_case_folding(false);
        exact.set("Mixed", Value::Int(1));
        assert_eq!(exact.get("mixed"), None);
        assert_eq!(exact.get("Mixed"), Some(&Value::Int(1)));
    }

    #[test]
    fn changes_count_saved_names_only() {
        let mut v = Variables::new();
        v.set("-cache", Value::Int(1));
        assert_eq!(v.changes(), 0);
        v.set("kept", Value::Int(1));
        v.delete("kept");
        assert_eq!(v.changes(), 2);
        assert!(v.touched().contains("kept"));
        v.saved();
        assert_eq!(v.changes(), 0);
        assert!(v.touched().is_empty());
    }

    #[test]
    fn strings_carry_skripts_length_prefix() {
        // "frost" as Skript 2.17 stores it: 0x8005 then the five UTF-8 bytes.
        assert_eq!(decode("string", "800566726F7374"), Some(Value::Text("frost".into())));
        assert_eq!(encode(&Value::Text("frost".into())), Some(("string", "800566726F7374".into())));
        assert_eq!(decode("string", "66726F7374"), None, "raw bytes without the prefix are not Skript's format");
        let long = "x".repeat(0x8000);
        let (_, hex) = encode(&Value::Text(long.clone())).unwrap();
        assert!(hex.starts_with("00008000"));
        assert_eq!(decode("string", &hex), Some(Value::Text(long)));
    }

    #[test]
    fn csv_round_trip_of_the_known_types() {
        let mut v = Variables::new();
        v.set("n::long", Value::Int(-42));
        v.set("n::double", Value::Num(2.5));
        v.set("n::yes", Value::Bool(true));
        v.set("n::text", Value::Text("hello, \"world\"".into()));
        v.set("-memory", Value::Int(7));
        let (text, skipped) = save_csv(&v, &[], &|_| true);
        assert_eq!(skipped, 0);
        assert!(text.starts_with("# === Skript's variable storage ===\n"));
        assert!(text.contains("# version: 2.17.1\n"));
        assert!(!text.contains("memory"));
        // Integers: 8 bytes big-endian; doubles: IEEE bits; booleans: one byte.
        assert!(text.contains("n::long, long, FFFFFFFFFFFFFFD6\n"));
        assert!(text.contains("n::double, double, 4004000000000000\n"));
        assert!(text.contains("n::yes, boolean, 01\n"));
        let back = load_csv(&text, true);
        assert!(back.problems.is_empty());
        assert_eq!(back.vars.get("n::long"), Some(&Value::Int(-42)));
        assert_eq!(back.vars.get("n::double"), Some(&Value::Num(2.5)));
        assert_eq!(back.vars.get("n::yes"), Some(&Value::Bool(true)));
        assert_eq!(
            back.vars.get("n::text"),
            Some(&Value::Text("hello, \"world\"".into()))
        );
        assert_eq!(back.vars.get("-memory"), None);
    }

    #[test]
    fn unknown_types_are_kept_raw_and_written_back() {
        let text = "# === Skript's variable storage ===\n# version: 2.17.1\n\
                    a::when, date, 00ABCDEF\n\
                    a::num, long, 0000000000000001\n";
        let loaded = load_csv(text, true);
        assert_eq!(loaded.raw.len(), 1);
        assert_eq!(loaded.vars.get("a::num"), Some(&Value::Int(1)));
        let (out, _) = save_csv(&loaded.vars, &loaded.raw, &|_| true);
        assert!(out.contains("a::when, date, 00ABCDEF\n"));
        // A raw row whose name was set again is replaced by the value.
        let mut changed = loaded.vars.clone();
        changed.set("a::when", Value::Int(5));
        let (out2, _) = save_csv(&changed, &loaded.raw, &|_| true);
        assert!(!out2.contains("date"));
    }

    #[test]
    fn csv_lines_with_bad_fields_are_reported() {
        let loaded = load_csv("only, two\nname, long, ZZ\nok, null, \n", true);
        assert_eq!(loaded.problems.len(), 2);
        assert!(loaded.vars.is_empty());
    }

    #[test]
    fn quoted_names_split_at_their_commas() {
        assert_eq!(
            split_csv("\"a, b\", string, 41"),
            Some(vec!["a, b".to_owned(), "string".to_owned(), "41".to_owned()])
        );
        assert_eq!(split_csv("\"open, x"), None);
    }
}
