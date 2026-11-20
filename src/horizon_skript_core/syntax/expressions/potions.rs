//! Potion effects of a player: `potion effects of %players%` (a list of potion effect values),
//! `potion effect type of %potion effect%`, and the `has potion` condition. Ported from Skript's
//! `ExprPotionEffects`, `CondHasPotion` and the potion effect and potion effect type classes. The
//! apply and remove effects are in effects/potions.rs, which also holds the potion names.
//!
//! A potion effect value is made only by `potion effects of`; there is no literal form of it.
//! Differences from Skript: the time left is in milliseconds (None when it lasts for ever), and
//! the names are the ones of the potion table (`speed`, `bad luck`, ...), not every Skript name.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::effects::entities::{POTIONS, potion_id};
use crate::horizon_skript_core::syntax::support::{items, player_of, slot};
use crate::horizon_skript_core::value::Value;

/// Milliseconds per game tick.
const MS_PER_TICK: i64 = 50;

/// `[the] [active] potion effects of %players%`: every effect of every player, in the order the
/// engine keeps them.
fn effects_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut out = Vec::new();
    for target in items(slot(cx, m, 0)?) {
        let (uuid, name) = player_of(&target)?;
        let effects = cx
            .host
            .potion_effects(uuid)
            .ok_or_else(|| RunError::new(format!("{name} is not online")))?;
        for p in effects {
            out.push(Value::PotionEffect {
                id: p.id,
                tier: i64::from(p.amplifier) + 1,
                duration: (p.duration_ticks >= 0)
                    .then(|| i64::from(p.duration_ticks) * MS_PER_TICK),
            });
        }
    }
    Ok(Value::List(out))
}

/// `potion effect type of %potion effect%`.
fn type_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    match slot(cx, m, 0)? {
        Value::PotionEffect { id, .. } => Ok(Value::PotionEffectType(id)),
        other => Err(RunError::new(format!(
            "expected a potion effect, got {}",
            other.class()
        ))),
    }
}

/// `%players% has potion <name>`: every player has the effect.
fn has(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let name = m.regex(0).unwrap_or_default();
    let id = potion_id(name)
        .ok_or_else(|| RunError::new(format!("there is no potion effect {name}")))?;
    let mut all = true;
    for target in items(slot(cx, m, 0)?) {
        let (uuid, player) = player_of(&target)?;
        let effects = cx
            .host
            .potion_effects(uuid)
            .ok_or_else(|| RunError::new(format!("{player} is not online")))?;
        all &= effects.iter().any(|p| p.id == id);
    }
    Ok(all)
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    let names: Vec<&str> = POTIONS.iter().map(|(n, _)| *n).collect();
    let has_pattern = format!("%players% (has|have) potion <({})>", names.join("|"));
    reg.add_expression(
        "potion effects",
        &["[the] [active] potion effects of %players%"],
        0,
        "object",
        effects_of,
    )?;
    reg.add_expression(
        "potion effect type",
        &["[the] potion effect type of %potion effect%"],
        0,
        "potion effect type",
        type_of,
    )?;
    reg.add_condition("has potion", &[has_pattern.as_str()], -1, has)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::{PotionInfo, Recorder};
    use crate::horizon_skript_core::syntax::support::testing::run;

    fn alice_with_speed_and_regeneration() -> Recorder {
        let mut host = Recorder::with_players(&[(7, "Alice")]);
        host.potions = vec![(
            7,
            vec![
                PotionInfo {
                    id: "speed".into(),
                    amplifier: 1,
                    duration_ticks: 200,
                },
                PotionInfo {
                    id: "regeneration".into(),
                    amplifier: 0,
                    duration_ticks: -1,
                },
            ],
        )];
        host
    }

    #[test]
    fn effects_read_with_tier_and_time() {
        let host = alice_with_speed_and_regeneration();
        let logs = run(
            &host,
            "\tloop potion effects of Alice:\n\t\tbroadcast \"%loop-value%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *host.broadcasts.borrow(),
            vec![
                "speed of tier 2".to_owned(),
                "regeneration of tier 1".to_owned()
            ]
        );
    }

    #[test]
    fn has_potion_checks_the_effect_id() {
        let host = alice_with_speed_and_regeneration();
        let logs = run(
            &host,
            "\tif Alice has potion speed:\n\t\tbroadcast \"yes\"\n\tif Alice has potion slowness:\n\t\tbroadcast \"no\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(*host.broadcasts.borrow(), vec!["yes".to_owned()]);
    }
}
