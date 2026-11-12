//! Helpers the syntax groups share: reading slots, lists, numbers, players and UUIDs, and the
//! comparison rules `is`, `>` and `contains` use.

use crate::error::RunError;
use crate::host::{EntityInfo, PlayerState};
use crate::registry::Matched;
use crate::run::Ctx;
use crate::value::{Location, Value};
use std::cmp::Ordering;

/// The value of a slot, `Null` when the slot is empty (`%-type%`).
pub fn slot(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Value, RunError> {
    match m.expr(n) {
        Some(e) => e.eval(cx),
        None => Ok(Value::Null),
    }
}

/// The items of a value: a list gives its items, nothing gives none, anything else is one item.
pub fn items(v: Value) -> Vec<Value> {
    match v {
        Value::List(items) => items,
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

/// A number value: whole numbers stay integers, others are decimals.
pub fn num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1.0e15 {
        Value::Int(f as i64)
    } else {
        Value::Num(f)
    }
}

/// The number a value stands for, or an error that names its class.
pub fn number_of(v: &Value) -> Result<f64, RunError> {
    v.as_f64()
        .ok_or_else(|| RunError::new(format!("expected a number, got {}", v.class())))
}

/// The text a value stands for.
pub fn text_of(v: &Value) -> String {
    v.to_text()
}

/// The UUID and the name of a player value.
pub fn player_of(v: &Value) -> Result<(u128, String), RunError> {
    match v {
        Value::Player { uuid, name } => Ok((*uuid, name.clone())),
        other => Err(RunError::new(format!(
            "expected a player, got {}",
            other.class()
        ))),
    }
}

/// The state of a player value. An online player has the engine's player state; an NPC or another
/// entity the host knows gives its entity state. Anything else has none.
pub fn state_of(cx: &Ctx<'_>, v: &Value) -> Result<(u128, PlayerState), RunError> {
    let (uuid, name) = player_of(v)?;
    cx.host
        .player_state(uuid)
        .or_else(|| cx.host.entity_state(uuid))
        .map(|s| (uuid, s))
        .ok_or_else(|| RunError::new(format!("{name} is not online")))
}

/// The value a script gets for an entity of a world: a player-type entity (NPCs included) is a
/// player value, any other entity is its entity type.
pub fn entity_value(e: &EntityInfo) -> Value {
    if e.kind == "minecraft:player" {
        Value::Player {
            uuid: e.uuid,
            name: e.name.clone(),
        }
    } else {
        Value::EntityType(e.kind.clone())
    }
}

/// Whether two locations are in the same world and within `radius` blocks of each other.
pub fn within(center: &Location, at: &Location, radius: f64) -> bool {
    center.world.eq_ignore_ascii_case(&at.world)
        && ((center.x - at.x).powi(2) + (center.y - at.y).powi(2) + (center.z - at.z).powi(2))
            <= radius * radius
}

/// A colour value, as `rgb(r, g, b)` makes it: a list of three whole numbers, each 0 to 255.
/// There is no colour class yet (GAPS.md), so a colour is this list.
pub fn colour_value(r: f64, g: f64, b: f64) -> Value {
    let ch = |v: f64| Value::Int(v.trunc().clamp(0.0, 255.0) as i64);
    Value::List(vec![ch(r), ch(g), ch(b)])
}

/// The channels of a colour value (see [`colour_value`]), `None` when the value is not one.
pub fn colour_of(v: &Value) -> Option<[u8; 3]> {
    match v {
        Value::List(items) if items.len() == 3 => {
            let mut out = [0u8; 3];
            for (i, item) in items.iter().enumerate() {
                let n = item.as_i64()?;
                out[i] = u8::try_from(n).ok()?;
            }
            Some(out)
        }
        _ => None,
    }
}

/// The state of the player in slot `n`.
pub fn state_at(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<(u128, PlayerState), RunError> {
    let v = slot(cx, m, n)?;
    state_of(cx, &v)
}

/// The location of a location value, or of a player's position.
pub fn location_of(cx: &Ctx<'_>, v: &Value) -> Result<Location, RunError> {
    match v {
        Value::Location(l) => Ok(l.clone()),
        other => Ok(state_of(cx, other)?.1.location),
    }
}

/// The location in slot `n`, from a location or a player.
pub fn location_at(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Location, RunError> {
    let v = slot(cx, m, n)?;
    location_of(cx, &v)
}

/// The world a player or a location in slot `n` is in.
pub fn world_at(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<String, RunError> {
    let v = slot(cx, m, n)?;
    world_of(cx, &v)
}

/// The world a player or a location is in.
pub fn world_of(cx: &Ctx<'_>, v: &Value) -> Result<String, RunError> {
    match v {
        Value::Location(Location { world, .. }) | Value::World(world) => Ok(world.clone()),
        Value::Player { .. } => Ok(state_of(cx, v)?.1.location.world),
        other => Err(RunError::new(format!(
            "expected a location or a player, got {}",
            other.class()
        ))),
    }
}

/// `1234abcd-...` form of a UUID, lower case, with dashes.
pub fn format_uuid(u: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (u >> 96) as u32,
        (u >> 80) & 0xffff,
        (u >> 64) & 0xffff,
        (u >> 48) & 0xffff,
        u & 0xffff_ffff_ffff
    )
}

/// Whether a negated condition (`is not`, `isn't`, `doesn't contain`) was matched: mark 1.
pub fn negated(m: &Matched) -> bool {
    m.has_mark(1)
}

/// Equality of two values. Numbers compare by value across integer and decimal; text, names and
/// worlds ignore case (Skript's default); values of different classes are not equal.
pub fn equal(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Null, Null) => true,
        (Int(_) | Num(_), Int(_) | Num(_)) => a.as_f64() == b.as_f64(),
        (Text(x), Text(y)) => x.eq_ignore_ascii_case(y),
        (Bool(x), Bool(y)) => x == y,
        (Timespan(x), Timespan(y)) | (Date(x), Date(y)) => x == y,
        (Player { uuid: x, .. }, Player { uuid: y, .. }) => x == y,
        (Player { name, .. }, Text(t)) | (Text(t), Player { name, .. }) => {
            name.eq_ignore_ascii_case(t)
        }
        (World(x), World(y)) => x.eq_ignore_ascii_case(y),
        (World(x), Text(t)) | (Text(t), World(x)) => x.eq_ignore_ascii_case(t),
        (Location(x), Location(y)) => {
            x.world.eq_ignore_ascii_case(&y.world) && x.x == y.x && x.y == y.y && x.z == y.z
        }
        // A named item matches only an item of that name; a plain item type matches any name
        // (Skript's ItemType.isSimilar: the right side's meta must be present on the left).
        (
            ItemType {
                id: x, name: xn, ..
            },
            ItemType {
                id: y, name: yn, ..
            },
        ) => x == y && (yn.is_none() || xn == yn),
        (EntityType(x), EntityType(y)) => x == y,
        _ => false,
    }
}

/// The order of two values, for `>` and `<`: numbers, timespans and dates. Other pairs cannot be
/// ordered and give an error.
pub fn order(a: &Value, b: &Value) -> Result<Ordering, RunError> {
    use Value::*;
    let r = match (a, b) {
        (Int(_) | Num(_), Int(_) | Num(_)) => a
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&b.as_f64().unwrap_or(0.0)),
        (Timespan(x), Timespan(y)) | (Date(x), Date(y)) => Some(x.cmp(y)),
        _ => None,
    };
    r.ok_or_else(|| RunError::new(format!("can't compare {} with {}", a.class(), b.class())))
}

/// The number of an item in an inventory: the sum of the stacks with its id.
pub fn count_in(state: &PlayerState, id: &str) -> i64 {
    state
        .inventory
        .iter()
        .filter(|s| s.id == id)
        .map(|s| s.amount)
        .sum()
}

/// Test helpers shared by the syntax groups: a script runs on a [`Recorder`] host and the
/// effects it asked for are read back from the host.
#[cfg(test)]
pub(crate) mod testing {
    use crate::host::Recorder;
    use crate::manager::Manager;
    use crate::registry::Registry;
    use crate::value::Value;

    /// Loads `body` (script lines, one tab in) as an `on load` trigger on `host`, and returns the
    /// run errors the log holds. Panics on parse errors.
    pub fn run(host: &Recorder, body: &str) -> Vec<String> {
        let src = format!("on load:\n{body}");
        let mut m = Manager::new(
            Registry::new(),
            std::env::temp_dir().join("horizon-skript-test"),
        );
        if let Err(errs) = m.load_source(host, "test.sk", &src, true) {
            panic!("the script does not parse: {errs:?}");
        }
        host.logs.borrow().clone()
    }

    /// The lines the effects asked the host to do.
    pub fn acted(host: &Recorder) -> Vec<String> {
        host.acted.borrow().clone()
    }

    /// The value of an expression, as its text (the script stores it in `{r}` and broadcasts it).
    pub fn expr(host: &Recorder, e: &str) -> String {
        let logs = run(
            host,
            &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r}}%\"\n"),
        );
        assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
        let out = host.broadcasts.borrow().clone();
        assert_eq!(out.len(), 1, "no broadcast for `{e}`");
        out[0].clone()
    }

    /// The value of an expression, as a value (the list form of `{r::*}` is not used here).
    pub fn value_text(v: &Value) -> String {
        v.to_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_print_in_the_usual_form() {
        assert_eq!(
            format_uuid(0x1234_5678_9abc_def0_1122_3344_5566_7788),
            "12345678-9abc-def0-1122-334455667788"
        );
        assert_eq!(format_uuid(2), "00000000-0000-0000-0000-000000000002");
    }

    #[test]
    fn numbers_come_back_as_integers_when_whole() {
        assert_eq!(num(18.0), Value::Int(18));
        assert_eq!(num(2.5), Value::Num(2.5));
    }

    #[test]
    fn equal_and_order_follow_the_class_rules() {
        assert!(equal(&Value::Text("A".into()), &Value::Text("a".into())));
        assert!(equal(&Value::Num(2.0), &Value::Int(2)));
        assert!(!equal(&Value::Text("2".into()), &Value::Int(2)));
        assert!(equal(
            &Value::ItemType {
                id: "minecraft:stone".into(),
                amount: 1,
                name: None
            },
            &Value::ItemType {
                id: "minecraft:stone".into(),
                amount: 9,
                name: None
            },
        ));
        assert_eq!(
            order(&Value::Int(1), &Value::Num(1.5)).unwrap(),
            Ordering::Less
        );
        assert!(order(&Value::Bool(true), &Value::Bool(false)).is_err());
    }

    #[test]
    fn count_sums_the_stacks_of_one_id() {
        let state = PlayerState {
            name: String::new(),
            display_name: String::new(),
            location: crate::value::Location {
                world: "w".into(),
                x: 0.0,
                y: 0.0,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            health: 20.0,
            max_health: 20.0,
            food: 20,
            level: 0,
            gamemode: "survival".into(),
            flying: false,
            can_fly: false,
            sneaking: false,
            sprinting: false,
            swimming: false,
            op: false,
            held: "minecraft:air".into(),
            inventory: vec![
                crate::host::ItemStack {
                    id: "minecraft:flint".into(),
                    amount: 2,
                    name: None,
                },
                crate::host::ItemStack {
                    id: "minecraft:flint".into(),
                    amount: 5,
                    name: None,
                },
            ],
        };
        assert_eq!(count_in(&state, "minecraft:flint"), 7);
        assert_eq!(count_in(&state, "minecraft:stone"), 0);
    }
}
