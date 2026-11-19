//! Values of the event a trigger runs for: the attacker, the victim and `event-*`. The glue fills
//! them when it fires an event (see `syntax/events/mod.rs` for the table of names). Ported from
//! Skript's `ExprAttacker`, `ExprVictim` and the `EventValueExpression` family.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::state_of;
use crate::horizon_skript_core::value::Value;

/// The event value a name stands for; `player` is the event's player.
fn event_value(cx: &mut Ctx<'_>, name: &str) -> Result<Value, RunError> {
    let v = if name == "player" {
        cx.event.player.clone()
    } else if name == "message" {
        cx.event.message_now().map(Value::Text)
    } else {
        cx.event.value(name).cloned()
    };
    v.ok_or_else(|| RunError::new(format!("there is no {name} in this event")))
}

fn attacker(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    event_value(cx, "attacker")
}

fn victim(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    event_value(cx, "victim")
}

/// `event-location`, `event-block`, ...: the name is the whole match of the regex.
fn event_named(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let name = m.regex(0).unwrap_or_default().to_owned();
    event_value(cx, &name)
}

/// What scripts call an event value, and the name the glue sets it under.
const ALIASES: &[(&str, &str)] = &[
    ("clicked block", "block"),
    ("damage cause", "cause"),
    ("final damage", "damage"),
    ("damage", "damage"),
    ("weather", "weather"),
    ("block", "block"),
];

/// `the clicked block`, `the damage`, `the damage cause`, `the weather`, `the block`.
fn event_alias(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let said = m.regex(0).unwrap_or_default().to_ascii_lowercase();
    let name = ALIASES
        .iter()
        .find(|(alias, _)| *alias == said)
        .map_or("", |(_, n)| *n);
    event_value(cx, name)
}

/// `event-world`, `event-entity`, `event-number`, `event-item type`, ...: the name scripts write
/// after `event-`, and the glue's name for the value. `event-number` is also the new value of an
/// event that changes one (`hunger change`, `level change`).
fn event_kind(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let said = m.regex(0).unwrap_or_default().to_ascii_lowercase();
    let name = match said.as_str() {
        "world" => "world",
        "entity" | "living entity" | "entity type" => "entity",
        "vehicle" => "vehicle",
        "weather" | "weather type" => "weather",
        "game mode" | "gamemode" => "gamemode",
        "item type" | "itemtype" | "itemstack" | "item stack" => "item",
        "number" | "integer" => {
            if cx.event.value("number").is_none() {
                "to"
            } else {
                "number"
            }
        }
        _ => "",
    };
    event_value(cx, name)
}

/// `the command` (its label) and `the full command` (with its arguments) in a command event.
fn command_text(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let full = cx
        .event
        .message_now()
        .ok_or_else(|| RunError::new("there is no command in this event"))?;
    if m.has_mark(1) {
        Ok(Value::Text(full))
    } else {
        Ok(Value::Text(
            full.split_whitespace().next().unwrap_or_default().to_owned(),
        ))
    }
}

/// `the command sender`: the player, or `console` when the event has no player.
fn command_sender(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx
        .event
        .player
        .clone()
        .unwrap_or_else(|| Value::Text("console".to_owned())))
}

/// `the location` and `the position` (a bare `location` stays free for slots): the event's location, or the player's when it has none.
fn event_location(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    if let Some(v) = cx.event.value("location") {
        return Ok(v.clone());
    }
    match cx.event.player.clone() {
        Some(p) => Ok(Value::Location(state_of(cx, &p)?.1.location)),
        None => Err(RunError::new("there is no location in this event")),
    }
}

/// `the world` (a bare `world` is a world name in slots such as `is in world world`): the event's world, or the world of its location or player.
fn event_world(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    if let Some(v) = cx.event.value("world") {
        return Ok(v.clone());
    }
    if let Some(Value::Location(l)) = cx.event.value("location") {
        return Ok(Value::World(l.world.clone()));
    }
    match cx.event.player.clone() {
        Some(p) => Ok(Value::World(state_of(cx, &p)?.1.location.world)),
        None => Err(RunError::new("there is no world in this event")),
    }
}

/// `me` and `myself`: the player of the event.
fn myself(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    event_value(cx, "player")
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "attacker",
        &["[the] (attacker|damager)"],
        0,
        "object",
        attacker,
    )?;
    reg.add_expression("victim", &["[the] victim"], 0, "object", victim)?;
    reg.add_expression(
        "event value",
        &["[the] event-<player|location|block|item|from|to|message|damage|cause>"],
        0,
        "object",
        event_named,
    )?;
    reg.add_expression(
        "event alias",
        &["[the] <(?i)clicked block|damage cause|final damage|damage|weather|block>"],
        0,
        "object",
        event_alias,
    )?;
    reg.add_expression(
        "event kind",
        &["[the] event-<(?i)world|entity type|living entity|entity|vehicle|weather type|weather|game mode|gamemode|item type|itemtype|itemstack|item stack|number|integer>"],
        0,
        "object",
        event_kind,
    )?;
    reg.add_expression(
        "command",
        &["[the] (1¦full|1¦complete|1¦whole) command", "[the] command"],
        0,
        "text",
        command_text,
    )?;
    reg.add_expression(
        "command sender",
        &["[the] [(command|script command)] (sender|executor)"],
        0,
        "object",
        command_sender,
    )?;
    reg.add_expression(
        "event location",
        &["the (location|position)"],
        1,
        "location",
        event_location,
    )?;
    reg.add_expression("event world", &["the world"], 1, "world", event_world)?;
    reg.add_expression("myself", &["me", "my[self]"], 1, "player", myself)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::Manager;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::registry::Registry;
    use crate::horizon_skript_core::run::Event;
    use crate::horizon_skript_core::value::Value;
    use std::path::PathBuf;

    /// Loads `on <event>:` with `body`, fires it with `data` and returns what it broadcast.
    fn fired(name: &str, body: &str, data: Event) -> Vec<String> {
        let host = Recorder::default();
        let mut m = Manager::new(Registry::new(), PathBuf::new());
        let src = format!("on {name}:\n\t{body}\n");
        m.load_source(&host, "t.sk", &src, true).unwrap();
        m.fire(&host, name, data);
        let out = host.broadcasts.borrow().clone();
        assert!(host.logs.borrow().is_empty(), "{:?}", host.logs.borrow());
        out
    }

    fn alice() -> Value {
        Value::Player {
            uuid: 1,
            name: "Alice".into(),
        }
    }

    #[test]
    fn damage_values_have_their_scripts_names() {
        let data = Event::default()
            .with("damage", Value::Num(4.5))
            .with("cause", Value::Text("fall".into()))
            .with("world", Value::World("w".into()));
        assert_eq!(
            fired(
                "damage",
                "broadcast \"%the damage% %damage cause% %event-world% %final damage%\"",
                data
            ),
            vec!["4.5 fall w 4.5".to_owned()]
        );
    }

    #[test]
    fn the_command_is_its_label_or_the_whole_line() {
        let mut data = Event {
            player: Some(alice()),
            ..Event::default()
        };
        data.message = Some("home set base".into());
        assert_eq!(
            fired(
                "command",
                "broadcast \"%the command%|%the full command%|%the command sender%|%me%\"",
                data
            ),
            vec!["home|home set base|Alice|Alice".to_owned()]
        );
        let mut data = Event::default();
        data.message = Some("stop".into());
        assert_eq!(
            fired("command", "broadcast \"%the command sender%\"", data),
            vec!["console".to_owned()]
        );
    }

    #[test]
    fn event_number_falls_back_to_the_new_value() {
        let data = Event::default()
            .with("from", Value::Int(10))
            .with("to", Value::Int(7));
        assert_eq!(
            fired("hunger change", "broadcast \"%event-number%\"", data),
            vec!["7".to_owned()]
        );
        let data = Event::default().with("number", Value::Int(3));
        assert_eq!(
            fired("experience change", "broadcast \"%event-number%\"", data),
            vec!["3".to_owned()]
        );
    }

    #[test]
    fn the_location_and_the_world_come_from_the_event() {
        use crate::horizon_skript_core::value::Location;
        let loc = Location {
            world: "w".into(),
            x: 1.0,
            y: 2.0,
            z: 3.0,
            yaw: 0.0,
            pitch: 0.0,
        };
        let data = Event::default().with("location", Value::Location(loc));
        assert_eq!(
            fired(
                "bed enter",
                "broadcast \"%the location%|%the world%\"",
                data
            ),
            vec!["w at 1, 2, 3|w".to_owned()]
        );
    }
}
