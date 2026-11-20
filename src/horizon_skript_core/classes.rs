//! The types Skript patterns name (`%number%`, `%players%`, ...): parsing from text, text form,
//! static compatibility between expression results and pattern slots, and run-time conversion.

use crate::horizon_skript_core::host::Host;
use crate::horizon_skript_core::value::{Location, Value};

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
    "vector",
    "slot",
    "potion effect type",
    "potion effect",
];

/// Classes beyond the core ones: entities, command senders, UUIDs, colours.
pub const EXTRA_CLASSES: &[&str] = &["entity", "commandsender", "uuid", "color"];

/// Skript types whose values are one word of a fixed set, kept as lower-case text (`survival`,
/// `entity attack`). The words are the allowed values; an empty list takes any word, because the
/// engine's own list is not known here.
pub const ENUM_CLASSES: &[(&str, &[&str])] = &[
    ("gamemode", &["survival", "creative", "adventure", "spectator"]),
    ("difficulty", &["peaceful", "easy", "normal", "hard"]),
    ("weathertype", &["clear", "rain", "thunder"]),
    (
        "soundcategory",
        &[
            "master", "music", "record", "weather", "block", "hostile", "neutral", "player",
            "ambient", "voice",
        ],
    ),
    ("itemrarity", &["common", "uncommon", "rare", "epic"]),
    ("environment", &["normal", "nether", "the end"]),
    ("damagecause", &[]),
    ("teleportcause", &[]),
    ("spawnreason", &[]),
    ("healreason", &[]),
    ("transformreason", &[]),
    ("quitreason", &[]),
    ("inventoryaction", &[]),
    ("inventoryclosereason", &[]),
    ("clicktype", &[]),
    ("equipmentslot", &[]),
    ("itemflag", &[]),
    ("villagerprofession", &[]),
    ("villagertype", &[]),
    ("biome", &[]),
    ("gamerule", &[]),
    ("enchantmenttype", &[]),
    ("potioneffecttype", &[]),
    ("particle", &[]),
    ("bossbarstyle", &[]),
    ("moonphase", &[]),
    ("treetype", &[]),
    ("resourcepackstate", &[]),
    ("billboard", &[]),
    ("textalignment", &[]),
    ("itemdisplaytransform", &[]),
];

/// Names that stand for another class: Skript's Java number types, `string`, the item and block
/// types (a block is read as its item type here), and the entity kinds that all take an entity.
/// An offline player is read as an online one: the host only knows players that are online.
const ALIASES: &[(&str, &str)] = &[
    ("string", "text"),
    ("byte", "integer"),
    ("short", "integer"),
    ("long", "integer"),
    ("float", "number"),
    ("double", "number"),
    ("itemstack", "item type"),
    ("material", "item type"),
    ("block", "item type"),
    ("blockdata", "item type"),
    ("offlineplayer", "player"),
    ("livingentity", "entity"),
    ("projectile", "entity"),
    ("vehicle", "entity"),
    ("inventoryholder", "entity"),
];

/// A type as a pattern names it: `number`, `numbers` (a list), `item types`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeName {
    pub class: &'static str,
    pub plural: bool,
}

/// The plural of a class name: `item type` gives `item types`, `entity` gives `entities`.
fn plural_of(class: &str) -> String {
    match class.strip_suffix('y') {
        Some(stem) if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) => format!("{stem}ies"),
        _ => format!("{class}s"),
    }
}

/// Every enum class: `ENUM_CLASSES` and the ones in `classes_more`.
fn enum_classes() -> impl Iterator<Item = &'static (&'static str, &'static [&'static str])> {
    ENUM_CLASSES.iter().chain(super::classes_more::MORE_ENUM_CLASSES.iter())
}

/// Whether `class` is one of the fixed-word enum classes.
pub fn is_enum(class: &str) -> bool {
    enum_classes().any(|(name, _)| *name == class)
}

/// Every class name the core knows: core, extra and enum classes (not the aliases).
pub fn all_class_names() -> Vec<&'static str> {
    CLASS_NAMES
        .iter()
        .chain(EXTRA_CLASSES)
        .copied()
        .chain(enum_classes().map(|(name, _)| *name))
        .collect()
}

/// Looks one spelling up among the class names and the aliases.
fn lookup(n: &str) -> Option<TypeName> {
    for class in all_class_names() {
        if n == class {
            return Some(TypeName {
                class,
                plural: false,
            });
        }
        // `numbers`, `item types`, `players`, `entities`.
        if class != "object" && (n == format!("{class}s") || n == plural_of(class)) {
            return Some(TypeName {
                class,
                plural: true,
            });
        }
    }
    for (alias, core) in ALIASES {
        let class = all_class_names().into_iter().find(|c| c == core)?;
        if n == *alias {
            return Some(TypeName {
                class,
                plural: false,
            });
        }
        if n == plural_of(alias) {
            return Some(TypeName {
                class,
                plural: true,
            });
        }
    }
    None
}

/// Resolves a pattern type name, singular or plural. `None` for unknown names. Names written with
/// spaces (`game mode`, `command sender`, `offline player`) are the same as without.
pub fn resolve(name: &str) -> Option<TypeName> {
    let n = name.trim().to_ascii_lowercase();
    if let Some(t) = lookup(&n) {
        return Some(t);
    }
    if n == "objects" {
        return Some(TypeName {
            class: "object",
            plural: true,
        });
    }
    if n.contains(' ') {
        return lookup(&n.replace(' ', ""));
    }
    None
}

/// Whether an expression of class `have` can fill a slot of class `want` (both singular).
/// Text takes any value (it is the text form), numbers take integers, objects take everything.
pub fn accepts(want: &str, have: &str) -> bool {
    want == "object"
        || want == have
        || want == "text"
        || (want == "number" && have == "integer")
        || match want {
            "entity" => matches!(have, "player" | "entity type"),
            "commandsender" => matches!(have, "player" | "text"),
            "uuid" => have == "text",
            // A text literal (`"2000-01-01"`, `"world"`) is read as a date or a world when it runs (`convert`).
            "date" | "world" => have == "text",
            w => is_enum(w) && have == "text",
        }
}

/// Whether `class` is an enum class with a fixed list of words, so a bare word in a script can be
/// told from other text.
pub fn is_strict_enum(class: &str) -> bool {
    enum_classes().any(|(name, words)| *name == class && !words.is_empty())
}

/// The words of an enum class as scripts write them: lower case, `_` and `-` read as spaces.
pub fn enum_word(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A colour as the list `rgb(r, g, b)` makes: the names Skript knows (dye colours) or a hex code
/// (`#rrggbb`, `rrggbb`).
fn parse_colour(text: &str) -> Option<Value> {
    let word = enum_word(text).replace("gray", "grey");
    let named: Option<[i64; 3]> = match word.as_str() {
        "black" => Some([29, 29, 33]),
        "dark grey" => Some([71, 79, 82]),
        "light grey" => Some([157, 157, 151]),
        "white" => Some([249, 255, 254]),
        "dark blue" => Some([60, 68, 170]),
        "brown" => Some([131, 84, 50]),
        "dark cyan" => Some([22, 156, 156]),
        "light cyan" => Some([58, 179, 218]),
        "dark green" => Some([94, 124, 22]),
        "light green" => Some([128, 199, 31]),
        "yellow" => Some([254, 216, 61]),
        "orange" => Some([249, 128, 29]),
        "dark red" => Some([176, 46, 38]),
        "light red" => Some([243, 139, 170]),
        "dark purple" => Some([137, 50, 184]),
        "light purple" => Some([199, 78, 189]),
        _ => None,
    };
    let rgb = match named {
        Some(c) => c,
        None => {
            let hex = text.trim().trim_start_matches('#');
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let n = i64::from_str_radix(hex, 16).ok()?;
            [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff]
        }
    };
    Some(Value::List(rgb.iter().map(|c| Value::Int(*c)).collect()))
}

/// `12345678-9abc-def0-1122-334455667788`, as text in lower case.
fn parse_uuid(text: &str) -> Option<Value> {
    let t = text.trim().to_ascii_lowercase();
    let groups: Vec<&str> = t.split('-').collect();
    let lengths = [8, 4, 4, 4, 12];
    let ok = groups.len() == 5
        && groups
            .iter()
            .zip(lengths)
            .all(|(g, len)| g.len() == len && g.chars().all(|c| c.is_ascii_hexdigit()));
    ok.then_some(Value::Text(t))
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
            host.is_item_type(&id).then(|| Value::ItemType {
                id,
                amount,
                name: None,
            })
        }
        "entity type" => {
            let id = normalize_id(t)?;
            host.is_entity_type(&id).then(|| Value::EntityType(id))
        }
        // A player by name first, then an entity type.
        "entity" => host
            .player_by_name(t)
            .map(|(uuid, name)| Value::Player { uuid, name })
            .or_else(|| parse_value("entity type", t, host)),
        "commandsender" => {
            if t.eq_ignore_ascii_case("console") {
                Some(Value::Text("console".to_owned()))
            } else {
                parse_value("player", t, host)
            }
        }
        "uuid" => parse_uuid(t),
        "color" => parse_colour(t),
        w => {
            let (_, words) = enum_classes().find(|(name, _)| *name == w)?;
            let word = enum_word(t);
            (!word.is_empty() && (words.is_empty() || words.contains(&word.as_str())))
                .then_some(Value::Text(word))
        }
    }
}

/// Converts a run-time value to a slot's class: text forms, numeric forms, and parsing of text.
pub fn convert(v: Value, want: &str, host: &dyn Host) -> Option<Value> {
    if want == "object" || v.class() == want {
        return Some(v);
    }
    // A value that already is one of the extra classes (a player is an entity and a command
    // sender); enum classes are read from text below.
    if EXTRA_CLASSES.contains(&want) && accepts(want, v.class()) && !matches!(v, Value::Text(_)) {
        return Some(v);
    }
    if want == "color" {
        if let Value::List(parts) = &v {
            let channels = parts.len() == 3
                && parts
                    .iter()
                    .all(|p| matches!(p, Value::Int(n) if (0..=255).contains(n)));
            return channels.then_some(v);
        }
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
        use crate::horizon_skript_core::host::Recorder;
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
                amount: 3,
                name: None
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
        use crate::horizon_skript_core::host::Recorder;
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
    fn more_names_resolve_to_classes() {
        let one = |name: &str| resolve(name).map(|t| (t.class, t.plural));
        assert_eq!(one("game mode"), Some(("gamemode", false)));
        assert_eq!(one("gamemodes"), Some(("gamemode", true)));
        assert_eq!(one("entities"), Some(("entity", true)));
        assert_eq!(one("living entity"), Some(("entity", false)));
        assert_eq!(one("livingentities"), Some(("entity", true)));
        assert_eq!(one("offline players"), Some(("player", true)));
        assert_eq!(one("doubles"), Some(("number", true)));
        assert_eq!(one("long"), Some(("integer", false)));
        assert_eq!(one("string"), Some(("text", false)));
        assert_eq!(one("itemstack"), Some(("item type", false)));
        assert_eq!(one("command sender"), Some(("commandsender", false)));
        assert_eq!(one("difficulties"), Some(("difficulty", true)));
        assert_eq!(one("colors"), Some(("color", true)));
        assert_eq!(one("uuid"), Some(("uuid", false)));
        assert_eq!(one("item types"), Some(("item type", true)));
        assert_eq!(one("nonsense"), None);
    }

    #[test]
    fn the_new_classes_parse_their_words() {
        use crate::horizon_skript_core::host::Recorder;
        let host = Recorder::with_players(&[(7, "Alice")]);
        let parse = |class: &str, text: &str| parse_value(class, text, &host);
        assert_eq!(
            parse("gamemode", "Creative"),
            Some(Value::Text("creative".into()))
        );
        assert_eq!(parse("gamemode", "banana"), None);
        assert_eq!(
            parse("environment", "the_end"),
            Some(Value::Text("the end".into()))
        );
        assert_eq!(
            parse("damagecause", "ENTITY_ATTACK"),
            Some(Value::Text("entity attack".into()))
        );
        assert_eq!(
            parse("color", "dark red"),
            Some(Value::List(vec![
                Value::Int(176),
                Value::Int(46),
                Value::Int(38)
            ]))
        );
        assert_eq!(
            parse("color", "#0a141e"),
            Some(Value::List(vec![
                Value::Int(10),
                Value::Int(20),
                Value::Int(30)
            ]))
        );
        assert_eq!(parse("color", "#12345"), None);
        assert_eq!(
            parse("uuid", "12345678-9ABC-def0-1122-334455667788"),
            Some(Value::Text("12345678-9abc-def0-1122-334455667788".into()))
        );
        assert_eq!(parse("uuid", "not-a-uuid"), None);
        assert_eq!(
            parse("entity", "alice"),
            Some(Value::Player {
                uuid: 7,
                name: "Alice".into()
            })
        );
        assert_eq!(
            parse("entity", "zombie"),
            Some(Value::EntityType("minecraft:zombie".into()))
        );
        assert_eq!(
            parse("commandsender", "Console"),
            Some(Value::Text("console".into()))
        );
    }

    #[test]
    fn the_new_classes_accept_and_convert() {
        use crate::horizon_skript_core::host::Recorder;
        let host = Recorder::default();
        assert!(accepts("entity", "player"));
        assert!(accepts("entity", "entity type"));
        assert!(!accepts("entity", "number"));
        assert!(accepts("commandsender", "player"));
        assert!(accepts("gamemode", "text"));
        assert!(!accepts("gamemode", "number"));
        assert!(accepts("uuid", "text"));
        let alice = Value::Player {
            uuid: 1,
            name: "Alice".into(),
        };
        assert_eq!(convert(alice.clone(), "entity", &host), Some(alice));
        let red = Value::List(vec![Value::Int(255), Value::Int(0), Value::Int(0)]);
        assert_eq!(convert(red.clone(), "color", &host), Some(red));
        let bad = Value::List(vec![Value::Int(256), Value::Int(0), Value::Int(0)]);
        assert_eq!(convert(bad, "color", &host), None);
        assert_eq!(
            convert(Value::Text("Hard".into()), "difficulty", &host),
            Some(Value::Text("hard".into()))
        );
        assert_eq!(convert(Value::Text("lunatic".into()), "difficulty", &host), None);
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
