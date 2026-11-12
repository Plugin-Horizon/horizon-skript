//! Collections, randomness and time: the amount of a list, a random element, random numbers and
//! integers, and `now`. Ported from Skript's `ExprAmount`, `ExprRandom`, `ExprRandomNumber` and
//! `ExprNow`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{items, num, number_of, slot};
use crate::value::Value;

/// `amount of %objects%`: the items of a list (one for a single value, none for nothing).
fn amount(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::Int(items(slot(cx, m, 0)?).len() as i64))
}

/// `random element out of %objects%`: one item of a list, chosen with the host's random number.
fn random_element(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let list = items(slot(cx, m, 0)?);
    if list.is_empty() {
        return Ok(Value::Null);
    }
    let i = ((cx.host.random_unit() * list.len() as f64) as usize).min(list.len() - 1);
    Ok(list[i].clone())
}

/// `random integer from A to B` (mark 1) and `random number from A to B`, both ends included for
/// integers. Swapped ends are fine.
fn random_number(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let a = number_of(&slot(cx, m, 0)?)?;
    let b = number_of(&slot(cx, m, 1)?)?;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let u = cx.host.random_unit();
    if m.has_mark(1) {
        let (lo, hi) = (lo.floor() as i64, hi.floor() as i64);
        let span = (hi - lo + 1) as f64;
        let k = ((u * span) as i64).min(hi - lo);
        Ok(Value::Int(lo + k))
    } else {
        Ok(num(lo + u * (hi - lo)))
    }
}

fn now(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(Value::Date(cx.host.now_millis()))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "amount",
        &["[the] (amount|number|size) of %objects%"],
        0,
        "integer",
        amount,
    )?;
    reg.add_expression(
        "random element",
        &["[a|an] random (element|item) [out] of %objects%"],
        0,
        "object",
        random_element,
    )?;
    reg.add_expression(
        "random number",
        &["[a] random (1¦integer|number)[s] (from|between) %number% (to|and) %number%"],
        0,
        "number",
        random_number,
    )?;
    reg.add_expression("now", &["[the] now"], 0, "date", now)?;
    Ok(())
}
