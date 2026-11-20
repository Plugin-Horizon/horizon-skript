//! Rounding, degrees, the first or last characters of a text, and the name of a world. Ported
//! from Skript's `ExprRound`, `ExprAngle`, `ExprSubstring` (the first and last form) and the
//! world form of `ExprName`.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{number_of, slot, text_of, world_at};
use crate::horizon_skript_core::value::Value;

/// Applies `f` to one number, or to each number of a list. Whole numbers pass through `f` too.
fn each_number(v: Value, f: impl Fn(f64) -> Value) -> Result<Value, RunError> {
    match v {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in &items {
                out.push(f(number_of(item)?));
            }
            Ok(Value::List(out))
        }
        other => Ok(f(number_of(&other)?)),
    }
}

/// Half up, as Java's `Math.round`: `2.5` gives `3`, `-2.5` gives `-2`.
fn round_half_up(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `[the] round[ed] %numbers%`, `%numbers% rounded`: to the nearest whole number.
fn round_nearest(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    each_number(v, |n| Value::Int(round_half_up(n) as i64))
}

/// `[the] round[ed] down %numbers%`, `floored`: down to the whole number below.
fn round_down(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    each_number(v, |n| Value::Int(n.floor() as i64))
}

/// `[the] round[ed] up %numbers%`, `ceiled`: up to the whole number above.
fn round_up(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    each_number(v, |n| Value::Int(n.ceil() as i64))
}

/// `%numbers% in degrees` (mark 1) and `%numbers% in radians` (mark 2). A radian value is turned
/// into degrees; a degree value is kept as it is, as Skript does.
fn to_degrees(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    let radians = m.has_mark(2);
    each_number(v, |n| {
        let deg = if radians { n.to_degrees() } else { n };
        Value::Num(deg)
    })
}

/// `[the] (first|last) [N] character[s] of %text%` (mark 1 is first, mark 2 is last), and
/// `[the] N (first|last) characters of %text%`. N is 1 when it is left out.
fn edge_characters(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let count = match slot(cx, m, 0)? {
        Value::Null => 1,
        v => number_of(&v)? as i64,
    };
    let chars: Vec<char> = text_of(&slot(cx, m, 1)?).chars().collect();
    let n = count.clamp(0, chars.len() as i64) as usize;
    let part: String = if m.has_mark(2) {
        chars[chars.len() - n..].iter().collect()
    } else {
        chars[..n].iter().collect()
    };
    Ok(Value::Text(part))
}

/// `[the] name of %world%`: the name of the world.
fn world_name(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::Text(world_at(cx, m, 0)?))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "round",
        &["[a|the] round[ed] %numbers%", "%numbers% round[ed]"],
        0,
        "integer",
        round_nearest,
    )?;
    reg.add_expression(
        "round down",
        &[
            "[a|the] (round[ed] down|floored) %numbers%",
            "%numbers% (round[ed] down|floored)",
        ],
        0,
        "integer",
        round_down,
    )?;
    reg.add_expression(
        "round up",
        &[
            "[a|the] (round[ed] up|ceil[ing]ed) %numbers%",
            "%numbers% (round[ed] up|ceil[ing]ed)",
        ],
        0,
        "integer",
        round_up,
    )?;
    reg.add_expression(
        "degrees",
        &["%numbers% [in] (1¦deg[ree][s]|2¦rad[ian][s])"],
        0,
        "number",
        to_degrees,
    )?;
    reg.add_expression(
        "first or last characters",
        &[
            "[the] (1¦first|2¦last) [%-number%] character[s] of %text%",
            "[the] %number% (1¦first|2¦last) characters of %text%",
        ],
        0,
        "text",
        edge_characters,
    )?;
    reg.add_expression(
        "world name of",
        &["[the] name[s] of %world%", "%world%'[s] name[s]"],
        0,
        "text",
        world_name,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::expr;

    fn fresh() -> Recorder {
        Recorder::default()
    }

    #[test]
    fn rounding() {
        assert_eq!(expr(&fresh(), "round 2.5"), "3");
        assert_eq!(expr(&fresh(), "-2.5 rounded"), "-2");
        assert_eq!(expr(&fresh(), "2.7 floored"), "2");
        assert_eq!(expr(&fresh(), "round down 2.7"), "2");
        assert_eq!(expr(&fresh(), "2.1 ceiled"), "3");
        assert_eq!(expr(&fresh(), "round up 2.1"), "3");
    }

    #[test]
    fn degrees_from_radians() {
        assert_eq!(expr(&fresh(), "180 in degrees"), "180");
        assert_eq!(expr(&fresh(), "0 radians"), "0");
    }

    #[test]
    fn first_and_last_characters() {
        assert_eq!(expr(&fresh(), "first 3 characters of \"hello\""), "hel");
        assert_eq!(expr(&fresh(), "last 2 characters of \"hello\""), "lo");
        assert_eq!(expr(&fresh(), "first character of \"abc\""), "a");
        assert_eq!(expr(&fresh(), "last character of \"abc\""), "c");
    }

    #[test]
    fn name_of_a_world() {
        assert_eq!(expr(&fresh(), "name of world \"stage\""), "stage");
    }
}
