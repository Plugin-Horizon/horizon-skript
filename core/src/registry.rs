//! The syntax registry: the effects, conditions, expressions and events the parser knows.
//! Builtins are registered by [`crate::builtins`], the rest by [`crate::syntax`], which is where
//! the syntax library lives (see `core/src/syntax/README.md`).

use crate::error::RunError;
use crate::expr::Expr;
use crate::pattern::{Pattern, PatternError};
use crate::run::Ctx;
use crate::value::Value;
use std::sync::Arc;

/// A built-in function (`abs(-2)`): gets its arguments already evaluated, with lists in
/// `a::*` form as one list value each.
pub type FunctionFn = fn(&mut Ctx<'_>, Vec<Value>) -> Result<Value, RunError>;

/// One built-in function. Several entries may share a name when they take different numbers of
/// arguments (`min(1, 2)` and `min({list::*})` are both `min` with one or more arguments).
pub struct FunctionEntry {
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    /// The class of the result, as for expressions.
    pub returns: &'static str,
    pub handler: FunctionFn,
}

/// Adapts a [`FunctionFn`] to the expression interface: evaluates the arguments in slot order.
pub(crate) struct FunctionCall(pub FunctionFn);

impl ExprSyntax for FunctionCall {
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
        let mut args = Vec::with_capacity(m.exprs.len());
        for slot in &m.exprs {
            args.push(match slot {
                Some(e) => e.eval(cx)?,
                None => Value::Null,
            });
        }
        (self.0)(cx, args)
    }
}

/// What a matched syntax element carries: its expressions by slot, parse marks, tags and regex
/// groups (see [`crate::pattern`]).
#[derive(Clone, Default)]
pub struct Matched {
    pub exprs: Vec<Option<Expr>>,
    pub marks: Vec<u32>,
    pub tags: Vec<String>,
    pub regex: Vec<String>,
    /// Which alternative of the pattern matched.
    pub variant: usize,
}

impl Matched {
    /// The expression in a slot, if the slot was filled.
    pub fn expr(&self, slot: usize) -> Option<&Expr> {
        self.exprs.get(slot).and_then(Option::as_ref)
    }

    pub fn has_mark(&self, mark: u32) -> bool {
        self.marks.contains(&mark)
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// The `n`th regex group of the match (0-based, in pattern order).
    pub fn regex(&self, n: usize) -> Option<&str> {
        self.regex.get(n).map(String::as_str)
    }
}

/// What a statement does to the trigger that runs it.
#[derive(Clone, Debug, PartialEq)]
pub enum Flow {
    /// Go on with the next statement.
    Next,
    /// Pause this trigger for this many server ticks (at least 1).
    Wait(u64),
    /// End the trigger (`stop`, `exit`).
    Stop,
    /// Leave this many sections, or loops only (`stop loop`, `exit 2 sections`).
    Exit { levels: usize, loops_only: bool },
    /// Leave the function with a value (`return`).
    Return(Option<Value>),
}

/// An effect: a statement that does something.
pub trait Effect: Send + Sync {
    fn run(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError>;
}

/// A condition: a test that holds or does not.
pub trait Condition: Send + Sync {
    fn check(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError>;
}

/// An expression with a value.
pub trait ExprSyntax: Send + Sync {
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError>;
}

impl<F> Effect for F
where
    F: Fn(&mut Ctx<'_>, &Matched) -> Result<Flow, RunError> + Send + Sync,
{
    fn run(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
        self(cx, m)
    }
}

impl<F> Condition for F
where
    F: Fn(&mut Ctx<'_>, &Matched) -> Result<bool, RunError> + Send + Sync,
{
    fn check(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
        self(cx, m)
    }
}

impl<F> ExprSyntax for F
where
    F: Fn(&mut Ctx<'_>, &Matched) -> Result<Value, RunError> + Send + Sync,
{
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
        self(cx, m)
    }
}

/// One registered syntax element.
pub struct Entry<H: ?Sized> {
    pub name: &'static str,
    pub patterns: Vec<Pattern>,
    /// Lower values are tried first. Builtins use 0.
    pub priority: i32,
    pub handler: Arc<H>,
    /// For expressions: the class of the value (`text`, `number`, ...), `object` if unknown.
    pub returns: &'static str,
}

/// An event a trigger can start with (`on join:`): the name the glue fires it under.
pub struct EventEntry {
    pub event: &'static str,
    pub patterns: Vec<Pattern>,
}

#[derive(Default)]
pub struct Registry {
    pub(crate) effects: Vec<Entry<dyn Effect>>,
    pub(crate) conditions: Vec<Entry<dyn Condition>>,
    pub(crate) expressions: Vec<Entry<dyn ExprSyntax>>,
    pub(crate) events: Vec<EventEntry>,
    pub(crate) functions: Vec<FunctionEntry>,
}

fn compile_all(patterns: &[&str]) -> Result<Vec<Pattern>, PatternError> {
    patterns.iter().map(|p| Pattern::compile(p)).collect()
}

impl Registry {
    /// An empty registry, without any syntax.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The registry a server uses: the builtins, then the syntax library.
    pub fn new() -> Self {
        let mut r = Self::empty();
        crate::builtins::register(&mut r).expect("builtin patterns compile");
        crate::syntax::register(&mut r);
        r
    }

    /// Adds an effect with one or more patterns. Fails when a pattern does not compile.
    pub fn add_effect(
        &mut self,
        name: &'static str,
        patterns: &[&str],
        priority: i32,
        handler: impl Effect + 'static,
    ) -> Result<(), PatternError> {
        let entry = Entry {
            name,
            patterns: compile_all(patterns)?,
            priority,
            handler: Arc::new(handler) as Arc<dyn Effect>,
            returns: "",
        };
        self.effects.push(entry);
        self.effects.sort_by_key(|e| e.priority);
        Ok(())
    }

    /// Adds a condition (usable after `if` and `while`).
    pub fn add_condition(
        &mut self,
        name: &'static str,
        patterns: &[&str],
        priority: i32,
        handler: impl Condition + 'static,
    ) -> Result<(), PatternError> {
        let entry = Entry {
            name,
            patterns: compile_all(patterns)?,
            priority,
            handler: Arc::new(handler) as Arc<dyn Condition>,
            returns: "",
        };
        self.conditions.push(entry);
        self.conditions.sort_by_key(|e| e.priority);
        Ok(())
    }

    /// Adds an expression. `returns` is its class (`text`, `number`, `player`, ...), used to
    /// check it against the slots it is put in.
    pub fn add_expression(
        &mut self,
        name: &'static str,
        patterns: &[&str],
        priority: i32,
        returns: &'static str,
        handler: impl ExprSyntax + 'static,
    ) -> Result<(), PatternError> {
        let entry = Entry {
            name,
            patterns: compile_all(patterns)?,
            priority,
            handler: Arc::new(handler) as Arc<dyn ExprSyntax>,
            returns,
        };
        self.expressions.push(entry);
        self.expressions.sort_by_key(|e| e.priority);
        Ok(())
    }

    /// Adds an event (`on <pattern>:`). The glue fires it with [`crate::Manager::fire`].
    pub fn add_event(
        &mut self,
        event: &'static str,
        patterns: &[&str],
    ) -> Result<(), PatternError> {
        self.events.push(EventEntry {
            event,
            patterns: compile_all(patterns)?,
        });
        Ok(())
    }

    /// Adds a built-in function taking `min_args` to `max_args` arguments.
    pub fn add_function(
        &mut self,
        name: &'static str,
        min_args: usize,
        max_args: usize,
        returns: &'static str,
        handler: FunctionFn,
    ) {
        self.functions.push(FunctionEntry {
            name,
            min_args,
            max_args,
            returns,
            handler,
        });
    }

    /// The built-in function a call with this many arguments goes to, if there is one.
    pub fn function(&self, name: &str, args: usize) -> Option<&FunctionEntry> {
        self.functions
            .iter()
            .find(|f| f.name == name && (f.min_args..=f.max_args).contains(&args))
    }

    /// Names of the registered effects, for documentation and `/skript info`.
    pub fn effect_names(&self) -> Vec<&'static str> {
        self.effects.iter().map(|e| e.name).collect()
    }

    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.effects.len(),
            self.conditions.len(),
            self.expressions.len(),
            self.events.len(),
        )
    }
}
