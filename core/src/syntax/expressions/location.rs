//! Locations: the coordinates, the world, the distance between two places and the block at a
//! location. Ported from Skript's `ExprXYZComponent`, `ExprWorld`, `ExprDistance` and `ExprBlock`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{location_at, num, world_at};
use crate::value::Value;

/// `x of <location or player>`, `y of ...`, `z of ...` (marks 1, 2, 3).
fn coordinate(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let l = location_at(cx, m, 0)?;
    let v = if m.has_mark(1) {
        l.x
    } else if m.has_mark(2) {
        l.y
    } else {
        l.z
    };
    Ok(num(v))
}

fn world(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::World(world_at(cx, m, 0)?))
}

fn distance(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let a = location_at(cx, m, 0)?;
    let b = location_at(cx, m, 1)?;
    if !a.world.eq_ignore_ascii_case(&b.world) {
        return Err(RunError::new("the two locations are in different worlds"));
    }
    let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    Ok(Value::Num(d))
}

/// The block at a location, as its item type (amount 1). Null when the chunk is not loaded.
fn block_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let loc = location_at(cx, m, 0)?;
    Ok(match cx.host.block_type_at(&loc) {
        Some(id) => Value::ItemType { id, amount: 1 },
        None => Value::Null,
    })
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "coordinate",
        &["[the] (1¦x|2¦y|3¦z)[ coord[inate][s]] of %location/player%"],
        0,
        "number",
        coordinate,
    )?;
    reg.add_expression(
        "world of location",
        &[
            "[the] world of %location/player%",
            "%location/player%'[s] world",
        ],
        0,
        "world",
        world,
    )?;
    reg.add_expression(
        "distance",
        &["[the] distance between %location/player% and %location/player%"],
        0,
        "number",
        distance,
    )?;
    reg.add_expression(
        "block at",
        &["[the] block at %location/player%"],
        0,
        "item type",
        block_at,
    )?;
    Ok(())
}
