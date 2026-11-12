//! The interpreter. A trigger is a stack of frames (its body, loop and while bodies); one step
//! runs statements until the run ends, a `wait` asks for ticks, or the step budget is spent. A
//! run that waits keeps its frames and is resumed by the scheduler, so nothing blocks a tick.

use crate::classes;
use crate::error::RunError;
use crate::host::Host;
use crate::parse::{Body, Cond, Function, Node};
use crate::registry::Flow;
use crate::value::Value;
use crate::vars::Variables;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Script functions by lower-case name.
pub type Functions = HashMap<String, Arc<Function>>;

/// What fired a trigger: the player, the chat message and the command arguments, when there are
/// any, and the event's other values by name (`victim`, `block`, `from`, ...; see
/// `syntax/events/mod.rs`). Clones share the cancel flag, so `cancel event` in a trigger is seen
/// by the glue that fired it, once [`crate::Manager::fire`] returns.
#[derive(Clone, Debug, Default)]
pub struct Event {
    pub player: Option<Value>,
    pub message: Option<String>,
    pub args: Vec<String>,
    /// The event's other values, in the order they were set.
    pub values: Vec<(&'static str, Value)>,
    /// The cancel flag, shared by the clones of the event. Use [`Event::cancel`].
    pub cancelled: Arc<AtomicBool>,
}

impl Event {
    /// Sets one of the event's other values (the glue does this when it fires the event).
    pub fn with(mut self, name: &'static str, value: Value) -> Self {
        self.values.retain(|(n, _)| *n != name);
        self.values.push((name, value));
        self
    }

    /// One of the event's other values: `victim`, `block`, `location`, ...
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.values.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// Cancels the event (`cancel event`).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Lets the event go on (`uncancel event`).
    pub fn uncancel(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Whether a trigger cancelled the event.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// The shared state a run reads and writes.
pub struct Env<'c> {
    pub vars: &'c mut Variables,
    pub funcs: &'c Functions,
    pub host: &'c dyn Host,
}

/// What an effect, a condition or an expression can reach while it runs.
pub struct Ctx<'c> {
    pub vars: &'c mut Variables,
    /// Temporary variables of this run (`{_x}`), and `loop-value` / `loop-index`.
    pub locals: &'c mut Variables,
    pub funcs: &'c Functions,
    pub host: &'c dyn Host,
    pub event: &'c Event,
    pub depth: usize,
}

/// How far a run got.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing is left to do.
    Done,
    /// Continue after this many ticks (a `wait`, or the step budget ran out).
    Wait(u64),
    /// A statement failed on this line.
    Failed(RunError, usize),
}

/// Statements one step may run before it yields to the next tick.
pub const STEP_BUDGET: u32 = 10_000;
const MAX_DEPTH: usize = 64;

impl Ctx<'_> {
    /// Calls a script function with arguments already evaluated. It runs to its end in this
    /// call; a `wait` inside a function is an error.
    pub fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RunError> {
        let key = name.to_ascii_lowercase();
        let f = self
            .funcs
            .get(&key)
            .cloned()
            .ok_or_else(|| RunError::new(format!("there is no function named {name}")))?;
        if self.depth >= MAX_DEPTH {
            return Err(RunError::new(format!("{name} calls itself too deeply")));
        }
        if args.len() != f.params.len() {
            return Err(RunError::new(format!(
                "{name} takes {} argument(s), got {}",
                f.params.len(),
                args.len()
            )));
        }
        let mut locals = Variables::new();
        for (p, a) in f.params.iter().zip(args) {
            let v = classes::convert(a, p.class, self.host).ok_or_else(|| {
                RunError::new(format!(
                    "argument {} of {name} must be a {}",
                    p.name, p.class
                ))
            })?;
            locals.set(&format!("_{}", p.name), v);
        }
        let mut run = Run::new(
            f.body.clone(),
            f.file.clone(),
            self.event.clone(),
            locals,
            self.depth + 1,
        );
        let mut env = Env {
            vars: &mut *self.vars,
            funcs: self.funcs,
            host: self.host,
        };
        let mut budget = STEP_BUDGET;
        match run.step(&mut env, &mut budget) {
            Outcome::Done => {
                let value = run.ret.take().unwrap_or(Value::Null);
                match f.returns {
                    Some(class) if !value.is_null() => classes::convert(value, class, self.host)
                        .ok_or_else(|| {
                            RunError::new(format!("{name} returned a value that is not a {class}"))
                        }),
                    _ => Ok(value),
                }
            }
            Outcome::Wait(_) => Err(RunError::new(format!(
                "{name} waits or runs too long; functions cannot wait"
            ))),
            Outcome::Failed(e, line) => Err(RunError::new(format!(
                "{} (line {line} of {name})",
                e.message
            ))),
        }
    }
}

enum Kind {
    Block,
    /// A loop over `items`; `times` for `loop N times` (whose items are the numbers 1 to N).
    Loop {
        items: Vec<Value>,
        idx: usize,
        times: bool,
    },
    While { cond: Arc<Cond>, line: usize },
}

struct Frame {
    body: Body,
    pc: usize,
    kind: Kind,
}

/// One execution of a trigger or function body.
pub struct Run {
    frames: Vec<Frame>,
    pub locals: Variables,
    pub event: Event,
    pub file: Arc<str>,
    pub depth: usize,
    /// The value of a `return`, once it ran.
    pub ret: Option<Value>,
    /// The `loop-<class>` local the last refresh set, so it can be cleared.
    typed_local: Option<String>,
}

impl Run {
    pub fn new(body: Body, file: Arc<str>, event: Event, locals: Variables, depth: usize) -> Run {
        Run {
            frames: vec![Frame {
                body,
                pc: 0,
                kind: Kind::Block,
            }],
            locals,
            event,
            file,
            depth,
            ret: None,
            typed_local: None,
        }
    }

    pub fn is_done(&self) -> bool {
        self.frames.is_empty()
    }

    /// Runs until the run ends, waits, fails or the budget is spent.
    pub fn step(&mut self, env: &mut Env<'_>, budget: &mut u32) -> Outcome {
        loop {
            let Some(top) = self.frames.last() else {
                return Outcome::Done;
            };
            if *budget == 0 {
                return Outcome::Wait(1);
            }
            *budget -= 1;
            let body = top.body.clone();
            let pc = top.pc;
            if pc >= body.len() {
                if let Err((e, line)) = self.finish_frame(env) {
                    return Outcome::Failed(e, line);
                }
                continue;
            }
            self.frames.last_mut().expect("a frame").pc += 1;
            let node = &body[pc];
            match node {
                Node::Effect { handler, m, line } => {
                    let result = {
                        let mut cx = ctx(self, env);
                        handler.run(&mut cx, m)
                    };
                    match result {
                        Err(e) => return Outcome::Failed(e, *line),
                        Ok(Flow::Next) => {}
                        Ok(Flow::Wait(ticks)) => return Outcome::Wait(ticks.max(1)),
                        Ok(Flow::Stop) => {
                            self.frames.clear();
                            return Outcome::Done;
                        }
                        Ok(Flow::Exit { levels, loops_only }) => {
                            if let Err(e) = self.exit(levels, loops_only) {
                                return Outcome::Failed(e, *line);
                            }
                        }
                        Ok(Flow::Return(value)) => {
                            self.ret = value;
                            self.frames.clear();
                            return Outcome::Done;
                        }
                    }
                }
                Node::Expr { e, line } => {
                    let result = {
                        let mut cx = ctx(self, env);
                        e.eval(&mut cx)
                    };
                    if let Err(e) = result {
                        return Outcome::Failed(e, *line);
                    }
                }
                Node::If { branches, line } => {
                    let mut chosen = None;
                    for b in branches {
                        let holds = match &b.cond {
                            None => Ok(true),
                            Some(c) => {
                                let mut cx = ctx(self, env);
                                c.check(&mut cx)
                            }
                        };
                        match holds {
                            Err(e) => return Outcome::Failed(e, *line),
                            Ok(true) => {
                                chosen = Some(b.body.clone());
                                break;
                            }
                            Ok(false) => {}
                        }
                    }
                    if let Some(body) = chosen {
                        self.frames.push(Frame {
                            body,
                            pc: 0,
                            kind: Kind::Block,
                        });
                    }
                }
                Node::Loop {
                    iter,
                    body,
                    times,
                    line,
                } => {
                    let value = {
                        let mut cx = ctx(self, env);
                        iter.eval(&mut cx)
                    };
                    let items = if *times {
                        match times_count(value) {
                            Ok(n) => (1..=n).map(Value::Int).collect(),
                            Err(e) => return Outcome::Failed(e, *line),
                        }
                    } else {
                        match value {
                            Ok(Value::List(v)) => v,
                            Ok(Value::Null) => Vec::new(),
                            Ok(other) => vec![other],
                            Err(e) => return Outcome::Failed(e, *line),
                        }
                    };
                    if !items.is_empty() {
                        self.frames.push(Frame {
                            body: body.clone(),
                            pc: 0,
                            kind: Kind::Loop {
                                items,
                                idx: 0,
                                times: *times,
                            },
                        });
                        self.refresh_loop_locals();
                    }
                }
                Node::While { cond, body, line } => {
                    let holds = {
                        let mut cx = ctx(self, env);
                        cond.check(&mut cx)
                    };
                    match holds {
                        Err(e) => return Outcome::Failed(e, *line),
                        Ok(true) => self.frames.push(Frame {
                            body: body.clone(),
                            pc: 0,
                            kind: Kind::While {
                                cond: cond.clone(),
                                line: *line,
                            },
                        }),
                        Ok(false) => {}
                    }
                }
            }
        }
    }

    /// The end of a frame's body: a block is left, a loop takes its next item, a while asks again.
    fn finish_frame(&mut self, env: &mut Env<'_>) -> Result<(), (RunError, usize)> {
        enum Next {
            Pop,
            Again,
            Recheck(Arc<Cond>, usize),
        }
        let next = match self.frames.last().map(|f| &f.kind) {
            None => return Ok(()),
            Some(Kind::Block) => Next::Pop,
            Some(Kind::Loop { items, idx, .. }) => {
                if idx + 1 < items.len() {
                    Next::Again
                } else {
                    Next::Pop
                }
            }
            Some(Kind::While { cond, line }) => Next::Recheck(cond.clone(), *line),
        };
        match next {
            Next::Pop => {
                self.frames.pop();
                self.refresh_loop_locals();
            }
            Next::Again => {
                let f = self.frames.last_mut().expect("a frame");
                if let Kind::Loop { idx, .. } = &mut f.kind {
                    *idx += 1;
                }
                f.pc = 0;
                self.refresh_loop_locals();
            }
            Next::Recheck(cond, line) => {
                let holds = {
                    let mut cx = ctx(self, env);
                    cond.check(&mut cx).map_err(|e| (e, line))?
                };
                if holds {
                    self.frames.last_mut().expect("a frame").pc = 0;
                } else {
                    self.frames.pop();
                    self.refresh_loop_locals();
                }
            }
        }
        Ok(())
    }

    /// Leaves sections: `levels` of them, or only loops when `loops_only`.
    fn exit(&mut self, levels: usize, loops_only: bool) -> Result<(), RunError> {
        if loops_only && !self.frames.iter().any(|f| !matches!(f.kind, Kind::Block)) {
            return Err(RunError::new("there is no loop to exit"));
        }
        let mut remaining = levels;
        while remaining > 0 {
            let Some(f) = self.frames.pop() else { break };
            let is_loop = !matches!(f.kind, Kind::Block);
            if loops_only && !is_loop {
                continue;
            }
            remaining -= 1;
        }
        self.refresh_loop_locals();
        Ok(())
    }

    /// Sets the loop locals from the frames. `loop-value` and `loop-index` come from the innermost
    /// list loop; `loop-number` from the innermost `times` loop; `loop-counter` and
    /// `loop-iteration` (1-based) from the innermost loop of any kind. A list loop also sets
    /// `loop-<class>` for its value's class (`loop-player`), so a typed loop names its value.
    fn refresh_loop_locals(&mut self) {
        let mut value: Option<(Value, i64)> = None;
        let mut times: Option<Value> = None;
        let mut any: Option<i64> = None;
        for f in self.frames.iter().rev() {
            if let Kind::Loop {
                items,
                idx,
                times: is_times,
            } = &f.kind
            {
                let n = *idx as i64 + 1;
                any.get_or_insert(n);
                if *is_times {
                    times.get_or_insert_with(|| items[*idx].clone());
                } else {
                    value.get_or_insert_with(|| (items[*idx].clone(), n));
                }
            }
        }
        let typed = value
            .as_ref()
            .map(|(v, _)| v.class())
            .filter(|c| !matches!(*c, "object" | "number" | "integer"))
            .map(|c| format!("loop-{c}"));
        if let Some(old) = self.typed_local.take() {
            self.locals.delete(&old);
        }
        match (&value, &typed) {
            (Some((v, _)), Some(name)) => {
                self.locals.set(name, v.clone());
                self.typed_local = Some(name.clone());
            }
            _ => {}
        }
        self.locals
            .set("loop-value", value.as_ref().map_or(Value::Null, |(v, _)| v.clone()));
        self.locals.set(
            "loop-index",
            value.as_ref().map_or(Value::Null, |(_, n)| Value::Int(*n)),
        );
        self.locals.set("loop-number", times.unwrap_or(Value::Null));
        let counter = any.map_or(Value::Null, Value::Int);
        self.locals.set("loop-counter", counter.clone());
        self.locals.set("loop-iteration", counter);
    }
}

/// How many times a `loop N times` runs: a whole number, or a number rounded down.
fn times_count(v: Result<Value, RunError>) -> Result<i64, RunError> {
    let v = v?;
    let n = match v.as_i64() {
        Some(n) => n,
        None => v
            .as_f64()
            .map(f64::floor)
            .filter(|f| f.is_finite())
            .map(|f| f as i64)
            .ok_or_else(|| RunError::new(format!("loop needs a number, got {}", v.class())))?,
    };
    if n > MAX_LOOP_TIMES {
        return Err(RunError::new(format!(
            "a loop of {n} times is too long (at most {MAX_LOOP_TIMES})"
        )));
    }
    Ok(n.max(0))
}

/// The most iterations one `loop N times` may run.
pub const MAX_LOOP_TIMES: i64 = 100_000;

/// A context over a run's own state and the shared environment. A macro, because the borrow of
/// the run's fields has to stay split from the frames.
fn ctx<'c>(run: &'c mut Run, env: &'c mut Env<'_>) -> Ctx<'c> {
    Ctx {
        vars: &mut *env.vars,
        locals: &mut run.locals,
        funcs: env.funcs,
        host: env.host,
        event: &run.event,
        depth: run.depth,
    }
}
