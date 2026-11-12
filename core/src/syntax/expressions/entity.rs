//! Entities: the entities of a world (`all entities in world ...`, `all entities in radius ...`),
//! their equipment (`helmet of ...`), a carved pumpkin with a name, and the yaw, pitch and head
//! location of a player or an entity. Ported from Skript's `ExprEntities`, `ExprArmorSlot`,
//! `ExprYaw`, `ExprPitch` and `ExprHeadLocation`.
//!
//! An entity that is a player-type entity (an NPC included) is a player value; any other entity
//! is its entity type, which `is a player` tells apart. The engine's entity list is
//! `Host::entities_in_world` (GAPS.md).

use crate::error::RunError;
use crate::host::ItemStack;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{entity_value, location_at, num, player_of, slot, state_of, within};
use crate::value::{Location, Value};

/// Eye height of a standing and of a sneaking player, in blocks.
const EYE_HEIGHT: f64 = 1.62;
const EYE_HEIGHT_SNEAKING: f64 = 1.27;

/// `all entities in world W`: every entity of the world, player-type entities as player values.
fn entities_in_world(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let world = match slot(cx, m, 0)? {
        Value::World(w) | Value::Text(w) => w,
        other => {
            return Err(RunError::new(format!(
                "expected a world, got {}",
                other.class()
            )));
        }
    };
    Ok(Value::List(
        cx.host
            .entities_in_world(&world)
            .iter()
            .map(entity_value)
            .collect(),
    ))
}

/// `all entities in radius R around LOCATION`: the entities of the location's world within R.
fn entities_in_radius(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let radius = slot(cx, m, 0)?
        .as_f64()
        .ok_or_else(|| RunError::new("a radius is a number"))?;
    let center = location_at(cx, m, 1)?;
    Ok(Value::List(
        cx.host
            .entities_in_world(&center.world)
            .iter()
            .filter(|e| within(&center, &e.location, radius))
            .map(entity_value)
            .collect(),
    ))
}

/// The item in an equipment slot of a player or an entity; `None` (null) when it is empty.
/// The slot name is regex group 0.
fn equipment(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let slot_name = m.regex(0).unwrap_or_default().to_owned();
    let (uuid, _) = player_of(&slot(cx, m, 0)?)?;
    Ok(match cx.host.equipment(uuid, &slot_name) {
        Some(ItemStack { id, name, .. }) => Value::ItemType {
            id,
            amount: 1,
            name,
        },
        None => Value::Null,
    })
}

/// `carved pumpkin named "..."`: the item with that custom name.
fn carved_pumpkin_named(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let name = slot(cx, m, 0)?.to_text();
    Ok(Value::ItemType {
        id: "minecraft:carved_pumpkin".to_owned(),
        amount: 1,
        name: Some(name),
    })
}

/// The yaw (`0`) or the pitch (`1`) of a location or of a player or entity (marks 1 and 2).
fn rotation(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let at = location_at(cx, m, 0)?;
    let v = if m.has_mark(1) { at.yaw } else { at.pitch };
    Ok(num(f64::from(v)))
}

/// The eye position of a player or an entity: its location raised to the eye height (1.27 when
/// it sneaks).
fn head_location(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let at = location_at(cx, m, 0)?;
    let v = slot(cx, m, 0)?;
    let sneaking =
        matches!(v, Value::Player { .. }) && state_of(cx, &v).is_ok_and(|(_, s)| s.sneaking);
    let eye = if sneaking {
        EYE_HEIGHT_SNEAKING
    } else {
        EYE_HEIGHT
    };
    Ok(Value::Location(Location {
        y: at.y + eye,
        ..at
    }))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "entities in world",
        &["all entities in world %text%"],
        0,
        "object",
        entities_in_world,
    )?;
    reg.add_expression(
        "entities in radius",
        &["all entities in radius %number% (around|of) %location%"],
        0,
        "object",
        entities_in_radius,
    )?;
    reg.add_expression(
        "equipment",
        &["[the] <(helmet|chestplate|leggings|boots)> of %player%"],
        0,
        "item type",
        equipment,
    )?;
    reg.add_expression(
        "carved pumpkin named",
        &["carved pumpkin named %text%"],
        0,
        "item type",
        carved_pumpkin_named,
    )?;
    reg.add_expression(
        "yaw",
        &[
            "[the] (1¦yaw|2¦pitch) of %location/player%",
            "%location/player%'[s] (1¦yaw|2¦pitch)",
        ],
        0,
        "number",
        rotation,
    )?;
    reg.add_expression(
        "head location",
        &["[the] head location of %location/player%"],
        0,
        "location",
        head_location,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{EntityInfo, PlayerState, Recorder};
    use crate::syntax::support::testing::{acted, run};

    const NPC: u128 = 9;

    fn at(world: &str, x: f64, y: f64, z: f64) -> Location {
        Location {
            world: world.into(),
            x,
            y,
            z,
            yaw: 90.0,
            pitch: -15.0,
        }
    }

    /// An NPC: a player-type entity that is not an online player.
    fn host() -> Recorder {
        let mut h = Recorder::default();
        h.entities = vec![
            EntityInfo {
                uuid: NPC,
                kind: "minecraft:player".into(),
                name: "Guide".into(),
                location: at("stage", 1.0, 46.0, 1.0),
            },
            EntityInfo {
                uuid: 10,
                kind: "minecraft:zombie".into(),
                name: "Zombie".into(),
                location: at("stage", 50.0, 46.0, 50.0),
            },
            EntityInfo {
                uuid: 11,
                kind: "minecraft:player".into(),
                name: "Far".into(),
                location: at("world", 1.0, 46.0, 1.0),
            },
        ];
        h.mobs = vec![(
            NPC,
            PlayerState {
                name: "Guide".into(),
                display_name: "Guide".into(),
                location: at("stage", 1.0, 46.0, 1.0),
                health: 20.0,
                max_health: 20.0,
                food: 20,
                level: 0,
                gamemode: "survival".into(),
                flying: false,
                can_fly: false,
                sneaking: false,
                sprinting: false,
                swimming: false,
                op: false,
                held: "minecraft:air".into(),
                inventory: vec![],
            },
        )];
        h.equipment = vec![(
            NPC,
            "helmet".into(),
            ItemStack {
                id: "minecraft:carved_pumpkin".into(),
                amount: 1,
                name: Some("Santa Hat".into()),
            },
        )];
        h
    }

    #[test]
    fn a_loop_over_a_world_sees_npcs_as_players_and_zombies_as_types() {
        let h = host();
        let logs = run(
            &h,
            "\tloop all entities in world \"stage\":\n\t\tbroadcast \"%loop-value%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["Guide".to_owned(), "zombie".to_owned()]
        );
    }

    #[test]
    fn yaw_pitch_and_head_location_of_an_npc() {
        let h = host();
        let logs = run(
            &h,
            "\tloop all entities in world \"stage\":\n\t\tif loop-value is a player:\n\t\t\tbroadcast \"%yaw of loop-value% %pitch of loop-value% %head location of loop-value%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["90 -15 stage at 1, 47.62, 1".to_owned()]
        );
    }

    #[test]
    fn radius_keeps_the_entities_near_the_centre() {
        let h = host();
        let logs = run(
            &h,
            "\tloop all entities in radius 5 around location(0, 46, 0, world \"stage\"):\n\t\tbroadcast \"%loop-value%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(*h.broadcasts.borrow(), vec!["Guide".to_owned()]);
    }

    #[test]
    fn helmet_of_an_npc_and_the_named_pumpkin() {
        let h = host();
        let logs = run(
            &h,
            "\tloop all entities in world \"stage\":\n\t\tif loop-value is a player:\n\t\t\tbroadcast \"%helmet of loop-value%\"\n\tset {r} to carved pumpkin named \"Santa Hat\"\n\tbroadcast \"%{r}%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["carved pumpkin".to_owned(), "carved pumpkin".to_owned()]
        );
    }

    /// rec_lobby_winter's winter_cosmetics.sk: the hat is put on where it is missing, and a
    /// plain carved pumpkin is not the named hat.
    #[test]
    fn the_winter_hat_goes_only_where_it_is_missing() {
        let script = "\tloop all entities in world \"stage\":\n\t\tif loop-entity is a player:\n\t\t\tif helmet of loop-entity is not carved pumpkin named \"Santa Hat\":\n\t\t\t\tset helmet of loop-entity to carved pumpkin named \"Santa Hat\"\n";
        let h = host();
        let logs = run(&h, script);
        assert!(logs.is_empty(), "{logs:?}");
        assert!(acted(&h).is_empty(), "{:?}", acted(&h));
        let mut h = host();
        h.equipment[0].2.name = None;
        let logs = run(&h, script);
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![format!(
                "equip {NPC} helmet Some(\"minecraft:carved_pumpkin\") named \"Santa Hat\""
            )]
        );
        // a plain item type still matches a named item
        let h = host();
        run(
            &h,
            "\tloop all entities in world \"stage\":\n\t\tif loop-entity is a player:\n\t\t\tif helmet of loop-entity is carved pumpkin:\n\t\t\t\tbroadcast \"plain\"\n",
        );
        assert_eq!(*h.broadcasts.borrow(), vec!["plain".to_owned()]);
    }
}
