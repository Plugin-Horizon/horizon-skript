//! Text: length, upper and lower case, substrings, joining, splitting and replacing. Ported from
//! Skript's `ExprLength`, `ExprSubstring`, `ExprJoinSplit`, and the case and replace
//! expressions. Indices are 1-based and inclusive, as in Skript.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{items, number_of, slot, text_of};
use crate::value::Value;

fn length(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let s = text_of(&slot(cx, m, 0)?);
    Ok(Value::Int(s.chars().count() as i64))
}

/// `uppercase %text%` (mark 1) and `lowercase %text%`.
fn case(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let s = text_of(&slot(cx, m, 0)?);
    Ok(Value::Text(if m.has_mark(1) {
        s.to_uppercase()
    } else {
        s.to_lowercase()
    }))
}

/// `substring of %text% from A to B`: the characters A to B, both included.
fn substring(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let chars: Vec<char> = text_of(&slot(cx, m, 0)?).chars().collect();
    let from = number_of(&slot(cx, m, 1)?)? as i64;
    let to = number_of(&slot(cx, m, 2)?)? as i64;
    let i1 = (from - 1).max(0) as usize;
    let i2 = (to.max(0) as usize).min(chars.len());
    if i1 >= i2 {
        return Ok(Value::Text(String::new()));
    }
    Ok(Value::Text(chars[i1..i2].iter().collect()))
}

/// `character at index N of %text%`: the one character at N (1-based), empty when there is none.
fn char_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let index = number_of(&slot(cx, m, 0)?)? as i64;
    let s = text_of(&slot(cx, m, 1)?);
    let c = usize::try_from(index - 1)
        .ok()
        .and_then(|i| s.chars().nth(i))
        .map(String::from)
        .unwrap_or_default();
    Ok(Value::Text(c))
}

/// `join %texts% [with %text%]`, `concat ...`: the texts one after another, with the delimiter
/// between them (none by default, as in Skript).
fn join(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let parts: Vec<String> = items(slot(cx, m, 0)?).iter().map(text_of).collect();
    let delimiter = match m.expr(1) {
        Some(_) => text_of(&slot(cx, m, 1)?),
        None => String::new(),
    };
    Ok(Value::Text(parts.join(&delimiter)))
}

/// `split %text% at %text%`: the parts between the delimiter. A list of texts.
fn split(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let text = text_of(&slot(cx, m, 0)?);
    let delimiter = text_of(&slot(cx, m, 1)?);
    if delimiter.is_empty() {
        return Err(RunError::new("can't split a text at an empty delimiter"));
    }
    Ok(Value::List(
        text.split(delimiter.as_str())
            .map(|p| Value::Text(p.to_owned()))
            .collect(),
    ))
}

/// `replace [all] %text% with %text% in %text%`: every occurrence replaced.
fn replace(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let from = text_of(&slot(cx, m, 0)?);
    let to = text_of(&slot(cx, m, 1)?);
    let text = text_of(&slot(cx, m, 2)?);
    if from.is_empty() {
        return Ok(Value::Text(text));
    }
    Ok(Value::Text(text.replace(from.as_str(), &to)))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression("length", &["[the] length of %text%"], 0, "integer", length)?;
    reg.add_expression("case", &["(1¦upper|2¦lower)case %text%"], 0, "text", case)?;
    reg.add_expression(
        "substring",
        &["[the] (part|sub[ ](text|string)) of %text% (between|from) %number% (and|to) %number%"],
        0,
        "text",
        substring,
    )?;
    reg.add_expression(
        "character at",
        &["[the] character[s] at [index] %number% (in|of) %text%"],
        0,
        "text",
        char_at,
    )?;
    reg.add_expression(
        "join",
        &[
            "(concat[enate]|join) %texts%",
            "(concat[enate]|join) %texts% (with|using|by) [[the] delimiter] %text%",
        ],
        0,
        "text",
        join,
    )?;
    reg.add_expression(
        "split",
        &[
            "split %text% (at|using|by) [[the] delimiter] %text%",
            "%text% split (at|using|by) [[the] delimiter] %text%",
        ],
        0,
        "object",
        split,
    )?;
    reg.add_expression(
        "replace",
        &["replace [all] %text% with %text% in %text%"],
        0,
        "text",
        replace,
    )?;
    Ok(())
}
