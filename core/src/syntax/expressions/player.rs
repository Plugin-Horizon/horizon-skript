//! Player properties: name, display name, UUID, health, food, level, game mode, held item,
//! inventory, location, world, the players online, and the block a player looks at. Ported from
//! Skript's `ExprName`, `ExprUUID`, `ExprHealth`, `ExprFoodLevel`, `ExprLevel`, `ExprTool`,
//! `ExprInventory`, `ExprTargetedBlock` and `ExprWorld`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{format_uuid, location_at, num, player_of, slot, state_at, within};
use crate::value::Value;

/// How far `target block` looks, in blocks.
const TARGET_RANGE: f64 = 100.0;

fn name(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Text(s.name))
}

fn display_name(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Text(s.display_name))
}

fn uuid(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, _) = player_of(&slot(cx, m, 0)?)?;
    Ok(Value::Text(format_uuid(uuid)))
}

fn health(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(num(s.health))
}

fn max_health(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(num(s.max_health))
}

fn food(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Int(s.food))
}

fn level(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Int(s.level))
}

fn gamemode(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Text(s.gamemode))
}

fn held_item(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::ItemType {
        id: s.held,
        amount: 1,
        name: None,
    })
}

fn location(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::Location(s.location))
}

/// The filled slots of the inventory, one item type per stack.
fn inventory(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, s) = state_at(cx, m, 0)?;
    Ok(Value::List(
        s.inventory
            .into_iter()
            .map(|st| Value::ItemType {
                id: st.id,
                amount: st.amount,
                name: st.name,
            })
            .collect(),
    ))
}

fn online_players(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(Value::List(
        cx.host
            .online_players()
            .into_iter()
            .map(|(uuid, name)| Value::Player { uuid, name })
            .collect(),
    ))
}

/// `all players in radius R around LOCATION`: the online players, NPCs not among them, within R of
/// the location (in its world).
fn players_in_radius(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let radius = slot(cx, m, 0)?
        .as_f64()
        .ok_or_else(|| RunError::new("a radius is a number"))?;
    let center = location_at(cx, m, 1)?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (uuid, name) in cx.host.online_players() {
        if let Some(state) = cx.host.player_state(uuid) {
            if within(&center, &state.location, radius) && seen.insert(uuid) {
                out.push(Value::Player { uuid, name });
            }
        }
    }
    // Skript on Paper looks up player-type ENTITIES near the location (`getNearbyEntities`), so a
    // Citizens NPC (a player entity, not an online player) counts too: the stage's auras stay on
    // because their own NPC stands at the centre.
    for e in cx.host.entities_in_world(&center.world) {
        if e.kind == "minecraft:player"
            && within(&center, &e.location, radius)
            && seen.insert(e.uuid)
        {
            out.push(Value::Player {
                uuid: e.uuid,
                name: e.name.clone(),
            });
        }
    }
    Ok(Value::List(out))
}

/// The item type of the block a player looks at.
fn target_block(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, _) = player_of(&slot(cx, m, 0)?)?;
    let Some(loc) = cx.host.target_block(uuid, TARGET_RANGE) else {
        return Ok(Value::Null);
    };
    Ok(match cx.host.block_type_at(&loc) {
        Some(id) => Value::ItemType {
            id,
            amount: 1,
            name: None,
        },
        None => Value::Null,
    })
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "name",
        &["[the] name of %player%", "%player%'[s] name"],
        0,
        "text",
        name,
    )?;
    reg.add_expression(
        "display name",
        &[
            "[the] display[ ]name of %player%",
            "%player%'[s] display[ ]name",
        ],
        0,
        "text",
        display_name,
    )?;
    reg.add_expression(
        "uuid",
        &["[the] uuid of %player%", "%player%'[s] uuid"],
        0,
        "text",
        uuid,
    )?;
    reg.add_expression(
        "health",
        &["[the] health of %player%", "%player%'[s] health"],
        0,
        "number",
        health,
    )?;
    reg.add_expression(
        "max health",
        &[
            "[the] max[imum] health of %player%",
            "%player%'[s] max[imum] health",
        ],
        0,
        "number",
        max_health,
    )?;
    reg.add_expression(
        "food level",
        &[
            "[the] (food|hunger)[ level] of %player%",
            "%player%'[s] (food|hunger)[ level]",
        ],
        0,
        "integer",
        food,
    )?;
    reg.add_expression(
        "level",
        &[
            "[the] [(xp|exp[erience]) ]level of %player%",
            "%player%'[s] [(xp|exp[erience]) ]level",
        ],
        0,
        "integer",
        level,
    )?;
    reg.add_expression(
        "game mode",
        &["[the] game[ ]mode of %player%", "%player%'[s] game[ ]mode"],
        0,
        "text",
        gamemode,
    )?;
    reg.add_expression(
        "held item",
        &[
            "[the] (tool|held item) of %player%",
            "%player%'[s] (tool|held item)",
        ],
        0,
        "item type",
        held_item,
    )?;
    reg.add_expression(
        "location of player",
        &["[the] location of %player%", "%player%'[s] location"],
        0,
        "location",
        location,
    )?;
    reg.add_expression(
        "inventory",
        &["[the] inventory of %player%", "%player%'[s] inventory"],
        0,
        "object",
        inventory,
    )?;
    reg.add_expression(
        "online players",
        &["[all] [the] [online] players"],
        0,
        "object",
        online_players,
    )?;
    reg.add_expression(
        "players in radius",
        &[
            "all players in radius %number% (around|of) %location%",
            "players in radius %number% (around|of) %location%",
        ],
        0,
        "object",
        players_in_radius,
    )?;
    reg.add_expression(
        "target block",
        &[
            "[the] target[ed] block of %player%",
            "%player%'[s] target[ed] block",
        ],
        0,
        "item type",
        target_block,
    )?;
    Ok(())
}

#[cfg(test)]
mod radius_tests {
    use crate::host::{PlayerState, Recorder};
    use crate::syntax::support::testing::run;
    use crate::value::Location;

    fn state(x: f64) -> PlayerState {
        PlayerState {
            name: String::new(),
            display_name: String::new(),
            location: Location {
                world: "stage".into(),
                x,
                y: 0.0,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            health: 20.0,
            max_health: 20.0,
            food: 20,
            level: 0,
            gamemode: "adventure".into(),
            flying: false,
            can_fly: false,
            sneaking: false,
            sprinting: false,
            swimming: false,
            op: false,
            held: "minecraft:air".into(),
            inventory: vec![],
        }
    }

    #[test]
    fn an_npc_player_entity_counts_in_players_in_radius() {
        let mut h = Recorder::with_players(&[(1, "Alice")]);
        h.states = vec![(1, state(40.0))];
        h.entities = vec![crate::host::EntityInfo {
            uuid: 99,
            kind: "minecraft:player".into(),
            name: "Lion".into(),
            location: crate::value::Location {
                world: "stage".into(),
                x: 1.0,
                y: 0.0,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            },
        }];
        let logs = run(
            &h,
            "\tif size of (all players in radius 5 around location(0, 0, 0, world \"stage\")) is 1:\n\t\tbroadcast \"npc counts\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(*h.broadcasts.borrow(), vec!["npc counts".to_owned()]);
    }

    #[test]
    fn players_in_radius_are_the_online_players_near_the_centre() {
        let mut h = Recorder::with_players(&[(1, "Alice"), (2, "Bob")]);
        h.states = vec![(1, state(1.0)), (2, state(40.0))];
        let logs = run(
            &h,
            "\tif size of (all players in radius 5 around location(0, 0, 0, world \"stage\")) is 1:\n\t\tbroadcast \"near\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(*h.broadcasts.borrow(), vec!["near".to_owned()]);
    }
}
