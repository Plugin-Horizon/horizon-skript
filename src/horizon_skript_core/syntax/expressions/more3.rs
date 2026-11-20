//! Percentages of a list, the inverse of booleans, the midpoint of two locations, a shuffled list
//! and a list without some of its items. Ported from Skript's `ExprPercent`, `ExprInverse`,
//! `ExprMidpoint`, `ExprShuffledList` and `ExprExcept`.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{equal, items, location_of, num, number_of, slot};
use crate::horizon_skript_core::value::{Location, Value};

/// `5% of %numbers%` (the number is written in the pattern) and `%number% percent of %numbers%`:
/// each number times the percent, divided by 100. Always a decimal form, as Skript's doubles.
fn percent_of(percent: f64, numbers: Value) -> Result<Value, RunError> {
    let mut out = Vec::new();
    for n in items(numbers) {
        out.push(num(number_of(&n)? * percent / 100.0));
    }
    Ok(Value::List(out))
}

/// `<[0-9.]+>\% of %numbers%`: the percent is a literal in the script.
fn percent_literal(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let text = m
        .regex(0)
        .ok_or_else(|| RunError::new("missing the percent"))?;
    let percent = text
        .parse::<f64>()
        .map_err(|_| RunError::new(format!("can't read the percent {text}")))?;
    let numbers = slot(cx, m, 0)?;
    percent_of(percent, numbers)
}

/// `%number% percent of %numbers%`: the percent is an expression.
fn percent_slot(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let percent = number_of(&slot(cx, m, 0)?)?;
    let numbers = slot(cx, m, 1)?;
    percent_of(percent, numbers)
}

/// `inverse of %booleans%`: each boolean negated.
fn inverse(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut out = Vec::new();
    for b in items(slot(cx, m, 0)?) {
        let v = b
            .as_bool()
            .ok_or_else(|| RunError::new(format!("expected a boolean, got {}", b.class())))?;
        out.push(Value::Bool(!v));
    }
    Ok(Value::List(out))
}

/// `midpoint of %location% and %location%`: the middle of the two points, in the first world.
/// Two locations in different worlds are an error, as in Skript. The yaw and pitch are zero.
fn midpoint(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let first = slot(cx, m, 0)?;
    let second = slot(cx, m, 1)?;
    let a = location_of(cx, &first)?;
    let b = location_of(cx, &second)?;
    if !a.world.eq_ignore_ascii_case(&b.world) {
        return Err(RunError::new(
            "cannot get the midpoint of two locations in different worlds",
        ));
    }
    Ok(Value::Location(Location {
        world: a.world,
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
        z: (a.z + b.z) / 2.0,
        yaw: 0.0,
        pitch: 0.0,
    }))
}

/// `shuffled %objects%`: the items in a random order (Fisher-Yates with the host's random number).
fn shuffled(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut v = items(slot(cx, m, 0)?);
    for i in (1..v.len()).rev() {
        let j = ((cx.host.random_unit() * (i + 1) as f64) as usize).min(i);
        v.swap(i, j);
    }
    Ok(Value::List(v))
}

/// `%objects% except %objects%`: the first list without the items of the second one.
fn except(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let base = items(slot(cx, m, 0)?);
    let excluded = items(slot(cx, m, 1)?);
    let kept = base
        .into_iter()
        .filter(|item| !excluded.iter().any(|x| equal(item, x)))
        .collect();
    Ok(Value::List(kept))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "percent of (literal)",
        &["<[0-9.]+>\\% of %numbers%"],
        0,
        "object",
        percent_literal,
    )?;
    reg.add_expression(
        "percent of",
        &["%number% (percent|\\%) of %numbers%"],
        0,
        "object",
        percent_slot,
    )?;
    reg.add_expression(
        "inverse",
        &["[the] (inverse|opposite)[s] of %booleans%"],
        0,
        "object",
        inverse,
    )?;
    reg.add_expression(
        "midpoint",
        &["[the] mid[-]point (of|between) %location% and %location%"],
        0,
        "location",
        midpoint,
    )?;
    reg.add_expression("shuffled", &["shuffled %objects%"], 0, "object", shuffled)?;
    reg.add_expression(
        "except",
        &["%objects% (except|excluding|not including) %objects%"],
        0,
        "object",
        except,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{expr, run};

    /// The items of a list expression, as the text of the list (`a, b`).
    fn list(h: &Recorder, e: &str) -> String {
        let logs = run(
            h,
            &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r::*}}%\"\n"),
        );
        assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
        let out = h.broadcasts.borrow().clone();
        assert_eq!(out.len(), 1, "no broadcast for `{e}`");
        out[0].clone()
    }

    fn fresh() -> Recorder {
        Recorder::default()
    }

    #[test]
    fn percent_of_numbers() {
        assert_eq!(list(&fresh(), "50% of 200"), "100");
        assert_eq!(list(&fresh(), "25 percent of 8 and 4"), "2, 1");
    }

    #[test]
    fn inverse_of_booleans() {
        assert_eq!(list(&fresh(), "inverse of true"), "false");
        assert_eq!(list(&fresh(), "opposite of false"), "true");
    }

    #[test]
    fn midpoint_of_two_locations() {
        assert_eq!(
            expr(
                &fresh(),
                "x of midpoint of location at 0, 0, 0 in world \"w\" and location at 2, 4, 6 in world \"w\""
            ),
            "1"
        );
        assert_eq!(
            expr(
                &fresh(),
                "z of midpoint of location at 0, 0, 0 in world \"w\" and location at 2, 4, 6 in world \"w\""
            ),
            "3"
        );
    }

    #[test]
    fn shuffled_keeps_every_item() {
        let shuffled = list(&fresh(), "sorted shuffled 1, 2, 3, 4 and 5");
        assert_eq!(shuffled, "1, 2, 3, 4, 5");
    }

    #[test]
    fn except_drops_the_excluded_items() {
        assert_eq!(list(&fresh(), "1, 2, 3 and 4 except 2 and 4"), "1, 3");
        assert_eq!(list(&fresh(), "\"a\", \"B\" and \"c\" excluding \"b\""), "a, c");
    }
}
