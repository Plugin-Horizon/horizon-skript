//! Players: kick, ban, op, kill, damage, heal, health, food, game mode, teleport, push, potion
//! effects, `cancel event` and `execute command`. Patterns follow Skript's `EffKick`, `EffBan`,
//! `EffOp`, `EffKill`, `EffHealth` (damage and heal), `EffTeleport`, `EffPush`,
//! `EffApplyPotionEffect`, `EffCancelEvent` and `EffCommand`.
//!
//! Kick, ban, op, kill, damage, game mode and potion effects run the vanilla command as the
//! console. Health, food, heal, teleport and push go through [`crate::host::Actions`].

use super::{console, next, number_in, players_in, slot, texts_in};
use crate::classes;
use crate::error::RunError;
use crate::host::Actions;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::{Location, Value, format_number};

/// A potion effect given without a time lasts this many seconds: vanilla's largest accepted
/// duration (about eleven days), which is as good as forever for a server.
const FOREVER_SECONDS: i64 = 1_000_000;
/// The speed of a push when the script gives none, in blocks per tick.
const DEFAULT_SPEED: f64 = 1.0;
const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "spectator"];

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "kick",
        &["kick %players% [(by reason of|because [of]|on account of|due to) %-text%]"],
        0,
        kick,
    )?;
    reg.add_effect(
        "ban",
        &["ban %players% [(by reason of|because [of]|on account of|due to) %-text%]"],
        0,
        ban,
    )?;
    reg.add_effect("unban", &["unban %players%"], 0, unban)?;
    reg.add_effect("op", &["op %players%"], 0, op)?;
    reg.add_effect("deop", &["de[-]op %players%"], 0, deop)?;
    reg.add_effect("kill", &["kill %players%"], 0, kill)?;
    reg.add_effect(
        "damage",
        &["damage %players% by %number% [heart[s]]"],
        0,
        damage,
    )?;
    reg.add_effect(
        "heal",
        &["heal %players% [by %-number% [heart[s]]]"],
        0,
        heal,
    )?;
    reg.add_effect(
        "set health",
        &["set health of %players% to %number%"],
        -1,
        set_health,
    )?;
    reg.add_effect(
        "set food",
        &["set (food|hunger) of %players% to %number%"],
        -1,
        set_food,
    )?;
    reg.add_effect(
        "set game mode",
        &[
            "(set|change) [the] game[ ]mode of %players% to %text%",
            "set %players%'s game[ ]mode to %text%",
        ],
        -1,
        set_game_mode,
    )?;
    // Skript's gamemode type also takes the bare word: `set player's gamemode to adventure`.
    reg.add_effect(
        "set game mode (word)",
        &[
            "(set|change) [the] game[ ]mode of %players% to (survival:survival|creative:creative|adventure:adventure|spectator:spectator)",
            "set %players%'s game[ ]mode to (survival:survival|creative:creative|adventure:adventure|spectator:spectator)",
        ],
        -1,
        set_game_mode_word,
    )?;
    reg.add_effect(
        "teleport",
        &["teleport %players% to %location%"],
        0,
        teleport,
    )?;
    reg.add_effect(
        "push",
        &["push %players% <(up|down|north|south|east|west)> [(at|with) [a] (speed|velocity|force) [of] %-number%]"],
        0,
        push_direction,
    )?;
    reg.add_effect(
        "push towards",
        &["push %players% (towards|1¦away from) %location% [(at|with) [a] (speed|velocity|force) [of] %-number%]"],
        0,
        push_towards,
    )?;
    // Both patterns have the players in slot 0 and the time in slot 1.
    reg.add_effect(
        "apply potion effect",
        &[
            "(apply|grant) <[a-z_:]+> [<[0-9]+>] to %players% [for %-timespan%]",
            "(affect|afflict) %players% with <[a-z_:]+>[ <[0-9]+>][ for %-timespan%]",
        ],
        0,
        apply_potion,
    )?;
    reg.add_effect("cancel event", &["cancel [the] event"], 0, cancel_event)?;
    reg.add_effect(
        "uncancel event",
        &["uncancel [the] event"],
        0,
        uncancel_event,
    )?;
    reg.add_effect(
        "execute command",
        &["execute [the] command[s] %texts% [by %-player%]"],
        0,
        execute_as_optional_player,
    )?;
    reg.add_effect(
        "execute command as player",
        &[
            "execute [the] %player% command[s] %texts%",
            "(let|make) %player% execute[ the][ command[s]] %texts%",
        ],
        0,
        execute_as_player,
    )?;
    Ok(())
}

/// The reason slot as ` reason`, or nothing when it is empty.
fn reason_suffix(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<String, RunError> {
    Ok(match slot(cx, m, n)? {
        Value::Null => String::new(),
        v => format!(" {}", v.to_text()),
    })
}

/// `kick players [by reason of R]`.
fn kick(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let reason = reason_suffix(cx, m, 1)?;
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("kick {name}{reason}"));
    }
    next()
}

/// `ban players [by reason of R]`: a vanilla ban, which has no duration here.
fn ban(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let reason = reason_suffix(cx, m, 1)?;
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("ban {name}{reason}"));
    }
    next()
}

fn unban(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("pardon {name}"));
    }
    next()
}

fn op(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("op {name}"));
    }
    next()
}

fn deop(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("deop {name}"));
    }
    next()
}

fn kill(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("kill {name}"));
    }
    next()
}

/// `damage players by N hearts`: a heart is two health points, and `/damage` counts points.
fn damage(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let points = format_number(number_in(cx, m, 1)?.unwrap_or(0.0) * 2.0);
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("damage {name} {points}"));
    }
    next()
}

/// `heal players [by N hearts]`: full health when no amount is given.
fn heal(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let points = number_in(cx, m, 1)?.map(|hearts| hearts * 2.0);
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().heal(uuid, points);
    }
    next()
}

/// `set health of players to N`, in health points (20 is full).
fn set_health(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let health = number_in(cx, m, 1)?.unwrap_or(0.0);
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().set_health(uuid, health);
    }
    next()
}

/// `set food of players to N`.
fn set_food(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let food = number_in(cx, m, 1)?.unwrap_or(0.0).round() as i64;
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().set_food(uuid, food);
    }
    next()
}

/// `set game mode of players to survival|creative|adventure|spectator`.
fn set_game_mode(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let mode = slot(cx, m, 1)?.to_text().to_ascii_lowercase();
    if !GAME_MODES.contains(&mode.as_str()) {
        return Err(RunError::new(format!("there is no game mode named {mode}")));
    }
    for (uuid, name) in players_in(cx, m, 0)? {
        // Skript sets it through the API (no chat message); the command is only the fallback.
        let id = GAME_MODES.iter().position(|m| *m == mode).unwrap_or(0) as i32;
        if !cx.host.actions().set_game_mode(uuid, id) {
            console(cx, &format!("gamemode {mode} {name}"));
        }
    }
    next()
}

/// `set player's game mode to adventure`: the mode is the parse tag.
fn set_game_mode_word(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let Some(mode) = m.tags.iter().find(|t| GAME_MODES.contains(&t.as_str())).cloned() else {
        return Err(RunError::new("no game mode in the effect"));
    };
    for (uuid, name) in players_in(cx, m, 0)? {
        // Skript sets it through the API (no chat message); the command is only the fallback.
        let id = GAME_MODES.iter().position(|m| *m == mode).unwrap_or(0) as i32;
        if !cx.host.actions().set_game_mode(uuid, id) {
            console(cx, &format!("gamemode {mode} {name}"));
        }
    }
    next()
}

/// `teleport players to location`.
fn teleport(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let to = location_in(cx, m, 1, "teleport")?;
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().teleport(uuid, &to);
    }
    next()
}

/// The location in slot `n`, or an error that names the effect.
fn location_in(
    cx: &mut Ctx<'_>,
    m: &Matched,
    n: usize,
    effect: &str,
) -> Result<Location, RunError> {
    match slot(cx, m, n)? {
        Value::Location(loc) => Ok(loc),
        other => Err(RunError::new(format!(
            "{effect} needs a location, got {}",
            other.class()
        ))),
    }
}

/// The unit vector of a vertical or compass direction.
fn direction_vector(word: &str) -> Option<[f64; 3]> {
    Some(match word {
        "up" => [0.0, 1.0, 0.0],
        "down" => [0.0, -1.0, 0.0],
        "north" => [0.0, 0.0, -1.0],
        "south" => [0.0, 0.0, 1.0],
        "east" => [1.0, 0.0, 0.0],
        "west" => [-1.0, 0.0, 0.0],
        _ => return None,
    })
}

/// `push players up|down|north|south|east|west [at speed S]`. Slots: 0 players, 1 speed.
fn push_direction(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let word = m.regex(0).unwrap_or_default().to_owned();
    let unit = direction_vector(&word)
        .ok_or_else(|| RunError::new(format!("there is no direction named {word}")))?;
    let speed = number_in(cx, m, 1)?.unwrap_or(DEFAULT_SPEED);
    let velocity = unit.map(|c| c * speed);
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().push(uuid, velocity);
    }
    next()
}

/// `push players towards|away from location [at speed S]`: from each player towards the location
/// (away: the opposite), a velocity of length S. Slots: 0 players, 1 location, 2 speed.
fn push_towards(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let away = m.has_mark(1);
    let target = location_in(cx, m, 1, "push")?;
    let speed = number_in(cx, m, 2)?.unwrap_or(DEFAULT_SPEED);
    for (uuid, _) in players_in(cx, m, 0)? {
        let state = cx
            .host
            .player_state(uuid)
            .ok_or_else(|| RunError::new("a player to push is not online"))?;
        let velocity = towards(&state.location, &target, speed, away);
        cx.host.actions().push(uuid, velocity);
    }
    next()
}

/// The velocity of length `speed` from `from` towards `to` (or away from it); zero when they are
/// in the same place.
fn towards(from: &Location, to: &Location, speed: f64, away: bool) -> [f64; 3] {
    let d = [to.x - from.x, to.y - from.y, to.z - from.z];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len == 0.0 {
        return [0.0; 3];
    }
    let signed = if away { -speed } else { speed };
    d.map(|c| c * signed / len)
}

/// `apply speed 2 to players [for T]` and `affect players with speed 2 [for T]`. The regex
/// groups are the effect's id and its level (level 1 is the amplifier 0, as Skript reads it).
/// Slots: 0 players, 1 timespan.
fn apply_potion(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let id = potion_id(m.regex(0).unwrap_or_default())?;
    let amplifier = potion_amplifier(m.regex(1))?;
    let seconds = match timespan_in(cx, m, 1)? {
        Some(ms) => potion_seconds(ms),
        None => FOREVER_SECONDS,
    };
    for (_, name) in players_in(cx, m, 0)? {
        console(
            cx,
            &format!("effect give {name} {id} {seconds} {amplifier}"),
        );
    }
    next()
}

/// The timespan in slot `n` in milliseconds, `None` when the slot is empty.
fn timespan_in(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Option<i64>, RunError> {
    match slot(cx, m, n)? {
        Value::Null => Ok(None),
        Value::Timespan(ms) => Ok(Some(ms)),
        other => Err(RunError::new(format!(
            "expected a timespan, got {}",
            other.class()
        ))),
    }
}

/// A potion effect's id as a namespaced id: `speed` is `minecraft:speed`.
fn potion_id(word: &str) -> Result<String, RunError> {
    classes::normalize_id(word).ok_or_else(|| RunError::new(format!("bad potion effect {word}")))
}

/// The level in the text (`2`) as the vanilla amplifier (`1`); no level is the first one.
fn potion_amplifier(level: Option<&str>) -> Result<i64, RunError> {
    match level {
        None => Ok(0),
        Some(text) => {
            let n: i64 = text
                .parse()
                .map_err(|_| RunError::new(format!("bad potion level {text}")))?;
            Ok((n - 1).max(0))
        }
    }
}

/// Whole seconds for a duration: at least one, at most [`FOREVER_SECONDS`].
fn potion_seconds(ms: i64) -> i64 {
    ((ms + 999) / 1000).clamp(1, FOREVER_SECONDS)
}

/// `cancel event`: the glue reads the flag once the event has fired.
fn cancel_event(cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    cx.event.cancel();
    next()
}

fn uncancel_event(cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    cx.event.uncancel();
    next()
}

/// `execute command "say hi" [by player]`: as the player when one is given, else as the console.
/// Slots: 0 texts, 1 player.
fn execute_as_optional_player(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let texts = texts_in(cx, m, 0)?;
    match slot(cx, m, 1)? {
        Value::Null => {
            for text in texts {
                console(cx, &text);
            }
        }
        Value::Player { uuid, .. } => {
            for text in &texts {
                run_as(cx.host.actions(), uuid, text);
            }
        }
        other => {
            return Err(RunError::new(format!(
                "execute needs a player, got {}",
                other.class()
            )));
        }
    }
    next()
}

/// `execute player command "say hi"`: as each player of the slot. Slots: 0 players, 1 texts.
fn execute_as_player(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let players = players_in(cx, m, 0)?;
    let texts = texts_in(cx, m, 1)?;
    for (uuid, _) in players {
        for text in &texts {
            run_as(cx.host.actions(), uuid, text);
        }
    }
    next()
}

fn run_as(actions: &dyn Actions, uuid: u128, line: &str) {
    actions.run_as(uuid, line.trim_start_matches('/'));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potion_levels_become_amplifiers() {
        assert_eq!(potion_amplifier(None).unwrap(), 0);
        assert_eq!(potion_amplifier(Some("1")).unwrap(), 0);
        assert_eq!(potion_amplifier(Some("3")).unwrap(), 2);
    }

    #[test]
    fn potion_ids_and_durations() {
        assert_eq!(potion_id("speed").unwrap(), "minecraft:speed");
        assert_eq!(potion_seconds(1), 1);
        assert_eq!(potion_seconds(90_000), 90);
        assert_eq!(potion_seconds(i64::MAX / 4), FOREVER_SECONDS);
    }

    #[test]
    fn towards_points_at_the_location_or_away() {
        let from = Location {
            world: "w".into(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
        };
        let to = Location {
            x: 3.0,
            y: 4.0,
            ..from.clone()
        };
        let v = towards(&from, &to, 1.0, false);
        assert!((v[0] - 0.6).abs() < 1e-9 && (v[1] - 0.8).abs() < 1e-9);
        assert!((towards(&from, &to, 1.0, true)[0] + 0.6).abs() < 1e-9);
        assert_eq!(towards(&from, &from, 1.0, false), [0.0; 3]);
    }

    #[test]
    fn directions_are_unit_vectors() {
        assert_eq!(direction_vector("north"), Some([0.0, 0.0, -1.0]));
        assert_eq!(direction_vector("sideways"), None);
    }
}

#[cfg(test)]
mod game_mode_tests {
    use crate::host::Recorder;
    use crate::syntax::support::testing::{acted, run};

    #[test]
    fn a_bare_game_mode_word_runs_the_gamemode_command() {
        let host = Recorder::with_players(&[(1, "Alice")]);
        let logs = run(&host, "\tset Alice's gamemode to adventure\n\tset game mode of Alice to creative\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&host), vec!["console gamemode adventure Alice", "console gamemode creative Alice"]);
    }

    #[test]
    fn possessive_game_mode_runs_the_gamemode_command() {
        let host = Recorder::with_players(&[(1, "Alice")]);
        let logs = run(&host, "\tset Alice's game mode to \"creative\"\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&host), vec!["console gamemode creative Alice"]);
    }
}
