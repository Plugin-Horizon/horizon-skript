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
    // Before the player form: `at %location%` is a sound at a place, not at each player.
    reg.add_effect(
        "play sound at",
        &["play sound[s] %text%[ (with|and) volume %-number%][ (with|and) pitch %-number%] at %location%[ (to|for) %-players%]"],
        -1,
        play_sound_at,
    )?;
    reg.add_effect("spawn", &["spawn %entity types% at %location%"], 0, spawn)?;
    Ok(())
}

/// `play sound S [with volume V] [with pitch P] at LOCATION [for PLAYERS]`: one sound at a place,
/// heard by the named players or by everyone near it. Slots: 0 sound, 1 volume, 2 pitch,
/// 3 location, 4 players.
fn play_sound_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let sound = sound_id(&slot(cx, m, 0)?.to_text())?;
    let volume = number_in(cx, m, 1)?.unwrap_or(1.0) as f32;
    let pitch = number_in(cx, m, 2)?.unwrap_or(1.0) as f32;
    let at = location(cx, m, 3)?;
    let to: Vec<Option<u128>> = match slot(cx, m, 4)? {
        Value::Null => vec![None],
        v => super::items_of(v)
            .iter()
            .map(|p| super::player_of(p).map(|(uuid, _)| Some(uuid)))
            .collect::<Result<_, _>>()?,
    };
    for target in to {
        cx.host
            .actions()
            .play_sound(target, &at, &sound, volume, pitch);
    }
    next()
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
    fn sound_at_a_location_for_named_players() {
        use crate::host::Recorder;
        use crate::syntax::support::testing::{acted, run};
        let host = Recorder::with_players(&[(1, "Alice")]);
        let logs = run(
            &host,
            "\tplay sound \"entity.generic.eat\" with volume 0.7 with pitch 1.5 at location(2, 3, 4, world \"stage\") for Alice\n\tplay sound \"block.amethyst_block.chime\" at location(0, 0, 0, world \"stage\")\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "sound 1 minecraft:entity.generic.eat 0.7/1.5 at stage at 2, 3, 4".to_owned(),
                "sound all minecraft:block.amethyst_block.chime 1/1 at stage at 0, 0, 0".to_owned(),
            ]
        );
    }

    #[test]
    fn sound_ids_are_namespaced_and_checked() {
        assert_eq!(
            sound_id("entity.cow.ambient").unwrap(),
            "minecraft:entity.cow.ambient"
        );
        assert!(sound_id("bad; say hi").is_err());
    }
}
