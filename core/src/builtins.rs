//! The builtin syntax every server has: assignments and list changes, `wait`, messages, `return`,
//! `stop`/`exit`, the events and the expressions of a trigger's context. The syntax library
//! (`core/src/syntax/`) adds the rest.

use crate::error::RunError;
use crate::expr::{Expr, Op, arith};
use crate::pattern::PatternError;
use crate::registry::ExprSyntax;
use crate::registry::{Change, Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;
use crate::vars::Variables;

/// Registers the builtins into `reg`.
pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect("set", &["set %object% to %object%"], 0, set)?;
    reg.add_effect("add", &["add %object% to %object%"], 0, add)?;
    reg.add_effect("remove", &["remove %object% from %object%"], 0, remove)?;
    reg.add_effect("subtract", &["subtract %number% from %object%"], 0, remove)?;
    reg.add_effect("delete", &["(delete|clear) %object%"], 0, delete)?;
    reg.add_effect("wait", &["wait %timespan%"], 0, wait)?;
    reg.add_effect("send", &["(send|message) %text% to %player%"], 0, send)?;
    reg.add_effect("broadcast", &["broadcast %text%"], 0, broadcast)?;
    reg.add_effect("return", &["return [%object%]"], 0, ret)?;
    reg.add_effect("stop", &["(stop|exit) [trigger]"], 0, stop)?;
    reg.add_effect(
        "exit loop",
        &["(stop|exit) [1|a|the|this] loop"],
        0,
        exit_loop,
    )?;
    reg.add_effect(
        "exit section",
        &["(stop|exit) [1|a|the|this] section"],
        0,
        exit_section,
    )?;
    reg.add_effect(
        "exit n",
        &["(stop|exit) <[0-9]+> (1¦loops|2¦sections)"],
        0,
        exit_n,
    )?;

    reg.add_condition("is set", &["%object% (is|are) set"], 0, is_set)?;
    reg.add_condition("is not set", &["%object% (is|are) not set"], 0, is_not_set)?;
    reg.add_condition(
        "is one of",
        &["%object% (is|are) one of %objects%"],
        0,
        is_one_of,
    )?;
    reg.add_condition(
        "is not one of",
        &["%object% (is|are) not one of %objects%"],
        0,
        is_not_one_of,
    )?;

    reg.add_expression("player", &["[the] player"], 0, "player", event_player)?;
    reg.add_expression("message", &["[the] message"], 0, "text", event_message)?;
    reg.add_expression("arg", &["arg-%integer%"], 0, "text", event_arg)?;
    reg.add_expression("loop-value", &["loop-value"], 0, "object", loop_value)?;
    reg.add_expression("loop-index", &["loop-index"], 0, "integer", loop_index)?;
    reg.add_expression("loop-number", &["loop-number"], 0, "object", loop_number)?;
    reg.add_expression(
        "loop-counter",
        &["loop-(counter|iteration)"],
        0,
        "integer",
        loop_counter,
    )?;
    reg.add_expression("loop-player", &["loop-player"], 0, "player", loop_player)?;
    reg.add_expression("loop-entity", &["loop-entity"], 0, "object", loop_entity)?;
    reg.add_expression(
        "difference",
        &["difference between %object% and %object%"],
        0,
        "object",
        difference,
    )?;

    reg.add_event("join", &["join"])?;
    reg.add_event("quit", &["quit", "leave"])?;
    reg.add_event("chat", &["chat"])?;
    reg.add_event("load", &["[script] load"])?;
    reg.add_event("unload", &["unload"])?;
    Ok(())
}

/// The variable a slot names, or an error that says what was expected.
fn target(cx: &mut Ctx<'_>, e: Option<&Expr>, what: &str) -> Result<String, RunError> {
    let e = e.ok_or_else(|| RunError::new(format!("missing {what}")))?;
    e.var_name(cx)?
        .ok_or_else(|| RunError::new(format!("{what} must be a variable")))
}

fn value(cx: &mut Ctx<'_>, e: Option<&Expr>) -> Result<Value, RunError> {
    match e {
        Some(e) => e.eval(cx),
        None => Ok(Value::Null),
    }
}

/// The variable store a name lives in: `{_x}` is local to the run.
fn store<'a>(cx: &'a mut Ctx<'_>, name: &str) -> &'a mut Variables {
    if name.starts_with('_') {
        &mut *cx.locals
    } else {
        &mut *cx.vars
    }
}

/// `set`/`add`/`remove` on a changeable syntax expression rather than a variable.
fn changed(
    cx: &mut Ctx<'_>,
    e: Option<&Expr>,
    how: Change,
    by: Option<&Expr>,
) -> Option<Result<Flow, RunError>> {
    let e = e?;
    if matches!(e, Expr::Var(_)) {
        return None;
    }
    let v = match value(cx, by) {
        Ok(v) => v,
        Err(err) => return Some(Err(err)),
    };
    Some(e.change(cx, how, &v)?.map(|()| Flow::Next))
}

fn set(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    if let Some(r) = changed(cx, m.expr(0), Change::Set, m.expr(1)) {
        return r;
    }
    let name = target(cx, m.expr(0), "the value to set")?;
    if let Some(base) = name.strip_suffix("::*") {
        // `set {x::*} to <values>`: the list is replaced, its entries numbered from 1 (Skript).
        let items = match value(cx, m.expr(1))? {
            Value::List(items) => items,
            Value::Null => Vec::new(),
            other => vec![other],
        };
        let s = store(cx, base);
        s.delete_children(base);
        for (i, item) in items.into_iter().enumerate() {
            s.set(&format!("{base}::{}", i + 1), item);
        }
        return Ok(Flow::Next);
    }
    let v = value(cx, m.expr(1))?;
    let s = store(cx, &name);
    match v {
        Value::List(items) => {
            s.delete(&name);
            for (i, item) in items.into_iter().enumerate() {
                s.set(&format!("{name}::{}", i + 1), item);
            }
        }
        other => s.set(&name, other),
    }
    Ok(Flow::Next)
}

/// `add x to {y}`: adds numbers (and joins text), or appends to a list variable `{y::*}`.
fn add(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    if let Some(r) = changed(cx, m.expr(1), Change::Add, m.expr(0)) {
        return r;
    }
    let name = target(cx, m.expr(1), "the variable to add to")?;
    let v = value(cx, m.expr(0))?;
    if let Some(base) = name.strip_suffix("::*") {
        let items = match v {
            Value::List(items) => items,
            other => vec![other],
        };
        let s = store(cx, base);
        let mut next = s.next_index(base);
        for item in items {
            s.set(&format!("{base}::{next}"), item);
            next += 1;
        }
        return Ok(Flow::Next);
    }
    let current = store(cx, &name).get(&name).cloned().unwrap_or(Value::Null);
    let new = if current.is_null() {
        v
    } else {
        arith(Op::Add, &current, &v)?
    };
    store(cx, &name).set(&name, new);
    Ok(Flow::Next)
}

/// `remove x from {y}`: subtracts numbers, or removes equal entries from a list.
pub(crate) fn remove(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    if let Some(r) = changed(cx, m.expr(1), Change::Remove, m.expr(0)) {
        return r;
    }
    let name = target(cx, m.expr(1), "the variable to remove from")?;
    let v = value(cx, m.expr(0))?;
    if let Some(base) = name.strip_suffix("::*") {
        let items = match v {
            Value::List(items) => items,
            other => vec![other],
        };
        let s = store(cx, base);
        for item in items {
            s.remove_matching(base, &item);
        }
        return Ok(Flow::Next);
    }
    let current = store(cx, &name).get(&name).cloned().unwrap_or(Value::Null);
    if current.is_null() {
        return Ok(Flow::Next);
    }
    let new = arith(Op::Sub, &current, &v)?;
    store(cx, &name).set(&name, new);
    Ok(Flow::Next)
}

/// `delete {x}` removes a variable with everything below it; `delete {x::*}` only the entries.
fn delete(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let name = target(cx, m.expr(0), "the variable to delete")?;
    match name.strip_suffix("::*") {
        Some(base) => {
            store(cx, base).delete_children(base);
        }
        None => {
            store(cx, &name).delete(&name);
        }
    }
    Ok(Flow::Next)
}

/// Server ticks in a timespan: 50 ms each, at least one.
pub fn ticks_of(ms: i64) -> u64 {
    ((ms as f64 / 50.0).ceil() as u64).max(1)
}

fn wait(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    match value(cx, m.expr(0))? {
        Value::Timespan(ms) => Ok(Flow::Wait(ticks_of(ms))),
        other => Err(RunError::new(format!(
            "wait needs a timespan, got {}",
            other.class()
        ))),
    }
}

fn send(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let text = value(cx, m.expr(0))?.to_text();
    match value(cx, m.expr(1))? {
        Value::Player { uuid, .. } => {
            cx.host.send(uuid, &text);
            Ok(Flow::Next)
        }
        other => Err(RunError::new(format!(
            "send needs a player, got {}",
            other.class()
        ))),
    }
}

fn broadcast(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let text = value(cx, m.expr(0))?.to_text();
    cx.host.broadcast(&text);
    Ok(Flow::Next)
}

fn ret(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    match m.expr(0) {
        Some(e) => Ok(Flow::Return(Some(e.eval(cx)?))),
        None => Ok(Flow::Return(None)),
    }
}

fn stop(_: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    Ok(Flow::Stop)
}

fn exit_loop(_: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    Ok(Flow::Exit {
        levels: 1,
        loops_only: true,
    })
}

fn exit_section(_: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    Ok(Flow::Exit {
        levels: 1,
        loops_only: false,
    })
}

fn exit_n(_: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let n: usize = m
        .regex(0)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| RunError::new("exit needs a number of levels"))?;
    if n == 0 {
        return Ok(Flow::Next);
    }
    Ok(Flow::Exit {
        levels: n,
        loops_only: m.has_mark(1),
    })
}

fn is_set(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    Ok(is_something(&value(cx, m.expr(0))?))
}

fn is_not_set(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    Ok(!is_something(&value(cx, m.expr(0))?))
}

fn is_something(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::List(items) => !items.is_empty(),
        _ => true,
    }
}

fn event_player(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    cx.event
        .player
        .clone()
        .ok_or_else(|| RunError::new("there is no player in this event"))
}

fn event_message(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    cx.event
        .message
        .clone()
        .map(Value::Text)
        .ok_or_else(|| RunError::new("there is no message in this event"))
}

fn event_arg(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let n = m
        .expr(0)
        .map(|e| e.eval(cx))
        .transpose()?
        .and_then(|v| v.as_i64())
        .ok_or_else(|| RunError::new("arg needs a number"))?;
    usize::try_from(n - 1)
        .ok()
        .and_then(|i| cx.event.args.get(i).cloned())
        .map(Value::Text)
        .ok_or_else(|| RunError::new(format!("there is no argument {n}")))
}

fn loop_value(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx.locals.get("loop-value").cloned().unwrap_or(Value::Null))
}

fn loop_index(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx.locals.get("loop-index").cloned().unwrap_or(Value::Null))
}

fn loop_number(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx.locals.get("loop-number").cloned().unwrap_or(Value::Null))
}

/// `loop-counter` and `loop-iteration`: the 1-based count of the innermost loop.
fn loop_counter(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx
        .locals
        .get("loop-counter")
        .cloned()
        .unwrap_or(Value::Null))
}

/// `loop-entity`: the value of the innermost list loop over entities (`loop all entities`). Skript
/// names that loop's value by its type; here the entity loop's value is the loop value itself.
fn loop_entity(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx
        .locals
        .get("loop-entity")
        .or_else(|| cx.locals.get("loop-value"))
        .cloned()
        .unwrap_or(Value::Null))
}

/// `loop-player`: the value of the innermost list loop over players.
fn loop_player(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(cx.locals.get("loop-player").cloned().unwrap_or(Value::Null))
}

fn is_one_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    one_of(cx, m)
}

fn is_not_one_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    Ok(!one_of(cx, m)?)
}

/// `x is one of "a", "b" or "c"`: equal to one of the values, by the rules of `is`.
fn one_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let x = value(cx, m.expr(0))?;
    let options = match value(cx, m.expr(1))? {
        Value::List(items) => items,
        other => vec![other],
    };
    Ok(options.iter().any(|o| crate::syntax::support::equal(&x, o)))
}

/// The value of a default operator (`a ? b`): `a` when it is set, else `b`.
pub(crate) struct DefaultValue;

impl ExprSyntax for DefaultValue {
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
        let first = value(cx, m.expr(0))?;
        let set = match &first {
            Value::Null => false,
            Value::List(items) => !items.is_empty(),
            _ => true,
        };
        if set { Ok(first) } else { value(cx, m.expr(1)) }
    }
}

/// `difference between a and b`: the absolute difference of two dates, timespans or numbers.
fn difference(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let a = value(cx, m.expr(0))?;
    let b = value(cx, m.expr(1))?;
    match (&a, &b) {
        (Value::Date(x), Value::Date(y)) => Ok(Value::Timespan((x - y).abs())),
        (Value::Timespan(x), Value::Timespan(y)) => Ok(Value::Timespan((x - y).abs())),
        (Value::Int(x), Value::Int(y)) => Ok(Value::Int((x - y).abs())),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => Ok(Value::Num((x - y).abs())),
            _ => Err(RunError::new(format!(
                "there is no difference between {} and {}",
                a.class(),
                b.class()
            ))),
        },
    }
}
