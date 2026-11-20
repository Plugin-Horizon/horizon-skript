//! Sorting a list variable: `sort`. The pattern follows Skript's `EffSort` for its plain form
//! (`sort {x::*} [in descending order]`), which sorts by the natural order of the entries.
//!
//! `sort {x::*} by <expression>` and `transform {x::*} using <expression>` are read by the
//! parser (it needs the mapping as an expression): `input` is the entry's value and `input index`
//! its name after `::`. Entries that cannot be
//! compared with each other (text and numbers, say) leave the list as it is, as Skript does.
//! Only the direct entries of the list are sorted (`{x::1}`, `{x::2}`, ...); nested keys stay.

use super::next;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::expr::Expr;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Effect, Flow, Matched, Registry};
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
    reg.add_expression(
        "input",
        &["[the] input"],
        0,
        "object",
        |cx: &mut Ctx<'_>, _: &Matched| {
            cx.locals
                .get(INPUT)
                .cloned()
                .ok_or_else(|| RunError::new("`input` is only available in a sort or transform"))
        },
    )?;
    reg.add_expression(
        "input index",
        &["[the] input index"],
        -1,
        "object",
        |cx: &mut Ctx<'_>, _: &Matched| {
            cx.locals
                .get(INPUT_INDEX)
                .cloned()
                .ok_or_else(|| RunError::new("`input index` is only available in a sort or transform"))
        },
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

/// The local that holds `input` while a mapping expression runs, and the one for `input index`.
pub(crate) const INPUT: &str = "\u{1}input";
pub(crate) const INPUT_INDEX: &str = "\u{1}input index";

/// `sort {x::*}`: the entries of a list variable are renumbered in sorted order.
fn sort(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let e = m
        .expr(0)
        .ok_or_else(|| RunError::new("a part of the statement is missing"))?;
    sort_variable(cx, e, m.has_tag("descending"), None)
}

/// The direct entries of the list variable `base`, as (variable name, value).
fn entries(cx: &mut Ctx<'_>, base: &str) -> Vec<(String, Value)> {
    let prefix = format!("{}::", base.to_lowercase());
    store(cx, base)
        .iter()
        .filter(|(k, _)| {
            k.to_lowercase().starts_with(&prefix)
                && !k.get(prefix.len()..).unwrap_or("").contains("::")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The list variable's base name (`x` for `{x::*}`).
fn list_base(cx: &mut Ctx<'_>, e: &Expr, what: &str) -> Result<String, RunError> {
    let not_a_list = || RunError::new(format!("you can only {what} list variables"));
    let name = e.var_name(cx)?.ok_or_else(not_a_list)?;
    Ok(name.strip_suffix("::*").ok_or_else(not_a_list)?.to_owned())
}

/// What the mapping expression says about one entry: `input` is the entry's value and `input
/// index` the part of its name after `::`.
fn mapped(cx: &mut Ctx<'_>, mapping: &Expr, name: &str, value: &Value) -> Result<Value, RunError> {
    let index = name.rsplit("::").next().unwrap_or("").to_owned();
    cx.locals.set(INPUT, value.clone());
    cx.locals.set(INPUT_INDEX, Value::Text(index));
    let out = mapping.eval(cx);
    cx.locals.delete(INPUT);
    cx.locals.delete(INPUT_INDEX);
    out
}

/// Sorts the entries of a list variable, by their own value or by what `mapping` makes of each.
pub(crate) fn sort_variable(
    cx: &mut Ctx<'_>,
    e: &Expr,
    descending: bool,
    mapping: Option<&Expr>,
) -> Result<Flow, RunError> {
    let base = list_base(cx, e, "sort")?;
    let direct = entries(cx, &base);
    let mut keyed = Vec::new();
    for (name, value) in &direct {
        let by = match mapping {
            Some(map) => mapped(cx, map, name, value)?,
            None => value.clone(),
        };
        keyed.push((by, value.clone()));
    }

    if let Some((first, _)) = keyed.first() {
        let group = group_of(first);
        if keyed.iter().any(|(by, _)| group_of(by) != group) {
            return next();
        }
    }

    let mut rows: Vec<(Key, Value)> = keyed.iter().map(|(by, v)| (key_of(by), v.clone())).collect();
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

/// `transform {x::*} using <expression of input>`: every entry becomes what the expression says
/// of it, in place.
pub(crate) fn transform_variable(
    cx: &mut Ctx<'_>,
    e: &Expr,
    mapping: &Expr,
) -> Result<Flow, RunError> {
    let base = list_base(cx, e, "transform")?;
    let direct = entries(cx, &base);
    let mut out = Vec::new();
    for (name, value) in &direct {
        out.push((name.clone(), mapped(cx, mapping, name, value)?));
    }
    let s = store(cx, &base);
    for (name, value) in out {
        s.set(&name, value);
    }
    next()
}

/// `sort {x::*} [in descending order] by <expression of input>`.
pub(crate) struct SortBy {
    pub target: Expr,
    pub mapping: Expr,
    pub descending: bool,
}

impl Effect for SortBy {
    fn run(&self, cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
        sort_variable(cx, &self.target, self.descending, Some(&self.mapping))
    }
}

/// `transform {x::*} using <expression of input>`.
pub(crate) struct Transform {
    pub target: Expr,
    pub mapping: Expr,
}

impl Effect for Transform {
    fn run(&self, cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
        transform_variable(cx, &self.target, &self.mapping)
    }
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
    fn sorts_by_a_mapping_of_the_input() {
        let out = said(
            "\tset {_w::1} to \"ccc\"\n\tset {_w::2} to \"a\"\n\tset {_w::3} to \"bb\"\n\tsort {_w::*} by length of input\n\tbroadcast \"%{_w::1}% %{_w::2}% %{_w::3}%\"\n\tsort {_w::*} in descending order by length of input\n\tbroadcast \"%{_w::1}% %{_w::2}% %{_w::3}%\"\n",
        );
        assert_eq!(out, vec!["a bb ccc", "ccc bb a"]);
    }

    #[test]
    fn transform_replaces_every_entry_in_place() {
        let out = said(
            "\tset {_n::1} to 1\n\tset {_n::2} to 5\n\ttransform {_n::*} using input * 2\n\tbroadcast \"%{_n::1}% %{_n::2}%\"\n",
        );
        assert_eq!(out, vec!["2 10"]);
    }

    #[test]
    fn mixed_text_and_numbers_stay_as_they_are() {
        let out = said(
            "\tset {_m::1} to \"a\"\n\tset {_m::2} to 1\n\tsort {_m::*}\n\tbroadcast \"%{_m::1}% %{_m::2}%\"\n",
        );
        assert_eq!(out, vec!["a 1"]);
    }
}
