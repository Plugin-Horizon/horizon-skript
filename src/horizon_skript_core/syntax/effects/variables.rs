//! Changing variables: `increase`, `decrease`, `reset`, `toggle`, `replace` and `copy`. Patterns
//! follow Skript's `EffChange` (increase, decrease, reset), `EffToggle`, `EffReplace` and
//! `EffCopy`. They work on variables, and on a changeable expression (`increase the
//! x-coordinate of {l} by 1`) where the expression takes a change.
//!
//! `replace` without `with case sensitivity` ignores case, as Skript does; the regex forms use
//! Rust's regex syntax, which differs from Java's in rare corners (look-around, possessive
//! quantifiers).

use super::{next, slot};
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::expr::Expr;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Change, Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{num, number_of};
use crate::horizon_skript_core::value::Value;
use crate::horizon_skript_core::vars::Variables;
use regex::{NoExpand, RegexBuilder};

const CASE: &str = "[ (case:with case sensitivity)]";

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "increase",
        &["(increase|raise) %object% by %number%"],
        0,
        increase,
    )?;
    reg.add_effect(
        "decrease",
        &["(decrease|reduce|lower) %object% by %number%"],
        0,
        decrease,
    )?;
    reg.add_effect("reset", &["reset %object%"], 0, reset)?;
    reg.add_effect("toggle", &["(toggle|switch) %object%"], 0, toggle)?;
    let find_in = format!("replace (all |every |first:the first |first:first |)%texts% in %text% with %text%{CASE}");
    let with_in = format!("replace (all |every |first:the first |first:first |)%texts% with %text% in %text%{CASE}");
    reg.add_effect("replace in", &[find_in.as_str()], 0, replace_in)?;
    reg.add_effect("replace with", &[with_in.as_str()], 0, replace_with)?;
    reg.add_effect(
        "regex replace in",
        &["(replace [with|using] regex|regex replace) %texts% in %text% with %text%"],
        0,
        regex_replace_in,
    )?;
    reg.add_effect(
        "regex replace with",
        &["(replace [with|using] regex|regex replace) %texts% with %text% in %text%"],
        0,
        regex_replace_with,
    )?;
    reg.add_effect("copy", &["copy %objects% [in]to %objects%"], 0, copy)?;
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

/// The expression in slot `n`, or an error.
fn expr_at<'m>(m: &'m Matched, n: usize) -> Result<&'m Expr, RunError> {
    m.expr(n)
        .ok_or_else(|| RunError::new("a part of the statement is missing"))
}

/// The variable name an expression names, or an error that says what the statement needs.
fn var_in(cx: &mut Ctx<'_>, e: &Expr, what: &str) -> Result<String, RunError> {
    e.var_name(cx)?
        .ok_or_else(|| RunError::new(format!("{what} must be a variable")))
}

/// Adds `by` (negative to subtract) to a variable, or hands a changeable expression the change.
fn bump(cx: &mut Ctx<'_>, m: &Matched, sign: f64) -> Result<Flow, RunError> {
    let e = expr_at(m, 0)?;
    let by = number_of(&slot(cx, m, 1)?)?;
    if e.var_name(cx)?.is_none() {
        let how = if sign < 0.0 { Change::Remove } else { Change::Add };
        let amount = num(by);
        if let Some(r) = e.change(cx, how, &amount) {
            r?;
            return next();
        }
    }
    let name = var_in(cx, e, "the value to change")?;
    if name.ends_with("::*") {
        return Err(RunError::new("a list can't be increased or decreased"));
    }
    let s = store(cx, &name);
    let current = match s.get(&name) {
        None | Some(Value::Null) => 0.0,
        Some(v) => number_of(v)?,
    };
    s.set(&name, num(current + sign * by));
    next()
}

fn increase(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    bump(cx, m, 1.0)
}

fn decrease(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    bump(cx, m, -1.0)
}

/// `reset {x}`: a variable goes back to unset, a list variable is emptied.
fn reset(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let name = var_in(cx, expr_at(m, 0)?, "the value to reset")?;
    match name.strip_suffix("::*") {
        Some(base) => {
            store(cx, base).delete_children(base);
        }
        None => {
            store(cx, &name).delete(&name);
        }
    }
    next()
}

/// `toggle {x}`: a boolean variable flips; an unset one becomes true.
fn toggle(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let name = var_in(cx, expr_at(m, 0)?, "the value to toggle")?;
    let s = store(cx, &name);
    let now = match s.get(&name) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(RunError::new(format!(
                "can't toggle {}, only a boolean",
                other.class()
            )));
        }
    };
    s.set(&name, Value::Bool(!now));
    next()
}

/// What a replace does, from its pattern.
struct Replace {
    first: bool,
    case: bool,
    regex: bool,
}

/// The replaced text of `haystack`: every needle in turn.
fn replaced(
    haystack: &str,
    needles: &[String],
    with: &str,
    how: &Replace,
) -> Result<String, RunError> {
    let mut text = haystack.to_owned();
    for needle in needles {
        let pattern = if how.regex {
            needle.clone()
        } else {
            regex::escape(needle)
        };
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(!how.regex && !how.case)
            .build()
            .map_err(|e| RunError::new(format!("bad regex {needle}: {e}")))?;
        let limit = if how.first { 1 } else { 0 };
        text = if how.regex {
            re.replacen(&text, limit, with).into_owned()
        } else {
            re.replacen(&text, limit, NoExpand(with)).into_owned()
        };
    }
    Ok(text)
}

/// Replaces in the text of the expression in `target` and stores the result back: a variable or
/// a changeable expression.
fn replace_into(
    cx: &mut Ctx<'_>,
    m: &Matched,
    (needles, target, with): (usize, usize, usize),
    how: Replace,
) -> Result<Flow, RunError> {
    let needles: Vec<String> = super::texts_in(cx, m, needles)?;
    let with = slot(cx, m, with)?.to_text();
    let e = expr_at(m, target)?;
    let hay = e.eval(cx)?.to_text();
    let out = Value::Text(replaced(&hay, &needles, &with, &how)?);
    match e.var_name(cx)? {
        Some(name) => store(cx, &name).set(&name, out),
        None => match e.change(cx, Change::Set, &out) {
            Some(r) => r?,
            None => return Err(RunError::new("that text can't be changed")),
        },
    }
    next()
}

fn plain(m: &Matched) -> Replace {
    Replace {
        first: m.has_tag("first"),
        case: m.has_tag("case"),
        regex: false,
    }
}

fn regex_form() -> Replace {
    Replace {
        first: false,
        case: true,
        regex: true,
    }
}

fn replace_in(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    replace_into(cx, m, (0, 1, 2), plain(m))
}

fn replace_with(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    replace_into(cx, m, (0, 2, 1), plain(m))
}

fn regex_replace_in(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    replace_into(cx, m, (0, 1, 2), regex_form())
}

fn regex_replace_with(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    replace_into(cx, m, (0, 2, 1), regex_form())
}

/// `copy {a::*} to {b::*}` copies every entry with its key; any other copy stores the value (a
/// list becomes numbered entries when the target is a list).
fn copy(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let from = expr_at(m, 0)?;
    let to = expr_at(m, 1)?;
    let to_name = var_in(cx, to, "the copy target")?;
    if let (Some(src), Some(dst)) = (from.var_name(cx)?, to_name.strip_suffix("::*")) {
        if let Some(base) = src.strip_suffix("::*") {
            let prefix = format!("{}::", base.to_lowercase());
            let entries: Vec<(String, Value)> = store(cx, base)
                .iter()
                .filter(|(k, _)| k.to_lowercase().starts_with(&prefix))
                .map(|(k, v)| (k.get(prefix.len()..).unwrap_or("").to_owned(), v.clone()))
                .collect();
            let s = store(cx, dst);
            s.delete_children(dst);
            for (key, value) in entries {
                s.set(&format!("{dst}::{key}"), value);
            }
            return next();
        }
    }
    let value = from.eval(cx)?;
    match to_name.strip_suffix("::*") {
        Some(base) => {
            let s = store(cx, base);
            s.delete_children(base);
            for (i, item) in super::items_of(value).into_iter().enumerate() {
                s.set(&format!("{base}::{}", i + 1), item);
            }
        }
        None => match value {
            Value::List(items) => {
                let s = store(cx, &to_name);
                s.delete(&to_name);
                for (i, item) in items.into_iter().enumerate() {
                    s.set(&format!("{to_name}::{}", i + 1), item);
                }
            }
            other => store(cx, &to_name).set(&to_name, other),
        },
    }
    next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::run;

    fn said(body: &str) -> Vec<String> {
        let host = Recorder::default();
        let logs = run(&host, body);
        assert!(logs.is_empty(), "{logs:?}");
        host.broadcasts.borrow().clone()
    }

    #[test]
    fn increase_and_decrease_a_number() {
        let out = said(
            "\tset {_n} to 5\n\tincrease {_n} by 3\n\tdecrease {_n} by 1\n\tbroadcast \"%{_n}%\"\n\tincrease {_new} by 2\n\tbroadcast \"%{_new}%\"\n",
        );
        assert_eq!(out, vec!["7", "2"]);
    }

    #[test]
    fn reset_and_toggle() {
        let out = said(
            "\tset {_x} to 1\n\treset {_x}\n\tif {_x} is not set:\n\t\tbroadcast \"gone\"\n\tset {_b} to true\n\ttoggle {_b}\n\tbroadcast \"%{_b}%\"\n\ttoggle {_b}\n\tbroadcast \"%{_b}%\"\n",
        );
        assert_eq!(out, vec!["gone", "false", "true"]);
    }

    #[test]
    fn replace_ignores_case_unless_asked() {
        let out = said(
            "\tset {_m} to \"Hello World\"\n\treplace \"world\" in {_m} with \"there\"\n\tbroadcast {_m}\n\treplace \"THERE\" in {_m} with \"x\" with case sensitivity\n\tbroadcast {_m}\n\treplace first \"l\" in {_m} with \"L\"\n\tbroadcast {_m}\n\treplace \"e\" with \"3\" in {_m}\n\tbroadcast {_m}\n",
        );
        assert_eq!(
            out,
            vec!["Hello there", "Hello there", "HeLlo there", "H3Llo th3r3"]
        );
    }

    #[test]
    fn regex_replace() {
        let out = said(
            "\tset {_m} to \"abc123\"\n\treplace regex \"[0-9]+\" in {_m} with \"#\"\n\tbroadcast {_m}\n",
        );
        assert_eq!(out, vec!["abc#"]);
    }

    #[test]
    fn copy_keeps_the_keys_of_a_list() {
        let out = said(
            "\tset {_a::x} to 1\n\tset {_a::y} to 2\n\tcopy {_a::*} to {_b::*}\n\tbroadcast \"%{_b::x}% %{_b::y}%\"\n\tcopy \"hi\" to {_c}\n\tbroadcast {_c}\n",
        );
        assert_eq!(out, vec!["1 2", "hi"]);
    }

    #[test]
    fn replaced_text_is_literal_unless_regex() {
        let all = Replace {
            first: false,
            case: false,
            regex: false,
        };
        assert_eq!(
            replaced("a.b.c", &[".".to_owned()], "$1", &all).unwrap(),
            "a$1b$1c"
        );
        assert!(replaced("x", &["(".to_owned()], "", &regex_form()).is_err());
    }
}
