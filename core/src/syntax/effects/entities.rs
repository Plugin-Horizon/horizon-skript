//! Effects on players and entities that the engine answers by uuid, so they reach NPCs too:
//! potion effects (`apply potion effect of <name> [of tier N] to ... for T`, `hide particles for
//! ...'s potion effects`), items into an inventory (`give ... to inventory of ...`), equipment
//! (`set helmet of ... to ...`) and the respawn location. Patterns follow Skript's `EffPotionEffect`,
//! `EffPotionParticles`, `EffChange` (inventory and equipment) and `EffSetRespawnLocation`.

use super::{items_of, next, players_in, slot};
use crate::builtins::ticks_of;
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{location_at, player_of};
use crate::value::Value;

/// Potion names as Skript writes them, and the vanilla effect ids. Listed longest first, so that
/// `fire resistance` is read before `resistance`.
pub const POTIONS: &[(&str, &str)] = &[
    ("hero of the village", "hero_of_the_village"),
    ("conduit power", "conduit_power"),
    ("dolphins grace", "dolphins_grace"),
    ("water breathing", "water_breathing"),
    ("fire resistance", "fire_resistance"),
    ("instant health", "instant_health"),
    ("instant damage", "instant_damage"),
    ("mining fatigue", "mining_fatigue"),
    ("slow falling", "slow_falling"),
    ("health boost", "health_boost"),
    ("night vision", "night_vision"),
    ("jump boost", "jump_boost"),
    ("bad omen", "bad_omen"),
    ("bad luck", "unluck"),
    ("regeneration", "regeneration"),
    ("invisibility", "invisibility"),
    ("absorption", "absorption"),
    ("resistance", "resistance"),
    ("blindness", "blindness"),
    ("darkness", "darkness"),
    ("levitation", "levitation"),
    ("saturation", "saturation"),
    ("slowness", "slowness"),
    ("strength", "strength"),
    ("weakness", "weakness"),
    ("glowing", "glowing"),
    ("nausea", "nausea"),
    ("hunger", "hunger"),
    ("poison", "poison"),
    ("wither", "wither"),
    ("speed", "speed"),
    ("haste", "haste"),
    ("luck", "luck"),
];

/// The vanilla id of a potion name, `None` for an unknown one.
pub fn potion_id(name: &str) -> Option<&'static str> {
    POTIONS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
        .map(|(_, id)| *id)
}

/// Equipment slots a script can name, as the engine's slot names.
const EQUIPMENT: &str = "helmet|chestplate|leggings|boots";

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    let names: Vec<&str> = POTIONS.iter().map(|(n, _)| *n).collect();
    let potion = format!(
        "apply potion effect of <({})>[ of tier %-number%] to %players%[ for %-timespan%][ (1¦hide|1¦without|2¦with) particles]",
        names.join("|")
    );
    reg.add_effect(
        "apply potion effect of",
        &[potion.as_str()],
        -1,
        apply_potion,
    )?;
    reg.add_effect(
        "hide potion particles",
        &["hide particles for %players%'[s] potion effects"],
        -1,
        hide_potion_particles,
    )?;
    reg.add_effect(
        "give to inventory",
        &["(give|add) %item types% to inventory of %players%"],
        -1,
        give_to_inventory,
    )?;
    let equipment = format!("set <({EQUIPMENT})> of %players% to %item type%");
    reg.add_effect("set equipment", &[equipment.as_str()], -1, set_equipment)?;
    reg.add_effect(
        "set respawn location",
        &["set [the] respawn location [of %-players%] to %location%"],
        -1,
        set_respawn,
    )?;
    Ok(())
}

/// `apply potion effect of <name> [of tier N] to TARGETS [for T] [hide|without|with particles]`.
/// Slots: 0 tier, 1 targets, 2 time. Tier N is amplifier N-1; no time lasts until it is removed.
/// The name is regex group 0; mark 1 (`hide`, `without`) hides the particles.
fn apply_potion(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let name = m.regex(0).unwrap_or_default();
    let id = potion_id(name)
        .ok_or_else(|| RunError::new(format!("there is no potion effect {name}")))?;
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
    let particles = !m.has_mark(1);
    for target in items_of(slot(cx, m, 1)?) {
        let (uuid, _) = player_of(&target)?;
        cx.host
            .actions()
            .apply_effect(uuid, id, (tier.max(1) - 1) as i32, duration, particles);
    }
    next()
}

/// `hide particles for TARGETS' potion effects`.
fn hide_potion_particles(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for target in items_of(slot(cx, m, 0)?) {
        let (uuid, _) = player_of(&target)?;
        cx.host.actions().hide_effect_particles(uuid);
    }
    next()
}

/// `give ITEMS to inventory of TARGETS`. Slots: 0 items, 1 targets.
fn give_to_inventory(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let items = items_of(slot(cx, m, 0)?);
    for target in items_of(slot(cx, m, 1)?) {
        let (uuid, _) = player_of(&target)?;
        for item in &items {
            match item {
                Value::ItemType { id, amount, .. } => {
                    cx.host.actions().give_item(uuid, id, *amount);
                }
                other => {
                    return Err(RunError::new(format!(
                        "give needs item types, got {}",
                        other.class()
                    )));
                }
            }
        }
    }
    next()
}

/// `set helmet of TARGETS to ITEM`: air empties the slot. Slots: 0 targets, 1 item. The slot name
/// is regex group 0.
fn set_equipment(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let slot_name = m.regex(0).unwrap_or_default().to_owned();
    let (item, name) = match slot(cx, m, 1)? {
        Value::ItemType { id, .. } if id == "minecraft:air" => (None, None),
        Value::ItemType { id, name, .. } => (Some(id), name),
        other => {
            return Err(RunError::new(format!(
                "{slot_name} needs an item type, got {}",
                other.class()
            )));
        }
    };
    for target in items_of(slot(cx, m, 0)?) {
        let (uuid, _) = player_of(&target)?;
        cx.host
            .actions()
            .set_equipment(uuid, &slot_name, item.as_deref(), name.as_deref());
    }
    next()
}

/// `set the respawn location [of PLAYERS] to LOCATION`; the event's player when no player is named.
fn set_respawn(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let at = location_at(cx, m, 1)?;
    for (uuid, _) in players_in(cx, m, 0)? {
        cx.host.actions().set_respawn(uuid, &at);
    }
    next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Recorder;
    use crate::syntax::support::testing::{acted, run};

    const ALICE: u128 = 1;

    fn host() -> Recorder {
        Recorder::with_players(&[(ALICE, "Alice")])
    }

    #[test]
    fn longer_potion_names_come_first() {
        let at = |n: &str| POTIONS.iter().position(|(x, _)| *x == n).unwrap();
        assert!(at("fire resistance") < at("resistance"));
        assert_eq!(potion_id("Fire Resistance"), Some("fire_resistance"));
        assert_eq!(potion_id("unicorn"), None);
    }

    #[test]
    fn apply_a_tiered_potion_for_a_time() {
        let h = host();
        let logs = run(
            &h,
            "\tapply potion effect of regeneration of tier 2 to Alice for 20 seconds\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec!["effect 1 regeneration 1 Some(400) particles=true"]
        );
    }

    #[test]
    fn particles_can_be_hidden_and_no_time_lasts_forever() {
        let h = host();
        let logs = run(
            &h,
            "\tapply potion effect of absorption to Alice without particles\n\tapply potion effect of speed of tier 3 to Alice hide particles\n\thide particles for Alice's potion effects\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "effect 1 absorption 0 None particles=false".to_owned(),
                "effect 1 speed 2 None particles=false".to_owned(),
                "hide effect particles 1".to_owned(),
            ]
        );
    }

    #[test]
    fn give_puts_items_in_the_inventory_of_every_target() {
        let h = host();
        let logs = run(&h, "\tgive 3 golden apple to inventory of all players\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&h), vec!["give 1 minecraft:golden_apple 3"]);
    }

    #[test]
    fn helmet_is_set_and_air_empties_it() {
        let h = host();
        let logs = run(
            &h,
            "\tset helmet of Alice to carved pumpkin\n\tset helmet of Alice to air\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "equip 1 helmet Some(\"minecraft:carved_pumpkin\")".to_owned(),
                "equip 1 helmet None".to_owned(),
            ]
        );
    }

    #[test]
    fn respawn_location_of_a_named_player() {
        let h = host();
        let logs = run(
            &h,
            "\tset the respawn location of Alice to location(5, 60, 7, world \"stage\")\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&h), vec!["respawn 1 stage at 5, 60, 7"]);
    }
}
