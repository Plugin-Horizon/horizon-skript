//! Effects: statements that do something. Messages and titles, players (kick, ban, op, kill,
//! damage, health, food, game mode, teleport, push, potion effects, `cancel event`, commands),
//! items (give, remove, clear) and the world (blocks, sounds, spawning). Patterns follow
//! Skript's registrations (`EffKick`, `EffPlaySound`, `EffSendTitle`, ...), named in each file.
//!
//! Some effects are registered before the builtins (priority -1 or -2) because their text is
//! more specific than a builtin's: `remove 3 stone from {p}` must not reach the builtin
//! `remove %object% from %object%`. Such an effect hands the text it does not own to the builtin.

mod console;
mod entities;
mod items;
mod messages;
mod particles;
mod players;
mod world;

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;

/// Registers every effect of this group.
pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    messages::register(reg)?;
    players::register(reg)?;
    items::register(reg)?;
    world::register(reg)?;
    particles::register(reg)?;
    entities::register(reg)?;
    console::register(reg)?;
    Ok(())
}

// -- helpers the effects share --------------------------------------------------------------

/// The value of a slot; `Null` when the slot is empty (`%-type%`).
fn slot(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Value, RunError> {
    match m.expr(n) {
        Some(e) => e.eval(cx),
        None => Ok(Value::Null),
    }
}

/// The items of a value: a list gives its items, nothing gives none, anything else is one.
fn items_of(v: Value) -> Vec<Value> {
    match v {
        Value::List(items) => items,
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

/// The (UUID, name) of every player in slot `n`. An empty slot is the event's player.
fn players_in(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Vec<(u128, String)>, RunError> {
    let v = slot(cx, m, n)?;
    if v.is_null() {
        return match cx.event.player.clone() {
            Some(p) => Ok(vec![player_of(&p)?]),
            None => Err(RunError::new("there is no player in this event")),
        };
    }
    items_of(v).iter().map(player_of).collect()
}

/// The UUID and name of a player value.
fn player_of(v: &Value) -> Result<(u128, String), RunError> {
    match v {
        Value::Player { uuid, name } => Ok((*uuid, name.clone())),
        other => Err(RunError::new(format!(
            "expected a player, got {}",
            other.class()
        ))),
    }
}

/// The texts of slot `n` (a single text or a list of them), as strings.
fn texts_in(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Vec<String>, RunError> {
    Ok(items_of(slot(cx, m, n)?)
        .iter()
        .map(Value::to_text)
        .collect())
}

/// The number of slot `n`, `None` when the slot is empty.
fn number_in(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Option<f64>, RunError> {
    match slot(cx, m, n)? {
        Value::Null => Ok(None),
        v => v
            .as_f64()
            .map(Some)
            .ok_or_else(|| RunError::new(format!("expected a number, got {}", v.class()))),
    }
}

/// The ticks of a timespan in slot `n`, or `default` when the slot is empty.
fn ticks_in(cx: &mut Ctx<'_>, m: &Matched, n: usize, default: u64) -> Result<u64, RunError> {
    match slot(cx, m, n)? {
        Value::Null => Ok(default),
        Value::Timespan(ms) => Ok(crate::builtins::ticks_of(ms)),
        other => Err(RunError::new(format!(
            "expected a timespan, got {}",
            other.class()
        ))),
    }
}

/// Runs a vanilla command as the console, without a leading slash.
fn console(cx: &mut Ctx<'_>, line: &str) {
    cx.host.actions().run_console(line.trim_start_matches('/'));
}

/// A `Flow::Next`, the result of every effect here.
fn next() -> Result<Flow, RunError> {
    Ok(Flow::Next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_of_gives_one_item_or_the_list() {
        assert!(items_of(Value::Null).is_empty());
        assert_eq!(items_of(Value::Int(3)), vec![Value::Int(3)]);
        assert_eq!(
            items_of(Value::List(vec![Value::Int(1), Value::Int(2)])).len(),
            2
        );
    }

    #[test]
    fn player_of_needs_a_player() {
        let p = Value::Player {
            uuid: 9,
            name: "Alice".into(),
        };
        assert_eq!(player_of(&p).unwrap(), (9, "Alice".to_owned()));
        assert!(player_of(&Value::Text("Alice".into())).is_err());
    }
}
