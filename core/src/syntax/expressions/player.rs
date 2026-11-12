//! Player properties: name, display name, UUID, health, food, level, game mode, held item,
//! inventory, location, world, the players online, and the block a player looks at. Ported from
//! Skript's `ExprName`, `ExprUUID`, `ExprHealth`, `ExprFoodLevel`, `ExprLevel`, `ExprTool`,
//! `ExprInventory`, `ExprTargetedBlock` and `ExprWorld`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{format_uuid, num, player_of, slot, state_at};
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

/// The item type of the block a player looks at.
fn target_block(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, _) = player_of(&slot(cx, m, 0)?)?;
    let Some(loc) = cx.host.target_block(uuid, TARGET_RANGE) else {
        return Ok(Value::Null);
    };
    Ok(match cx.host.block_type_at(&loc) {
        Some(id) => Value::ItemType { id, amount: 1 },
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
