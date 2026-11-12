//! Comparisons: `is` / `isn't`, `>`, `<`, `>=`, `<=`, `is between`, `contains`, `starts with`,
//! `ends with`, `is empty`, `is evenly divisible by`. Ported from Skript's `CondCompare`,
//! `CondContains`, `CondStartsEndsWith`, `CondIsEmpty` and `CondIsDivisibleBy`.
//!
//! A side that is a list holds when every item of the left side relates to some item of the
//! right side (`is`), or to every item of it (`>`). An empty side never holds.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{self, equal, items, negated, order, slot};
use crate::value::Value;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rel {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

fn relates(a: &Value, b: &Value, rel: Rel) -> Result<bool, RunError> {
    Ok(match rel {
        Rel::Eq => equal(a, b),
        Rel::Gt => order(a, b)? == Ordering::Greater,
        Rel::Ge => order(a, b)? != Ordering::Less,
        Rel::Lt => order(a, b)? == Ordering::Less,
        Rel::Le => order(a, b)? != Ordering::Greater,
    })
}

/// `left rel right` over lists, as described in the module docs.
pub fn holds(left: &[Value], right: &[Value], rel: Rel) -> Result<bool, RunError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }
    for l in left {
        match rel {
            Rel::Eq => {
                let mut any = false;
                for r in right {
                    if relates(l, r, rel)? {
                        any = true;
                        break;
                    }
                }
                if !any {
                    return Ok(false);
                }
            }
            _ => {
                for r in right {
                    if !relates(l, r, rel)? {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(true)
}

/// `left` is between `lo` and `hi` (both inclusive), as Skript's `is between`.
fn between(left: &[Value], lo: &[Value], hi: &[Value]) -> Result<bool, RunError> {
    Ok(holds(left, lo, Rel::Ge)? && holds(left, hi, Rel::Le)?)
}

/// A comparison of the slots 0 and 1 (2 for `between`), negated when the pattern says so.
fn compare(cx: &mut Ctx<'_>, m: &Matched, rel: Rel) -> Result<bool, RunError> {
    let left = items(slot(cx, m, 0)?);
    let right = items(slot(cx, m, 1)?);
    let r = holds(&left, &right, rel)?;
    Ok(r != negated(m))
}

fn is_between(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = items(slot(cx, m, 0)?);
    let lo = items(slot(cx, m, 1)?);
    let hi = items(slot(cx, m, 2)?);
    Ok(between(&left, &lo, &hi)? != negated(m))
}

/// The text of a value for `contains` and `starts with`: its text form.
fn texts(v: Vec<Value>) -> Vec<String> {
    v.iter().map(support::text_of).collect()
}

/// `text contains part`: substring test; for a list, every part must be one of its items.
fn contains(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = slot(cx, m, 0)?;
    let right = items(slot(cx, m, 1)?);
    let found = match &left {
        Value::Text(s) => {
            let s = s.to_lowercase();
            texts(right).iter().all(|p| s.contains(&p.to_lowercase()))
        }
        _ => {
            let items = items(left);
            !right.is_empty() && right.iter().all(|r| items.iter().any(|i| equal(i, r)))
        }
    };
    Ok(found != negated(m))
}

fn starts_or_ends(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = texts(items(slot(cx, m, 0)?));
    let right = texts(items(slot(cx, m, 1)?));
    let ends = m.has_mark(2);
    let mut ok = !left.is_empty() && !right.is_empty();
    for l in &left {
        let l = l.to_lowercase();
        if !right.iter().any(|r| {
            let r = r.to_lowercase();
            if ends {
                l.ends_with(&r)
            } else {
                l.starts_with(&r)
            }
        }) {
            ok = false;
        }
    }
    Ok(ok != negated(m))
}

fn is_empty(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let v = slot(cx, m, 0)?;
    let empty = match &v {
        Value::Null => true,
        Value::List(items) => items.is_empty(),
        Value::Text(s) => s.is_empty(),
        _ => false,
    };
    Ok(empty != negated(m))
}

fn divisible(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = items(slot(cx, m, 0)?);
    let d = support::number_of(&slot(cx, m, 1)?)?;
    if d == 0.0 {
        return Err(RunError::new("can't divide by zero"));
    }
    let mut ok = !left.is_empty();
    for l in &left {
        if support::number_of(l)? % d != 0.0 {
            ok = false;
        }
    }
    Ok(ok != negated(m))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    // `is` / `isn't` / `=` / `!=`, and the words for them.
    reg.add_condition(
        "is",
        &[
            "%objects% (is|are)(1¦ not|1¦n't|) [(equal to|the same as)] %objects%",
            "%objects% (1¦!=|=) %objects%",
        ],
        0,
        |cx: &mut Ctx<'_>, m: &Matched| compare(cx, m, Rel::Eq),
    )?;
    reg.add_condition(
        "greater",
        &[
            "%objects% (is|are)(1¦ not|1¦n't|) ((greater|more|higher|bigger|larger) than|above) %objects%",
            "%objects% > %objects%",
        ],
        0,
        |cx: &mut Ctx<'_>, m: &Matched| compare(cx, m, Rel::Gt),
    )?;
    reg.add_condition(
        "greater or equal",
        &[
            "%objects% (is|are)(1¦ not|1¦n't|) (greater|more|higher|bigger|larger|above) [than] or (equal to|the same as) %objects%",
            "%objects% >= %objects%",
        ],
        0,
        |cx: &mut Ctx<'_>, m: &Matched| compare(cx, m, Rel::Ge),
    )?;
    reg.add_condition(
        "less",
        &[
            "%objects% (is|are)(1¦ not|1¦n't|) ((less|smaller|lower) than|below) %objects%",
            "%objects% \\< %objects%",
        ],
        0,
        |cx: &mut Ctx<'_>, m: &Matched| compare(cx, m, Rel::Lt),
    )?;
    reg.add_condition(
        "less or equal",
        &[
            "%objects% (is|are)(1¦ not|1¦n't|) (less|smaller|lower|below) [than] or (equal to|the same as) %objects%",
            "%objects% \\<= %objects%",
        ],
        0,
        |cx: &mut Ctx<'_>, m: &Matched| compare(cx, m, Rel::Le),
    )?;
    reg.add_condition(
        "between",
        &["%objects% (is|are)(1¦ not|1¦n't|) between %objects% and %objects%"],
        0,
        is_between,
    )?;
    // `contains`: `text contains part`, `list does not contain item`.
    reg.add_condition(
        "contains",
        &["%objects% (1¦(does not|doesn't|do not|don't) |)contain[s] %objects%"],
        0,
        contains,
    )?;
    reg.add_condition(
        "starts or ends with",
        &["%texts% (1¦(does not|doesn't|do not|don't) |)(start|2¦end)[s] with %texts%"],
        0,
        starts_or_ends,
    )?;
    reg.add_condition(
        "is empty",
        &["%objects% (is|are)(1¦ not|1¦n't|) empty"],
        0,
        is_empty,
    )?;
    reg.add_condition(
        "evenly divisible",
        &["%numbers% (is|are)(1¦ not|1¦n't|) evenly divisible by %number%"],
        0,
        divisible,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn equality_across_classes() {
        assert!(equal(&Value::Int(5), &Value::Num(5.0)));
        assert!(!equal(&Value::Int(5), &t("5")));
        assert!(equal(&t("Alice"), &t("alice")));
        assert!(!equal(&Value::Null, &t("")));
        assert!(equal(
            &Value::Player {
                uuid: 1,
                name: "A".into()
            },
            &Value::Player {
                uuid: 1,
                name: "B".into()
            }
        ));
    }

    #[test]
    fn ordering_needs_comparable_classes() {
        assert_eq!(
            order(&Value::Int(2), &Value::Num(2.5)).unwrap(),
            Ordering::Less
        );
        assert!(order(&t("a"), &t("b")).is_err());
        assert_eq!(
            order(&Value::Timespan(2000), &Value::Timespan(1000)).unwrap(),
            Ordering::Greater
        );
    }

    #[test]
    fn lists_relate_item_by_item() {
        let one_two = [Value::Int(1), Value::Int(2)];
        // Every left item must equal one right item: 1 is not 2.
        assert!(!holds(&one_two, &[Value::Int(2)], Rel::Eq).unwrap());
        assert!(holds(&[Value::Int(2)], &one_two, Rel::Eq).unwrap());
        assert!(!holds(&one_two, &[Value::Int(3)], Rel::Eq).unwrap());
        assert!(holds(&[Value::Int(5)], &[Value::Int(1), Value::Int(2)], Rel::Gt).unwrap());
        assert!(!holds(&[Value::Int(2)], &[Value::Int(1), Value::Int(3)], Rel::Gt).unwrap());
        assert!(!holds(&[], &[Value::Int(1)], Rel::Eq).unwrap());
        assert!(between(&[Value::Int(5)], &[Value::Int(5)], &[Value::Int(9)]).unwrap());
        assert!(!between(&[Value::Int(10)], &[Value::Int(5)], &[Value::Int(9)]).unwrap());
    }
}
