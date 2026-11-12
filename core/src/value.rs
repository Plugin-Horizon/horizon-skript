//! Runtime values and their text form.

use crate::classes;
use std::sync::atomic::{AtomicU32, Ordering};

/// A Skript value. `List` only exists as the result of a list expression (`{x::*}`, `1, 2 and 3`);
/// variables store single values, a list variable is a set of indexed variables.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Text(String),
    /// Whole numbers (`integer`).
    Int(i64),
    /// Any number (`number`); finite.
    Num(f64),
    Bool(bool),
    /// Milliseconds.
    Timespan(i64),
    /// Milliseconds since the Unix epoch, UTC.
    Date(i64),
    Player {
        uuid: u128,
        name: String,
    },
    World(String),
    Location(Location),
    /// Namespaced item id (`minecraft:stone`), amount and custom name (`named "..."`; None for a
    /// plain item).
    ItemType {
        id: String,
        amount: i64,
        name: Option<String>,
    },
    /// Namespaced entity id (`minecraft:zombie`).
    EntityType(String),
    List(Vec<Value>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub world: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

impl Value {
    /// The class name this value belongs to, as written in patterns (`%number%`).
    pub fn class(&self) -> &'static str {
        match self {
            Value::Null | Value::List(_) => "object",
            Value::Text(_) => "text",
            Value::Int(_) => "integer",
            Value::Num(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Timespan(_) => "timespan",
            Value::Date(_) => "date",
            Value::Player { .. } => "player",
            Value::World(_) => "world",
            Value::Location(_) => "location",
            Value::ItemType { .. } => "item type",
            Value::EntityType(_) => "entity type",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The number this value is, if it is one (numbers, integers, and text that reads as one).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n),
            Value::Text(s) => classes::parse_number(s).map(|v| v.as_f64().unwrap_or(0.0)),
            _ => None,
        }
    }

    /// The whole number this value is, if it is one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
            Value::Text(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Booleans, and the texts `true` and `false`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Text(s) if s.eq_ignore_ascii_case("true") => Some(true),
            Value::Text(s) if s.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }

    /// How the value reads in a message: no `.0` on whole numbers, lists joined with `, `.
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => "<none>".to_owned(),
            Value::Text(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Num(n) => format_number(*n),
            Value::Bool(b) => b.to_string(),
            Value::Timespan(ms) => classes::format_timespan(*ms),
            Value::Date(ms) => classes::format_date(*ms),
            Value::Player { name, .. } => name.clone(),
            Value::World(w) => w.clone(),
            Value::Location(l) => format!(
                "{} at {}, {}, {}",
                l.world,
                format_number(round2(l.x)),
                format_number(round2(l.y)),
                format_number(round2(l.z))
            ),
            Value::ItemType { id, amount, .. } => {
                let name = classes::short_id(id).replace('_', " ");
                if *amount == 1 {
                    name
                } else {
                    format!("{amount} {name}")
                }
            }
            Value::EntityType(id) => classes::short_id(id).replace('_', " "),
            Value::List(items) => items
                .iter()
                .map(Value::to_text)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

static NUMBER_ACCURACY: AtomicU32 = AtomicU32::new(2);

/// The decimals a non-whole number prints with (`number accuracy` in `config.sk`, 2 by default).
pub fn set_number_accuracy(decimals: u32) {
    NUMBER_ACCURACY.store(decimals.min(15), Ordering::SeqCst);
}

pub fn number_accuracy() -> u32 {
    NUMBER_ACCURACY.load(Ordering::SeqCst)
}

/// A number as Skript prints it: `5` for 5.0, `2.5` for 2.5, and at most `number accuracy`
/// decimals with the trailing zeros dropped (`2.50` is `2.5`, `1.999` is `2`).
pub fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return format!("{n}");
    }
    if n.fract() == 0.0 && n.abs() < 1.0e15 {
        return format!("{}", n as i64);
    }
    let fixed = format!("{:.*}", number_accuracy() as usize, n);
    let trimmed = if fixed.contains('.') {
        fixed.trim_end_matches('0').trim_end_matches('.')
    } else {
        fixed.as_str()
    };
    match trimmed {
        "-0" | "" => "0".to_owned(),
        t => t.to_owned(),
    }
}

/// The colour and style names of Skript's tags (`<gold>`, `<bold>`), with their codes.
fn style_code(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "§0",
        "dark blue" => "§1",
        "dark green" => "§2",
        "dark aqua" => "§3",
        "dark red" => "§4",
        "dark purple" => "§5",
        "gold" => "§6",
        "gray" | "grey" => "§7",
        "dark gray" | "dark grey" => "§8",
        "blue" => "§9",
        "green" => "§a",
        "aqua" => "§b",
        "red" => "§c",
        "light purple" => "§d",
        "yellow" => "§e",
        "white" => "§f",
        "obfuscated" => "§k",
        "bold" => "§l",
        "strikethrough" => "§m",
        "underline" => "§n",
        "italic" => "§o",
        "reset" => "§r",
        _ => return None,
    })
}

/// A hex colour as Minecraft's `§x§R§R§G§G§B§B` form, from six hex digits.
fn hex_code(hex: &str) -> Option<String> {
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = String::from("§x");
    for c in hex.to_ascii_lowercase().chars() {
        out.push('§');
        out.push(c);
    }
    Some(out)
}

/// Skript's colour and format tags in a literal text, turned into the section-sign codes the
/// engine prints: `<gold>`, `<bold>`, `</red>` (a closing colour tag resets), `<#RRGGBB>` and
/// `<##RRGGBB>`, `<u:00e9>`, and the ampersand codes `&6`, `&l` (from `&` followed by a code
/// character). Unknown `<...>` and every other character stay as they are.
pub fn format_chat(s: &str) -> String {
    if !s.contains('<') && !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(['<', '>']) {
            Some(close) if after.as_bytes()[close] == b'>' && close > 0 => {
                let tag = &after[..close];
                match tag_code(tag) {
                    Some(code) => out.push_str(&code),
                    None => {
                        out.push('<');
                        out.push_str(tag);
                        out.push('>');
                    }
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    ampersand_codes(&out)
}

fn tag_code(tag: &str) -> Option<String> {
    if let Some(name) = tag.strip_prefix('/') {
        let name = name.to_ascii_lowercase();
        return style_code(&name)
            .filter(|c| *c != "§r")
            .map(|_| "§r".to_owned());
    }
    if let Some(name) = style_code(&tag.to_ascii_lowercase()) {
        return Some(name.to_owned());
    }
    let lower = tag.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("##").or_else(|| lower.strip_prefix('#')) {
        return hex_code(hex);
    }
    let code = lower
        .strip_prefix("u:")
        .or_else(|| lower.strip_prefix("unicode:"))?;
    if code.len() < 4 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(code, 16)
        .ok()
        .and_then(char::from_u32)
        .map(String::from)
}

/// `&` followed by a colour or style character becomes `§` and that character.
fn ampersand_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_hexdigit() || "klmnor".contains(next.to_ascii_lowercase()) {
                    chars.next();
                    out.push('§');
                    out.push(next.to_ascii_lowercase());
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}
