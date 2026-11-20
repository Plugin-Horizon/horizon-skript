//! Effects on what the engine lets a script set on a player or an entity: fire, flight,
//! invulnerability, silence and a forced respawn. Patterns follow Skript's `EffIgnite`,
//! `EffToggleFlight`, `EffMakeFly`, `EffInvulnerability`, `EffSilence` and `EffRespawn`.
//!
//! Scripts see players (and NPC player-entities), so the entity forms of these effects take them.

use super::{next, players_in, slot};
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::Value;

/// How long `ignite` burns without a timespan: 8 seconds, as lava does.
const DEFAULT_FIRE_TICKS: i64 = 8 * 20;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "ignite",
        &[
            "(ignite|set fire to) %players% [for %-timespan%]",
            "(set|light) %players% on fire [for %-timespan%]",
        ],
        -1,
        ignite,
    )?;
    reg.add_effect("extinguish", &["extinguish %players%"], -1, extinguish)?;
    reg.add_effect(
        "allow flight",
        &["(allow|enable) (fly|flight) (for|to) %players%"],
        -1,
        allow_flight,
    )?;
    reg.add_effect(
        "disallow flight",
        &["(disallow|disable) (fly|flight) (for|to) %players%"],
        -1,
        disallow_flight,
    )?;
    reg.add_effect(
        "make fly",
        &[
            "force %players% to [(start|1¦stop)] fly[ing]",
            "make %players% (start|1¦stop) flying",
            "make %players% fly",
        ],
        -1,
        make_fly,
    )?;
    reg.add_effect(
        "make invulnerable",
        &["make %players% (invulnerable|invincible)"],
        -1,
        |cx: &mut Ctx<'_>, m: &Matched| flag(cx, m, "invulnerable", true),
    )?;
    reg.add_effect(
        "make vulnerable",
        &["make %players% (not (invulnerable|invincible)|vulnerable|vincible)"],
        -1,
        |cx: &mut Ctx<'_>, m: &Matched| flag(cx, m, "invulnerable", false),
    )?;
    reg.add_effect(
        "silence",
        &["silence %players%", "make %players% silent"],
        -1,
        |cx: &mut Ctx<'_>, m: &Matched| flag(cx, m, "silent", true),
    )?;
    reg.add_effect(
        "unsilence",
        &["unsilence %players%", "make %players% not silent"],
        -1,
        |cx: &mut Ctx<'_>, m: &Matched| flag(cx, m, "silent", false),
    )?;
    reg.add_effect(
        "force respawn",
        &["force %players% to respawn"],
        -1,
        force_respawn,
    )?;
    Ok(())
}

fn refused(what: &str) -> RunError {
    RunError::new(format!("the server refused to {what}"))
}

fn ignite(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let ticks = match slot(cx, m, 1)? {
        Value::Timespan(ms) => ms / 50,
        _ => DEFAULT_FIRE_TICKS,
    };
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx.host.actions().set_fire_ticks(uuid, ticks) {
            return Err(refused("set the fire"));
        }
    }
    next()
}

fn extinguish(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx.host.actions().set_fire_ticks(uuid, 0) {
            return Err(refused("put the fire out"));
        }
    }
    next()
}

fn allow_flight(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx.host.actions().set_abilities(uuid, Some(true), None, None, None) {
            return Err(refused("allow flight"));
        }
    }
    next()
}

/// Taking the flight away also lands the player, as Bukkit's `setAllowFlight(false)` does.
fn disallow_flight(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx
            .host
            .actions()
            .set_abilities(uuid, Some(false), Some(false), None, None)
        {
            return Err(refused("disallow flight"));
        }
    }
    next()
}

/// `make %players% fly` starts flying (allowing flight too); mark 1 stops it.
fn make_fly(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let start = !m.has_mark(1);
    for (uuid, _) in players_in(cx, m, 0)? {
        let may_fly = start.then_some(true);
        if !cx
            .host
            .actions()
            .set_abilities(uuid, may_fly, Some(start), None, None)
        {
            return Err(refused("change the flight"));
        }
    }
    next()
}

fn flag(cx: &mut Ctx<'_>, m: &Matched, flag: &str, on: bool) -> Result<Flow, RunError> {
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx.host.actions().set_flag(uuid, flag, on) {
            return Err(refused(&format!("set {flag}")));
        }
    }
    next()
}

fn force_respawn(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (uuid, _) in players_in(cx, m, 0)? {
        if !cx.host.actions().respawn(uuid) {
            return Err(refused("respawn the player"));
        }
    }
    next()
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    fn host() -> Recorder {
        Recorder::with_players(&[(1, "Alice")])
    }

    #[test]
    fn fire_and_flight_effects() {
        let h = host();
        let logs = run(
            &h,
            "\tignite Alice for 3 seconds\n\tignite Alice\n\textinguish Alice\n\tallow flight for Alice\n\tdisable fly to Alice\n\tmake Alice fly\n\tmake Alice stop flying\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "fire 1 60",
                "fire 1 160",
                "fire 1 0",
                "abilities 1 may_fly=Some(true) flying=None fly_speed=None walk_speed=None",
                "abilities 1 may_fly=Some(false) flying=Some(false) fly_speed=None walk_speed=None",
                "abilities 1 may_fly=Some(true) flying=Some(true) fly_speed=None walk_speed=None",
                "abilities 1 may_fly=None flying=Some(false) fly_speed=None walk_speed=None",
            ]
        );
    }

    #[test]
    fn flags_and_respawn() {
        let h = host();
        let logs = run(
            &h,
            "\tmake Alice invulnerable\n\tmake Alice vulnerable\n\tsilence Alice\n\tunsilence Alice\n\tforce Alice to respawn\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "flag 1 invulnerable true",
                "flag 1 invulnerable false",
                "flag 1 silent true",
                "flag 1 silent false",
                "force respawn 1",
            ]
        );
    }
}
