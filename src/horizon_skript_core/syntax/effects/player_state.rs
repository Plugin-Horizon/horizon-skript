//! Player state: `feed`, `look at`, `make invisible`, `ride`/`dismount`, `equip`/`unequip`, clear
//! potion effects, `stop sound` and `ip-ban`. Patterns follow Skript's `EffFeed`, `EffLook`,
//! `EffInvisible`, `EffVehicle`, `EffEquip`, `EffStopSound` and `EffBan` (IP form). Everything
//! except `feed` runs a vanilla command as the console.
//!
//! Skript's entity forms (`%livingentities%`, `%entity%'s eyes`, head rotation speed, `eject
//! passengers of`) are not here: scripts see players, not other entities.

use super::{console, next, number_in, players_in, slot, texts_in};
use crate::horizon_skript_core::classes;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::{Value, format_number};

/// The equipment slots the host reads, with the vanilla `item replace` slot of each.
const ARMOUR: [(&str, &str); 4] = [
    ("helmet", "armor.head"),
    ("chestplate", "armor.chest"),
    ("leggings", "armor.legs"),
    ("boots", "armor.feet"),
];

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "feed",
        &["feed [the] %players% [by %-number% [beef[s]]]"],
        0,
        feed,
    )?;
    reg.add_effect(
        "look at",
        &["(force|make) %players% [to] (face [towards]|look [(at|towards)]) %location/player%"],
        0,
        look_at,
    )?;
    reg.add_effect(
        "make invisible",
        &["make %players% (invisible|not visible)"],
        0,
        make_invisible,
    )?;
    reg.add_effect(
        "make visible",
        &["make %players% (visible|not invisible)"],
        0,
        make_visible,
    )?;
    reg.add_effect(
        "ride",
        &["(make|let|force) %players% [to] (ride|mount) [(in|on)] %player%"],
        0,
        ride,
    )?;
    reg.add_effect(
        "dismount",
        &["(make|let|force) %players% [to] (dismount|leave [(from|of)] [(any|the|their|his|her)] vehicle[s])"],
        0,
        dismount,
    )?;
    reg.add_effect(
        "equip",
        &[
            "equip [%-players%] with %item types%",
            "make %players% wear %item types%",
        ],
        0,
        equip,
    )?;
    reg.add_effect(
        "unequip items",
        &["unequip %item types% from %players%"],
        0,
        unequip_items,
    )?;
    reg.add_effect(
        "unequip armour",
        &["unequip %players%'[s] (armo[u]r|equipment)"],
        0,
        unequip_armour,
    )?;
    // Before the builtin `clear %object%` and the `remove ... from` item effect.
    reg.add_effect(
        "clear potion effects",
        &["(clear|remove|delete) (all[ of][ the] |the |)[active ]potion effects (of|from|on) %players%"],
        -2,
        clear_potion_effects,
    )?;
    reg.add_effect(
        "stop sound",
        &[
            "stop (all:all sound[s]|sound[s] %-texts%) [(from playing to|for) %-players%]",
            "stop playing sound[s] %texts% [(to|for) %-players%]",
        ],
        0,
        stop_sound,
    )?;
    reg.add_effect(
        "ip ban",
        &["ip(-| |)ban %players% [(by reason of|because [of]|on account of|due to) %-text%]"],
        0,
        ip_ban,
    )?;
    Ok(())
}

/// `feed players [by N beefs]`: adds N food points (all 20 when no number), capped at 20.
fn feed(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let by = number_in(cx, m, 1)?.unwrap_or(20.0) as i64;
    for (uuid, _) in players_in(cx, m, 0)? {
        let food = cx.host.player_state(uuid).map_or(0, |s| s.food);
        cx.host.actions().set_food(uuid, (food + by).clamp(0, 20));
    }
    next()
}

/// `make players look at LOCATION` (or at another player's eyes): vanilla `tp ... facing`, run
/// from the player's own position so only the direction changes.
fn look_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let facing = match slot(cx, m, 1)? {
        Value::Location(l) => format!(
            "facing {} {} {}",
            format_number(l.x),
            format_number(l.y),
            format_number(l.z)
        ),
        Value::Player { name, .. } => format!("facing entity {name} eyes"),
        other => {
            return Err(RunError::new(format!(
                "expected a location or a player, got {}",
                other.class()
            )));
        }
    };
    for (_, name) in players_in(cx, m, 0)? {
        console(
            cx,
            &format!("execute as {name} at {name} run tp {name} ~ ~ ~ {facing}"),
        );
    }
    next()
}

/// `make players invisible`: the invisibility effect without a time limit and without particles
/// (Skript sets the entity flag; vanilla has the effect only).
fn make_invisible(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(
            cx,
            &format!("effect give {name} minecraft:invisibility infinite 0 true"),
        );
    }
    next()
}

fn make_visible(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("effect clear {name} minecraft:invisibility"));
    }
    next()
}

/// `make players ride player`: vanilla `ride <rider> mount <vehicle>`. Slots: 0 riders, 1 vehicle.
fn ride(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let (_, vehicle) = super::player_of(&slot(cx, m, 1)?)?;
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("ride {name} mount {vehicle}"));
    }
    next()
}

fn dismount(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("ride {name} dismount"));
    }
    next()
}

/// The vanilla slot an item is worn in: armour by its name, `elytra` on the chest, heads and
/// pumpkins on the head, a shield in the off hand, anything else in the main hand.
fn slot_of(id: &str) -> &'static str {
    let path = classes::short_id(id);
    if path.ends_with("_helmet")
        || path.ends_with("_head")
        || path.ends_with("_skull")
        || path == "carved_pumpkin"
    {
        "armor.head"
    } else if path.ends_with("_chestplate") || path == "elytra" {
        "armor.chest"
    } else if path.ends_with("_leggings") {
        "armor.legs"
    } else if path.ends_with("_boots") {
        "armor.feet"
    } else if path == "shield" {
        "weapon.offhand"
    } else {
        "weapon.mainhand"
    }
}

/// `equip players with items`: each item goes into its slot, replacing what is there.
fn equip(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let items = super::items_of(slot(cx, m, 1)?);
    for (_, name) in players_in(cx, m, 0)? {
        for item in &items {
            match item {
                Value::ItemType { id, .. } => console(
                    cx,
                    &format!("item replace entity {name} {} with {id}", slot_of(id)),
                ),
                other => {
                    return Err(RunError::new(format!(
                        "expected an item type, got {}",
                        other.class()
                    )));
                }
            }
        }
    }
    next()
}

/// `unequip items from players`: an armour piece is taken off when the player wears that item.
/// Slots: 0 items, 1 players.
fn unequip_items(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let items = super::items_of(slot(cx, m, 0)?);
    for (uuid, name) in players_in(cx, m, 1)? {
        for item in &items {
            let Value::ItemType { id, .. } = item else {
                return Err(RunError::new(format!(
                    "expected an item type, got {}",
                    item.class()
                )));
            };
            for (host_slot, vanilla) in ARMOUR {
                if cx.host.equipment(uuid, host_slot).is_some_and(|w| w.id == *id) {
                    console(cx, &format!("item replace entity {name} {vanilla} with minecraft:air"));
                }
            }
        }
    }
    next()
}

fn unequip_armour(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        for (_, vanilla) in ARMOUR {
            console(cx, &format!("item replace entity {name} {vanilla} with minecraft:air"));
        }
    }
    next()
}

fn clear_potion_effects(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("effect clear {name}"));
    }
    next()
}

/// `stop all sounds [for players]` and `stop sound "x" [for players]`. Slots: 0 sounds,
/// 1 players (every player when empty).
fn stop_sound(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let all = m.has_tag("all");
    let mut sounds = Vec::new();
    if !all {
        for raw in texts_in(cx, m, 0)? {
            let id = classes::normalize_id(&raw)
                .ok_or_else(|| RunError::new(format!("bad sound name {raw}")))?;
            sounds.push(id);
        }
    }
    let targets: Vec<String> = match slot(cx, m, 1)? {
        Value::Null => vec!["@a".to_owned()],
        _ => players_in(cx, m, 1)?.into_iter().map(|(_, n)| n).collect(),
    };
    for target in targets {
        if sounds.is_empty() {
            console(cx, &format!("stopsound {target}"));
        }
        for sound in &sounds {
            console(cx, &format!("stopsound {target} * {sound}"));
        }
    }
    next()
}

/// `ip-ban players [by reason of R]`: vanilla `ban-ip` with the player's name.
fn ip_ban(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let reason = match slot(cx, m, 1)? {
        Value::Null => String::new(),
        v => format!(" {}", v.to_text()),
    };
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("ban-ip {name}{reason}"));
    }
    next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon_skript_core::host::{ItemStack, Recorder};
    use crate::horizon_skript_core::syntax::effects::fixtures::state;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    fn alice() -> Recorder {
        Recorder::with_players(&[(1, "Alice"), (2, "Bob")])
    }

    #[test]
    fn items_go_to_their_slots() {
        assert_eq!(slot_of("minecraft:diamond_helmet"), "armor.head");
        assert_eq!(slot_of("minecraft:zombie_head"), "armor.head");
        assert_eq!(slot_of("minecraft:carved_pumpkin"), "armor.head");
        assert_eq!(slot_of("minecraft:elytra"), "armor.chest");
        assert_eq!(slot_of("minecraft:iron_leggings"), "armor.legs");
        assert_eq!(slot_of("minecraft:netherite_boots"), "armor.feet");
        assert_eq!(slot_of("minecraft:shield"), "weapon.offhand");
        assert_eq!(slot_of("minecraft:stick"), "weapon.mainhand");
    }

    #[test]
    fn feed_adds_food_up_to_twenty() {
        let mut host = alice();
        let mut s = state();
        s.food = 15;
        host.states.push((1, s));
        let logs = run(&host, "\tfeed Alice by 3 beefs\n\tfeed Alice\n");
        assert!(logs.is_empty(), "{logs:?}");
        // The state does not change between the two effects (the recorder keeps it fixed).
        assert_eq!(acted(&host), vec!["food 1 18", "food 1 20"]);
    }

    #[test]
    fn look_and_vehicle_commands() {
        let host = alice();
        let logs = run(
            &host,
            "\tmake Alice look at location(1, 2.5, 3, world \"world\")\n\tmake Alice look at Bob\n\tmake Alice ride Bob\n\tmake Alice dismount\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console execute as Alice at Alice run tp Alice ~ ~ ~ facing 1 2.5 3",
                "console execute as Alice at Alice run tp Alice ~ ~ ~ facing entity Bob eyes",
                "console ride Alice mount Bob",
                "console ride Alice dismount",
            ]
        );
    }

    #[test]
    fn invisibility_is_an_effect_without_particles() {
        let host = alice();
        let logs = run(&host, "\tmake Alice invisible\n\tmake Alice visible\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console effect give Alice minecraft:invisibility infinite 0 true",
                "console effect clear Alice minecraft:invisibility",
            ]
        );
    }

    #[test]
    fn equip_and_unequip() {
        let mut host = alice();
        host.equipment.push((
            1,
            "boots".into(),
            ItemStack {
                id: "minecraft:iron_boots".into(),
                amount: 1,
                name: None,
            },
        ));
        let logs = run(
            &host,
            "\tequip Alice with diamond helmet\n\tunequip iron boots from Alice\n\tunequip diamond helmet from Alice\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console item replace entity Alice armor.head with minecraft:diamond_helmet",
                "console item replace entity Alice armor.feet with minecraft:air",
            ]
        );
    }

    #[test]
    fn potion_effects_sounds_and_ip_ban() {
        let host = alice();
        let logs = run(
            &host,
            "\tclear all potion effects of Alice\n\tstop all sounds for Alice\n\tstop sound \"entity.cow.ambient\"\n\tip-ban Bob because of \"spam\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console effect clear Alice",
                "console stopsound Alice",
                "console stopsound @a * minecraft:entity.cow.ambient",
                "console ban-ip Bob spam",
            ]
        );
    }
}
