//! Sorting a list variable: `sort`. The pattern follows Skript's `EffSort` for its plain form
//! (`sort {x::*} [in descending order]`), which sorts by the natural order of the entries.
//!
//! The `by <expression>` and `based on <expression>` forms are not here: they need the input
//! expression (`input` in a mapping), which the port does not have yet. Entries that cannot be
//! compared with each other (text and numbers, say) leave the list as it is, as Skript does.
//! Only the direct entries of the list are sorted (`{x::1}`, `{x::2}`, ...); nested keys stay.

use super::next;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::Value;
use crate::horizon_skript_core::vars::Variables;
use std::cmp::Ordering;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "sort",
        &["sort %objects% [in (descending:descending|ascending) order]"],
        0,
        sort,
    )?;
    Ok(())
}

/// The variable store a name lives in: `{_x}` is local to the run.
fn store<'a>(cx: &'a mut Ctx<'_>, name: &str) -> &'a mut Variables {
    if name.starts_with('_') {
        &mut *cx.locals
    } else {
        &mut *cx.vars
    }
}

/// The sort key of a value. Numbers, timespans and dates compare by their amount; anything else
/// by its text in lower case.
enum Key {
    Number(f64),
    Text(String),
}

fn key_of(v: &Value) -> Key {
    match v {
        Value::Int(i) => Key::Number(*i as f64),
        Value::Num(n) => Key::Number(*n),
        Value::Timespan(ms) | Value::Date(ms) => Key::Number(*ms as f64),
        other => Key::Text(other.to_text().to_lowercase()),
    }
}

/// Values sort together only when they share this group: all numbers are one group.
fn group_of(v: &Value) -> &'static str {
    match v {
        Value::Int(_) | Value::Num(_) => "number",
        other => other.class(),
    }
}

fn key_cmp(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Number(x), Key::Number(y)) => x.total_cmp(y),
        (Key::Text(x), Key::Text(y)) => x.cmp(y),
        // Within one group every key has the same kind, so this is not reached.
        _ => Ordering::Equal,
    }
}

/// `sort {x::*}`: the entries of a list variable are renumbered in sorted order.
fn sort(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let e = m
        .expr(0)
        .ok_or_else(|| RunError::new("a part of the statement is missing"))?;
    let not_a_list = || RunError::new("you can only sort list variables");
    let name = e.var_name(cx)?.ok_or_else(not_a_list)?;
    let base = name.strip_suffix("::*").ok_or_else(not_a_list)?.to_owned();

    let prefix = format!("{}::", base.to_lowercase());
    let direct: Vec<(String, Value)> = store(cx, &base)
        .iter()
        .filter(|(k, _)| {
            k.to_lowercase().starts_with(&prefix)
                && !k.get(prefix.len()..).unwrap_or("").contains("::")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if let Some((_, first)) = direct.first() {
        let group = group_of(first);
        if direct.iter().any(|(_, v)| group_of(v) != group) {
            return next();
        }
    }

    let descending = m.has_tag("descending");
    let mut rows: Vec<(Key, Value)> = direct
        .iter()
        .map(|(_, v)| (key_of(v), v.clone()))
        .collect();
    rows.sort_by(|(a, _), (b, _)| {
        let order = key_cmp(a, b);
        if descending { order.reverse() } else { order }
    });

    let s = store(cx, &base);
    for (key, _) in &direct {
        s.delete(key);
    }
    for (i, (_, value)) in rows.into_iter().enumerate() {
        s.set(&format!("{base}::{}", i + 1), value);
    }
    next()
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::run;

    fn said(body: &str) -> Vec<String> {
        let host = Recorder::default();
        let logs = run(&host, body);
        assert!(logs.is_empty(), "{logs:?}");
        host.broadcasts.borrow().clone()
    }

    #[test]
    fn sorts_text_in_natural_order() {
        let out = said(
            "\tset {_w::1} to \"pear\"\n\tset {_w::2} to \"Apple\"\n\tset {_w::3} to \"banana\"\n\tsort {_w::*}\n\tbroadcast \"%{_w::1}% %{_w::2}% %{_w::3}%\"\n",
        );
        assert_eq!(out, vec!["Apple banana pear"]);
    }

    #[test]
    fn sorts_numbers_by_value_descending() {
        let out = said(
            "\tset {_n::1} to 2\n\tset {_n::2} to 10\n\tset {_n::3} to 1\n\tsort {_n::*} in descending order\n\tbroadcast \"%{_n::1}% %{_n::2}% %{_n::3}%\"\n\tsort {_n::*}\n\tbroadcast \"%{_n::1}% %{_n::2}% %{_n::3}%\"\n",
        );
        assert_eq!(out, vec!["10 2 1", "1 2 10"]);
    }

    #[test]
    fn mixed_text_and_numbers_stay_as_they_are() {
        let out = said(
            "\tset {_m::1} to \"a\"\n\tset {_m::2} to 1\n\tsort {_m::*}\n\tbroadcast \"%{_m::1}% %{_m::2}%\"\n",
        );
        assert_eq!(out, vec!["a 1"]);
    }
}
