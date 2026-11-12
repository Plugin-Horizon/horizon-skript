//! The types Skript patterns name (`%number%`, `%players%`, ...): parsing from text, text form,
//! static compatibility between expression results and pattern slots, and run-time conversion.

use crate::host::Host;
use crate::value::{Location, Value};

/// The singular class names the core knows. `object` accepts anything.
pub const CLASS_NAMES: &[&str] = &[
    "object",
    "text",
    "number",
    "integer",
    "boolean",
    "timespan",
    "date",
    "player",
    "world",
    "location",
    "item type",
    "entity type",
];

/// A type as a pattern names it: `number`, `numbers` (a list), `item types`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeName {
    pub class: &'static str,
    pub plural: bool,
}

/// Resolves a pattern type name, singular or plural. `None` for unknown names.
pub fn resolve(name: &str) -> Option<TypeName> {
    let n = name.trim().to_ascii_lowercase();
    for class in CLASS_NAMES {
        if n == *class {
            return Some(TypeName {
                class,
                plural: false,
            });
        }
        // `numbers`, `item types`, `players`: the plural adds `s` to the singular name.
        if *class != "object" && n == format!("{class}s") {
            return Some(TypeName {
                class,
                plural: true,
            });
        }
    }
    if n == "objects" {
        return Some(TypeName {
            class: "object",
            plural: true,
        });
    }
    None
}

/// Whether an expression of class `have` can fill a slot of class `want` (both singular).
/// Text takes any value (it is the text form), numbers take integers, objects take everything.
pub fn accepts(want: &str, have: &str) -> bool {
    want == "object" || want == have || want == "text" || (want == "number" && have == "integer")
}

/// Parses a single value of the given singular class from text (a pattern's literal input, or
/// text a player typed). `player` looks the name up through the host.
pub fn parse_value(class: &str, text: &str, host: &dyn Host) -> Option<Value> {
    let t = text.trim();
    match class {
        "text" => Some(Value::Text(text.to_owned())),
        "number" => parse_number(t),
        "integer" => t.parse::<i64>().ok().map(Value::Int),
        "boolean" => match t.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        "timespan" => parse_timespan(t).map(Value::Timespan),
        "date" => parse_date(t).map(Value::Date),
        "player" => host
            .player_by_name(t)
            .map(|(uuid, name)| Value::Player { uuid, name }),
        "world" => {
            if !t.is_empty() && !t.contains(char::is_whitespace) {
                Some(Value::World(t.to_owned()))
            } else {
                None
            }
        }
        "location" => parse_location(t),
        "item type" => {
            let (amount, rest) = split_amount(t);
            let id = normalize_id(rest)?;
            host.is_item_type(&id)
                .then(|| Value::ItemType { id, amount })
        }
        "entity type" => {
            let id = normalize_id(t)?;
            host.is_entity_type(&id).then(|| Value::EntityType(id))
        }
        _ => None,
    }
}

/// Converts a run-time value to a slot's class: text forms, numeric forms, and parsing of text.
pub fn convert(v: Value, want: &str, host: &dyn Host) -> Option<Value> {
    if want == "object" || v.class() == want {
        return Some(v);
    }
    match want {
        "text" => Some(Value::Text(v.to_text())),
        "number" => match v {
            Value::Int(_) | Value::Num(_) => Some(v),
            Value::Text(ref s) => parse_number(s.trim()),
            _ => None,
        },
        "integer" => v.as_i64().map(Value::Int),
        "boolean" => v.as_bool().map(Value::Bool),
        _ => match v {
            Value::Text(ref s) => parse_value(want, s, host),
            _ => None,
        },
    }
}

/// A number literal: `5` is an integer, `2.5` a number. Rejects `nan`, `inf` and the like.
pub fn parse_number(t: &str) -> Option<Value> {
    if t.is_empty()
        || !t
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
    {
        return None;
    }
    if !t.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return None;
    }
    if let Ok(i) = t.parse::<i64>() {
        return Some(Value::Int(i));
    }
    let f: f64 = t.parse().ok()?;
    f.is_finite().then_some(Value::Num(f))
}

const MS_PER_TICK: f64 = 50.0;

/// A timespan: one or more `<number> <unit>` pairs, `1 minute, 30 seconds` or `2 hours and 5 minutes`.
pub fn parse_timespan(t: &str) -> Option<i64> {
    let words: Vec<String> = t
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case("and"))
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() || words.len() % 2 != 0 {
        return None;
    }
    let mut total = 0.0;
    for pair in words.chunks(2) {
        let n: f64 = pair[0].parse().ok()?;
        if !n.is_finite() || n < 0.0 {
            return None;
        }
        let unit_ms = match pair[1].trim_end_matches('s') {
            "millisecond" | "ms" => 1.0,
            "tick" => MS_PER_TICK,
            "second" => 1000.0,
            "minute" => 60_000.0,
            "hour" => 3_600_000.0,
            "day" => 86_400_000.0,
            "week" => 604_800_000.0,
            _ => return None,
        };
        total += n * unit_ms;
    }
    Some(total.round() as i64)
}

/// `1 second`, `2 minutes, 30 seconds`; milliseconds when nothing larger fits.
pub fn format_timespan(ms: i64) -> String {
    let units: [(i64, &str); 6] = [
        (604_800_000, "week"),
        (86_400_000, "day"),
        (3_600_000, "hour"),
        (60_000, "minute"),
        (1000, "second"),
        (1, "millisecond"),
    ];
    let mut rest = ms.abs();
    let mut parts = Vec::new();
    for (size, name) in units {
        let n = rest / size;
        rest %= size;
        if n > 0 {
            parts.push(format!("{n} {name}{}", if n == 1 { "" } else { "s" }));
        }
    }
    if parts.is_empty() {
        "0 seconds".to_owned()
    } else {
        parts.join(", ")
    }
}

/// `YYYY-MM-DD` or `YYYY-MM-DD HH:MM[:SS]`, UTC.
pub fn parse_date(t: &str) -> Option<i64> {
    let (day, time) = match t.split_once(' ') {
        Some((d, tm)) => (d, Some(tm)),
        None => (t, None),
    };
    let mut it = day.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut ms = days_from_civil(y, m, d) * 86_400_000;
    if let Some(tm) = time {
        let p: Vec<&str> = tm.split(':').collect();
        if p.len() < 2 || p.len() > 3 {
            return None;
        }
        let h: i64 = p[0].parse().ok()?;
        let mi: i64 = p[1].parse().ok()?;
        let s: i64 = if p.len() == 3 { p[2].parse().ok()? } else { 0 };
        if !(0..24).contains(&h) || !(0..60).contains(&mi) || !(0..60).contains(&s) {
            return None;
        }
        ms += (h * 3600 + mi * 60 + s) * 1000;
    }
    Some(ms)
}

/// `YYYY-MM-DD HH:MM:SS` in UTC.
pub fn format_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let secs = ms.rem_euclid(86_400_000) / 1000;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `world x y z` (yaw and pitch 0).
fn parse_location(t: &str) -> Option<Value> {
    let p: Vec<&str> = t.split_whitespace().collect();
    if p.len() != 4 {
        return None;
    }
    let x: f64 = p[1].parse().ok()?;
    let y: f64 = p[2].parse().ok()?;
    let z: f64 = p[3].parse().ok()?;
    Some(Value::Location(Location {
        world: p[0].to_owned(),
        x,
        y,
        z,
        yaw: 0.0,
        pitch: 0.0,
    }))
}

/// `3 stone` gives (3, "stone"); `stone` gives (1, "stone").
fn split_amount(t: &str) -> (i64, &str) {
    if let Some((first, rest)) = t.split_once(' ') {
        if let Ok(n) = first.parse::<i64>() {
            if n >= 1 {
                return (n, rest.trim());
            }
        }
    }
    (1, t)
}

/// `Diamond Sword` becomes `minecraft:diamond_sword`. `None` for names with other characters.
pub fn normalize_id(t: &str) -> Option<String> {
    let lower = t.trim().to_ascii_lowercase().replace(' ', "_");
    if lower.is_empty() {
        return None;
    }
    let (ns, path) = match lower.split_once(':') {
        Some((ns, path)) => (ns.to_owned(), path.to_owned()),
        None => ("minecraft".to_owned(), lower),
    };
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./".contains(c))
    };
    (ok(&ns) && ok(&path)).then(|| format!("{ns}:{path}"))
}

/// `minecraft:stone` is `stone`; other namespaces are kept.
pub fn short_id(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_singular_and_plural() {
        assert_eq!(
            resolve("number"),
            Some(TypeName {
                class: "number",
                plural: false
            })
        );
        assert_eq!(
            resolve("numbers"),
            Some(TypeName {
                class: "number",
                plural: true
            })
        );
        assert_eq!(
            resolve("item types"),
            Some(TypeName {
                class: "item type",
                plural: true
            })
        );
        assert_eq!(
            resolve("objects"),
            Some(TypeName {
                class: "object",
                plural: true
            })
        );
        assert_eq!(resolve("nope"), None);
    }

    #[test]
    fn numbers_and_timespans() {
        assert_eq!(parse_number("5"), Some(Value::Int(5)));
        assert_eq!(parse_number("-2.5"), Some(Value::Num(-2.5)));
        assert_eq!(parse_number("nan"), None);
        assert_eq!(parse_number("1e3"), None);
        assert_eq!(parse_timespan("1 second"), Some(1000));
        assert_eq!(parse_timespan("1 minute, 30 seconds"), Some(90_000));
        assert_eq!(parse_timespan("2 ticks"), Some(100));
        assert_eq!(parse_timespan("5"), None);
        assert_eq!(format_timespan(90_000), "1 minute, 30 seconds");
        assert_eq!(format_timespan(0), "0 seconds");
    }

    #[test]
    fn dates_round_trip() {
        let ms = parse_date("2026-11-09 19:05:07").unwrap();
        assert_eq!(format_date(ms), "2026-11-09 19:05:07");
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(format_date(-1000), "1969-12-31 23:59:59");
        assert_eq!(parse_date("2026-13-01"), None);
    }

    #[test]
    fn item_and_entity_ids() {
        assert_eq!(
            normalize_id("Diamond Sword").as_deref(),
            Some("minecraft:diamond_sword")
        );
        assert_eq!(normalize_id("mod:thing").as_deref(), Some("mod:thing"));
        assert_eq!(normalize_id("bad id!"), None);
        assert_eq!(split_amount("3 stone"), (3, "stone"));
        assert_eq!(split_amount("stone"), (1, "stone"));
    }

    #[test]
    fn every_class_parses_and_prints() {
        use crate::host::Recorder;
        let host = Recorder::with_players(&[(7, "Alice")]);
        let parse = |class: &str, text: &str| parse_value(class, text, &host);
        assert_eq!(parse("integer", "42"), Some(Value::Int(42)));
        assert_eq!(parse("integer", "4.2"), None);
        assert_eq!(parse("boolean", "TRUE"), Some(Value::Bool(true)));
        assert_eq!(
            parse("player", "alice"),
            Some(Value::Player {
                uuid: 7,
                name: "Alice".into()
            })
        );
        assert_eq!(parse("player", "Nobody"), None);
        assert_eq!(
            parse("world", "world_nether"),
            Some(Value::World("world_nether".into()))
        );
        assert_eq!(parse("world", "two words"), None);
        let loc = parse("location", "world 1 64 -3.5").unwrap();
        assert_eq!(loc.to_text(), "world at 1, 64, -3.5");
        let item = parse("item type", "3 Diamond Sword").unwrap();
        assert_eq!(
            item,
            Value::ItemType {
                id: "minecraft:diamond_sword".into(),
                amount: 3
            }
        );
        assert_eq!(item.to_text(), "3 diamond sword");
        assert_eq!(parse("item type", "stone").unwrap().to_text(), "stone");
        let entity = parse("entity type", "Zombie").unwrap();
        assert_eq!(entity, Value::EntityType("minecraft:zombie".into()));
        assert_eq!(entity.to_text(), "zombie");
        let date = parse("date", "2026-11-09 12:00").unwrap();
        assert_eq!(date.to_text(), "2026-11-09 12:00:00");
        assert_eq!(Value::Num(2.5).to_text(), "2.5");
        assert_eq!(Value::Num(5.0).to_text(), "5");
    }

    #[test]
    fn convert_follows_the_slot_class() {
        use crate::host::Recorder;
        let host = Recorder::default();
        assert_eq!(
            convert(Value::Int(3), "text", &host),
            Some(Value::Text("3".into()))
        );
        assert_eq!(
            convert(Value::Text("2".into()), "number", &host),
            Some(Value::Int(2))
        );
        assert_eq!(convert(Value::Num(2.5), "integer", &host), None);
        assert_eq!(
            convert(Value::Text("false".into()), "boolean", &host),
            Some(Value::Bool(false))
        );
        assert_eq!(convert(Value::Bool(true), "timespan", &host), None);
    }

    #[test]
    fn compatibility() {
        assert!(accepts("number", "integer"));
        assert!(!accepts("integer", "number"));
        assert!(accepts("text", "number"));
        assert!(accepts("object", "player"));
        assert!(!accepts("player", "text"));
    }
}
