//! The world: `set block`, `play sound` and `spawn`. Patterns follow Skript's `EffBlockUpdate`
//! family (`set block` as Skript's `set %block% to`), `EffPlaySound` and `EffSecSpawn`.
//!
//! A sound plays to each target player alone, from the player's own position: the vanilla
//! `playsound` runs `execute at <player>` so the sound is heard at full volume there. Sound ids
//! are checked like item ids; a sound that does not exist is the client's to ignore.

use super::{console, next, number_in, players_in, slot};
use crate::classes;
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::{Location, Value};

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "set block",
        &["set block (at|of) %location% to %item type%"],
        -1,
        set_block,
    )?;
    reg.add_effect(
        "play sound",
        &["play sound[s] %text%[ (at|with) volume %-number%][ (and|at|with) pitch %-number%][ (to|for) %-players%]"],
        0,
        play_sound,
    )?;
    reg.add_effect("spawn", &["spawn %entity types% at %location%"], 0, spawn)?;
    Ok(())
}

/// `set block at location to stone`: the block id of the item type.
fn set_block(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let at = location(cx, m, 0)?;
    let block = match slot(cx, m, 1)? {
        Value::ItemType { id, .. } => id,
        other => {
            return Err(RunError::new(format!(
                "set block needs an item type, got {}",
                other.class()
            )));
        }
    };
    cx.host.actions().set_block(&at, &block);
    next()
}

/// `spawn zombie at location`, for each entity type in the list.
fn spawn(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let at = location(cx, m, 1)?;
    for kind in super::items_of(slot(cx, m, 0)?) {
        match kind {
            Value::EntityType(id) => {
                cx.host.actions().spawn(&at, &id);
            }
            other => {
                return Err(RunError::new(format!(
                    "spawn needs an entity type, got {}",
                    other.class()
                )));
            }
        }
    }
    next()
}

/// `play sound "entity.experience_orb.pickup" [with volume V] [and pitch P] [to players]`.
/// Slots: 0 sound, 1 volume, 2 pitch, 3 players (the event's player when empty).
fn play_sound(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let raw = slot(cx, m, 0)?.to_text();
    let sound = sound_id(&raw)?;
    let volume = number_in(cx, m, 1)?.unwrap_or(1.0);
    let pitch = number_in(cx, m, 2)?.unwrap_or(1.0);
    for (_, name) in players_in(cx, m, 3)? {
        console(
            cx,
            &format!(
                "execute at {name} run playsound {sound} master {name} ~ ~ ~ {volume} {pitch}"
            ),
        );
    }
    next()
}

/// A sound id as the vanilla command takes it: `entity.cow.ambient` is `minecraft:entity.cow.ambient`.
fn sound_id(raw: &str) -> Result<String, RunError> {
    classes::normalize_id(raw).ok_or_else(|| RunError::new(format!("bad sound name {raw}")))
}

/// The location in slot `n`.
fn location(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Location, RunError> {
    match slot(cx, m, n)? {
        Value::Location(loc) => Ok(loc),
        other => Err(RunError::new(format!(
            "expected a location, got {}",
            other.class()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_ids_are_namespaced_and_checked() {
        assert_eq!(
            sound_id("entity.cow.ambient").unwrap(),
            "minecraft:entity.cow.ambient"
        );
        assert!(sound_id("bad; say hi").is_err());
    }
}
