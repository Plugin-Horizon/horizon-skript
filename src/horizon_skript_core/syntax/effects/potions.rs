//! Giving and removing a named potion effect: `apply <name> [potion] [[of] tier N] to %players% [for
//! T]` and `remove <name> [potion] from %players%`. Ported from Skript's `EffApplyPotionEffect` and
//! the removal of one potion effect. The names are the potion table of effects/entities.rs.
//! Writing goes through the host's `apply_effect` (the vanilla `effect give` command) and
//! `remove_potion_effect` (`living_remove_effect`, or `living_remove_all_effects`).

use super::entities::{POTIONS, potion_id};
use super::{items_of, next, players_in, slot};
use crate::horizon_skript_core::builtins::ticks_of;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::Value;

/// The vanilla id of the potion named by the pattern's first name group.
fn named(m: &Matched) -> Result<&'static str, RunError> {
    let name = m.regex(0).unwrap_or_default();
    potion_id(name).ok_or_else(|| RunError::new(format!("there is no potion effect {name}")))
}

/// `apply <name> [potion] [[of] tier N] to %players% [for T]`. Slots: 0 tier, 1 targets, 2 time.
/// Particles show, as Skript's default does.
fn apply(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let id = named(m)?;
    let tier = match slot(cx, m, 0)? {
        Value::Null => 1,
        v => v
            .as_i64()
            .ok_or_else(|| RunError::new(format!("a tier is a whole number, got {}", v.class())))?,
    };
    let duration = match slot(cx, m, 2)? {
        Value::Null => None,
        Value::Timespan(ms) => Some(ticks_of(ms)),
        other => {
            return Err(RunError::new(format!(
                "expected a timespan, got {}",
                other.class()
            )));
        }
    };
    for (uuid, _) in players_in(cx, m, 1)? {
        cx.host
            .actions()
            .apply_effect(uuid, id, (tier.max(1) - 1) as i32, duration, true);
    }
    next()
}

/// `remove <name> [potion] from %players%`: the effect goes from each player; a player without it
/// is left as it is.
fn remove(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let id = named(m)?;
    for target in items_of(slot(cx, m, 0)?) {
        let (uuid, _) = crate::horizon_skript_core::syntax::support::player_of(&target)?;
        cx.host.actions().remove_potion_effect(uuid, Some(id));
    }
    next()
}

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    let names: Vec<&str> = POTIONS.iter().map(|(n, _)| *n).collect();
    let apply_pattern = format!(
        "apply <({})> [potion] [[of] tier %-number%] to %players% [for %-timespan%]",
        names.join("|")
    );
    let remove_pattern = format!("remove <({})> [potion] from %players%", names.join("|"));
    reg.add_effect("apply potion", &[apply_pattern.as_str()], -1, apply)?;
    // Before `remove` of items (priority -1, registered earlier): `remove bad luck from Alice` is a potion.
    reg.add_effect("remove potion", &[remove_pattern.as_str()], -2, remove)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    #[test]
    fn apply_and_remove_name_the_potion() {
        let host = Recorder::with_players(&[(7, "Alice")]);
        let logs = run(
            &host,
            "\tapply speed of tier 2 to Alice for 10 seconds\n\tremove bad luck from Alice\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "effect 7 speed 1 Some(200) particles=true".to_owned(),
                "potion 7 remove unluck".to_owned(),
            ]
        );
    }
}
