//! Conditions about what the engine reports of a player or an entity: burning, on the ground,
//! invisible, frozen, blocking, sleeping, riding, gliding, glowing, silent, invulnerable, and
//! line of sight. Ported from Skript's `CondIsBurning`, `CondIsOnGround`, `CondIsInvisible`,
//! `CondIsFrozen`, `CondIsBlocking`, `CondIsSleeping`, `CondIsRiding`, `CondIsGliding`,
//! `CondIsGlowing`, `CondIsSilent`, `CondIsInvulnerable` and `CondHasLineOfSight`.
//!
//! Only players (and NPC player-entities) are values in the port, so these hold for them.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::host::Vitals;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Condition, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{items, negated, player_of, slot};

/// `%players% is <property>`: every player has the property (or, negated, none has it).
struct Property(fn(&Vitals) -> bool);

impl Condition for Property {
    fn check(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
        let players = items(slot(cx, m, 0)?);
        if players.is_empty() {
            return Ok(negated(m));
        }
        let mut all = true;
        for p in &players {
            let (uuid, _) = player_of(p)?;
            if !cx.host.vitals(uuid).is_some_and(|v| (self.0)(&v)) {
                all = false;
            }
        }
        Ok(all != negated(m))
    }
}

fn property(
    reg: &mut Registry,
    name: &'static str,
    words: &str,
    check: fn(&Vitals) -> bool,
) -> Result<(), PatternError> {
    let pattern = format!("%players% (is|are)(1¦ not|1¦n't|) {words}");
    reg.add_condition(name, &[pattern.as_str()], -1, Property(check))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    property(reg, "is burning", "(burning|ignited|on fire)", |v| v.fire_ticks > 0)?;
    property(reg, "is on ground", "on [the] ground", |v| v.on_ground)?;
    property(reg, "is invisible", "invisible", |v| v.invisible)?;
    property(reg, "is visible", "visible", |v| !v.invisible)?;
    property(reg, "is frozen", "frozen", |v| v.frozen_ticks > 0)?;
    property(
        reg,
        "is blocking",
        "(blocking|defending) [with [a] shield]",
        |v| v.blocking,
    )?;
    property(reg, "is sleeping", "sleeping", |v| v.sleeping)?;
    property(reg, "is riding", "riding", |v| v.vehicle.is_some())?;
    property(reg, "is gliding", "gliding", |v| v.gliding)?;
    property(reg, "is glowing", "glowing", |v| v.glowing)?;
    property(reg, "is silent", "silent", |v| v.silent)?;
    property(reg, "is invulnerable", "(invulnerable|invincible)", |v| v.invulnerable)?;
    reg.add_condition(
        "has line of sight",
        &[
            "%players% (has|have) [a] [direct] line of sight to %player%",
            "%players% (1¦does(n't| not) have|1¦(has|have) no) [a] [direct] line of sight to %player%",
        ],
        -1,
        line_of_sight,
    )?;
    Ok(())
}

/// Every looker sees the target (or, with a negation mark, none does).
fn line_of_sight(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let lookers = items(slot(cx, m, 0)?);
    let (target, _) = player_of(&slot(cx, m, 1)?)?;
    if lookers.is_empty() {
        return Ok(negated(m));
    }
    let mut all = true;
    for l in &lookers {
        let (uuid, _) = player_of(l)?;
        if cx.host.line_of_sight(uuid, target) != Some(true) {
            all = false;
        }
    }
    Ok(all != negated(m))
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::{Recorder, Vitals};
    use crate::horizon_skript_core::syntax::support::testing::run;

    fn said(h: &Recorder, body: &str) -> Vec<String> {
        let logs = run(h, body);
        assert!(logs.is_empty(), "{logs:?}");
        h.broadcasts.borrow().clone()
    }

    fn host() -> Recorder {
        let mut h = Recorder::with_players(&[(1, "Alice"), (2, "Bob")]);
        h.vitals = vec![
            (
                1,
                Vitals {
                    fire_ticks: 20,
                    on_ground: true,
                    invisible: true,
                    vehicle: Some(2),
                    ..Vitals::default()
                },
            ),
            (2, Vitals::default()),
        ];
        h.sight = vec![(1, 2)];
        h
    }

    #[test]
    fn properties_hold_and_negate() {
        let h = host();
        let out = said(
            &h,
            "\tif Alice is burning:\n\t\tbroadcast \"a burning\"\n\tif Bob is not burning:\n\t\tbroadcast \"b not burning\"\n\tif Alice is on the ground:\n\t\tbroadcast \"a ground\"\n\tif Alice is invisible:\n\t\tbroadcast \"a invisible\"\n\tif Bob is visible:\n\t\tbroadcast \"b visible\"\n\tif Alice is riding:\n\t\tbroadcast \"a riding\"\n\tif Bob isn't riding:\n\t\tbroadcast \"b not riding\"\n",
        );
        assert_eq!(
            out,
            vec![
                "a burning",
                "b not burning",
                "a ground",
                "a invisible",
                "b visible",
                "a riding",
                "b not riding"
            ]
        );
    }

    #[test]
    fn line_of_sight_is_one_way() {
        let h = host();
        let out = said(
            &h,
            "\tif Alice has line of sight to Bob:\n\t\tbroadcast \"sees\"\n\tif Bob does not have line of sight to Alice:\n\t\tbroadcast \"blind\"\n",
        );
        assert_eq!(out, vec!["sees", "blind"]);
    }
}
