//! Locations: the coordinates, the world, the distance between two places and the block at a
//! location. Ported from Skript's `ExprXYZComponent`, `ExprWorld`, `ExprDistance` and `ExprBlock`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Change, ExprSyntax, Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{location_at, num, slot, world_at};
use crate::value::Value;

/// `x of <location or player>`, `y of ...`, `z of ...` (marks 1, 2, 3). The pattern takes `x of`,
/// `x-coordinate of` and `x coordinates of`, as Skript does.
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

/// The coordinate expression, changeable when it names a variable that holds a location
/// (`subtract 0.18 from y-coordinate of {_mouth}`, as Skript's `ExprXYZComponent.change`).
struct Coordinate;

impl ExprSyntax for Coordinate {
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
        coordinate(cx, m)
    }

    fn change(
        &self,
        cx: &mut Ctx<'_>,
        m: &Matched,
        how: Change,
        by: &Value,
    ) -> Option<Result<(), RunError>> {
        let name = match m.expr(0)?.var_name(cx) {
            Ok(Some(name)) => name,
            Ok(None) => return None,
            Err(e) => return Some(Err(e)),
        };
        Some(change_coordinate(cx, m, &name, how, by))
    }
}

fn change_coordinate(
    cx: &mut Ctx<'_>,
    m: &Matched,
    name: &str,
    how: Change,
    by: &Value,
) -> Result<(), RunError> {
    let by = by
        .as_f64()
        .ok_or_else(|| RunError::new("a coordinate changes by a number"))?;
    let vars = if name.starts_with('_') {
        &mut *cx.locals
    } else {
        &mut *cx.vars
    };
    let Some(Value::Location(mut l)) = vars.get(name).cloned() else {
        return Err(RunError::new(format!("{{{name}}} holds no location")));
    };
    let axis = if m.has_mark(1) {
        &mut l.x
    } else if m.has_mark(2) {
        &mut l.y
    } else {
        &mut l.z
    };
    match how {
        Change::Set => *axis = by,
        Change::Add => *axis += by,
        Change::Remove => *axis -= by,
    }
    vars.set(name, Value::Location(l));
    Ok(())
}

fn world(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::World(world_at(cx, m, 0)?))
}

/// `world "name"`: the world with that name (the name is not checked against the engine).
fn world_named(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    match slot(cx, m, 0)? {
        Value::Text(name) if !name.trim().is_empty() => Ok(Value::World(name.trim().to_owned())),
        other => Err(RunError::new(format!(
            "a world needs a name, got {}",
            other.class()
        ))),
    }
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
        "coordinate",
        &["[the] (1¦x|2¦y|3¦z)[( |-)coord[inate][s]] of %location/player%"],
        0,
        "number",
        Coordinate,
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
    // After `world of ...`: `world "stage"` is the world with that name, as in `location(x, y, z, world "stage")`.
    reg.add_expression("world name", &["world %text%"], 1, "world", world_named)?;
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

#[cfg(test)]
mod tests {
    use crate::host::Recorder;
    use crate::syntax::support::testing::{expr, run, value_text};
    use crate::value::Value;

    #[test]
    fn coordinates_take_a_space_or_a_hyphen() {
        // `expr` reads the host's broadcasts, so each expression gets a host of its own.
        for (e, want) in [
            ("x-coordinate of location(1, 2, 3, world \"stage\")", "1"),
            ("y coordinates of location(1, 2, 3, world \"stage\")", "2"),
            ("z-coordinate of location(1, 2, 3, world \"stage\")", "3"),
        ] {
            assert_eq!(expr(&Recorder::default(), e), want);
        }
    }

    #[test]
    fn world_named_is_a_world_value() {
        let h = Recorder::default();
        assert_eq!(expr(&h, "world \"stage\""), "stage");
        assert_eq!(value_text(&Value::World("stage".into())), "stage");
    }

    /// A seasonal lobby script moves a location variable by its coordinates.
    #[test]
    fn coordinates_of_a_location_variable_change() {
        let h = Recorder::default();
        let logs = run(
            &h,
            "\tset {_m} to location(1, 2, 3, world \"stage\")\n\tsubtract 0.18 from y-coordinate of {_m}\n\tadd 0.5 to x-coordinate of {_m}\n\tadd -1 to z-coordinate of {_m}\n\tset y-coordinate of {_m} to y-coordinate of {_m} + 10\n\tbroadcast \"%{_m}%\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["stage at 1.5, 11.82, 2".to_owned()]
        );
    }
}
