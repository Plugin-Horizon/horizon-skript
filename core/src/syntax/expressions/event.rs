//! Values of the event a trigger runs for: the attacker, the victim and `event-*`. The glue fills
//! them when it fires an event (see `syntax/events/mod.rs` for the table of names). Ported from
//! Skript's `ExprAttacker`, `ExprVictim` and the `EventValueExpression` family.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;

/// The event value a name stands for; `player` is the event's player.
fn event_value(cx: &mut Ctx<'_>, name: &str) -> Result<Value, RunError> {
    let v = if name == "player" {
        cx.event.player.clone()
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
    Ok(())
}
