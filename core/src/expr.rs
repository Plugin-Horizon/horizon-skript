//! Expressions: the tree a value is computed from, the parser that reads it from text, and its
//! evaluation. Text literals interpolate `%expr%`, variables are `{name}`, `{_local}`,
//! `{list::*}`, arithmetic follows the usual precedence, `name(args)` calls a function.

use crate::classes;
use crate::error::RunError;
use crate::host::Host;
use crate::pattern::SlotSpec;
use crate::registry::{ExprSyntax, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;
use crate::vars::Variables;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub enum Expr {
    Lit(Value),
    /// A text with interpolated parts.
    Text(Vec<Part>),
    /// A variable: its interpolated name. A name ending in `::*` is a list.
    Var(Vec<Part>),
    Arith(Op, Box<Expr>, Box<Expr>),
    /// `a, b and c`, or a list of expressions in a plural slot.
    List(Vec<Expr>),
    /// A call of a script function.
    Call(String, Vec<Expr>),
    /// An expression from the registry.
    Syntax {
        handler: Arc<dyn ExprSyntax>,
        m: Matched,
        returns: &'static str,
    },
    /// A player name typed bare in a `%player%` slot, looked up when evaluated.
    PlayerName(String),
}

#[derive(Clone)]
pub enum Part {
    Text(String),
    Expr(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl Expr {
    /// The class of the value this expression gives, when it is known without running it.
    pub fn static_class(&self) -> Option<&'static str> {
        match self {
            Expr::Lit(v) => Some(v.class()),
            Expr::Text(_) => Some("text"),
            Expr::Syntax { returns, .. } if *returns != "object" => Some(*returns),
            Expr::PlayerName(_) => Some("player"),
            _ => None,
        }
    }

    /// Evaluates the expression. Values are computed fresh each time.
    pub fn eval(&self, cx: &mut Ctx<'_>) -> Result<Value, RunError> {
        match self {
            Expr::Lit(v) => Ok(v.clone()),
            Expr::Text(parts) => Ok(Value::Text(render(parts, cx)?)),
            Expr::Var(parts) => {
                let name = render(parts, cx)?;
                Ok(read_var(&name, cx))
            }
            Expr::Arith(op, a, b) => {
                let x = a.eval(cx)?;
                let y = b.eval(cx)?;
                arith(*op, &x, &y)
            }
            Expr::List(items) => {
                let mut out = Vec::new();
                for item in items {
                    match item.eval(cx)? {
                        Value::List(inner) => out.extend(inner),
                        other => out.push(other),
                    }
                }
                Ok(Value::List(out))
            }
            Expr::Call(name, args) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(a.eval(cx)?);
                }
                cx.call(name, values)
            }
            Expr::Syntax { handler, m, .. } => handler.eval(cx, m),
            Expr::PlayerName(name) => match cx.host.player_by_name(name) {
                Some((uuid, name)) => Ok(Value::Player { uuid, name }),
                None => Err(RunError::new(format!("there is no player named {name}"))),
            },
        }
    }
}

impl Expr {
    /// The variable name an expression names, when it is a variable (`{x::*}` gives `x::*`).
    /// Changes a changeable syntax expression (`x-coordinate of {_l}`); None when it is not one.
    pub fn change(
        &self,
        cx: &mut Ctx<'_>,
        how: crate::registry::Change,
        by: &Value,
    ) -> Option<Result<(), RunError>> {
        match self {
            Expr::Syntax { handler, m, .. } => handler.change(cx, m, how, by),
            _ => None,
        }
    }

    pub fn var_name(&self, cx: &mut Ctx<'_>) -> Result<Option<String>, RunError> {
        match self {
            Expr::Var(parts) => render(parts, cx).map(Some),
            _ => Ok(None),
        }
    }
}

pub(crate) fn render(parts: &[Part], cx: &mut Ctx<'_>) -> Result<String, RunError> {
    let mut out = String::new();
    for part in parts {
        match part {
            Part::Text(s) => out.push_str(s),
            Part::Expr(e) => out.push_str(&e.eval(cx)?.to_text()),
        }
    }
    Ok(out)
}

fn read_var(name: &str, cx: &Ctx<'_>) -> Value {
    let store: &Variables = if name.starts_with('_') {
        &*cx.locals
    } else {
        &*cx.vars
    };
    match name.strip_suffix("::*") {
        Some(base) => Value::List(store.values_below(base)),
        None => store.get(name).cloned().unwrap_or(Value::Null),
    }
}

/// A number while doing arithmetic: integers stay exact unless they overflow or divide badly.
#[derive(Clone, Copy)]
enum N {
    I(i64),
    F(f64),
}

fn as_n(v: &Value) -> Option<N> {
    match v {
        Value::Int(i) => Some(N::I(*i)),
        Value::Num(f) => Some(N::F(*f)),
        Value::Text(s) => match classes::parse_number(s.trim())? {
            Value::Int(i) => Some(N::I(i)),
            Value::Num(f) => Some(N::F(f)),
            _ => None,
        },
        _ => None,
    }
}

fn to_value(n: N) -> Result<Value, RunError> {
    match n {
        N::I(i) => Ok(Value::Int(i)),
        N::F(f) if f.is_finite() => Ok(Value::Num(f)),
        N::F(_) => Err(RunError::new("the result is not a finite number")),
    }
}

fn float(n: N) -> f64 {
    match n {
        N::I(i) => i as f64,
        N::F(f) => f,
    }
}

/// `+ - * / ^` on two values. Timespans add and subtract; text joins with `+` when the
/// operands are not both numbers.
pub fn arith(op: Op, a: &Value, b: &Value) -> Result<Value, RunError> {
    if let (Value::Timespan(x), Value::Timespan(y)) = (a, b) {
        match op {
            Op::Add => return Ok(Value::Timespan(x + y)),
            Op::Sub => return Ok(Value::Timespan(x - y)),
            _ => {}
        }
    }
    // A date moves by a timespan: `{date} + 2 days`, `subtract a day from {date}`.
    if let (Value::Date(d), Value::Timespan(t)) = (a, b) {
        match op {
            Op::Add => return Ok(Value::Date(d + t)),
            Op::Sub => return Ok(Value::Date(d - t)),
            _ => {}
        }
    }
    // A timespan scales by a number (`(random ...) * 1 second`, `1 hour / 2`).
    if let Some(scaled) = scale_timespan(op, a, b) {
        return scaled;
    }
    let (Some(x), Some(y)) = (as_n(a), as_n(b)) else {
        if op == Op::Add && (matches!(a, Value::Text(_)) || matches!(b, Value::Text(_))) {
            return Ok(Value::Text(format!("{}{}", a.to_text(), b.to_text())));
        }
        return Err(RunError::new(format!(
            "can't do this arithmetic on {} and {}",
            a.class(),
            b.class()
        )));
    };
    let r = match (op, x, y) {
        (Op::Add, N::I(p), N::I(q)) => p.checked_add(q).map_or(N::F(p as f64 + q as f64), N::I),
        (Op::Sub, N::I(p), N::I(q)) => p.checked_sub(q).map_or(N::F(p as f64 - q as f64), N::I),
        (Op::Mul, N::I(p), N::I(q)) => p.checked_mul(q).map_or(N::F(p as f64 * q as f64), N::I),
        (Op::Div, _, _) if float(y) == 0.0 => return Err(RunError::new("division by zero")),
        (Op::Div, N::I(p), N::I(q)) if p % q == 0 => N::I(p / q),
        (Op::Div, _, _) => N::F(float(x) / float(y)),
        (Op::Pow, N::I(p), N::I(q))
            if q >= 0 && u32::try_from(q).is_ok_and(|e| p.checked_pow(e).is_some()) =>
        {
            N::I(p.pow(q as u32))
        }
        (Op::Pow, _, _) => N::F(float(x).powf(float(y))),
        (Op::Add, _, _) => N::F(float(x) + float(y)),
        (Op::Sub, _, _) => N::F(float(x) - float(y)),
        (Op::Mul, _, _) => N::F(float(x) * float(y)),
    };
    to_value(r)
}

/// `timespan * number`, `number * timespan` and `timespan / number`. `None` when the operands are
/// not of that shape.
fn scale_timespan(op: Op, a: &Value, b: &Value) -> Option<Result<Value, RunError>> {
    let (ms, n) = match (op, a, b) {
        (Op::Mul, Value::Timespan(t), other) | (Op::Mul, other, Value::Timespan(t)) => {
            (*t, as_n(other)?)
        }
        (Op::Div, Value::Timespan(t), other) => (*t, as_n(other)?),
        _ => return None,
    };
    if op == Op::Div && float(n) == 0.0 {
        return Some(Err(RunError::new("division by zero")));
    }
    let f = match op {
        Op::Div => ms as f64 / float(n),
        _ => ms as f64 * float(n),
    };
    Some(Ok(Value::Timespan(f.round() as i64)))
}

const MAX_NESTING: usize = 48;

/// Reads expressions from text. `reg` gives the syntax elements, `host` the names it can look up.
pub struct Parser<'a> {
    reg: &'a Registry,
    host: &'a dyn Host,
    depth: Cell<usize>,
    /// Results of [`Parser::expr`] by (text, hint). Without it, every slot of every registered
    /// pattern re-parses the same sub-texts, which grows exponentially with the registry.
    memo: RefCell<HashMap<(String, String), Option<Expr>>>,
    /// Second pass only: read a bare word in an untyped slot as an item type (`x is air`).
    items_in_objects: Cell<bool>,
}

impl<'a> Parser<'a> {
    pub fn new(reg: &'a Registry, host: &'a dyn Host) -> Self {
        Parser {
            reg,
            host,
            depth: Cell::new(0),
            memo: RefCell::new(HashMap::new()),
            items_in_objects: Cell::new(false),
        }
    }

    /// Runs `f` with bare words in untyped slots read as item types. Statements try this only
    /// after every pattern failed without it, so other syntax keeps its own words.
    pub fn with_items_in_objects<R>(&self, f: impl FnOnce() -> R) -> R {
        self.items_in_objects.set(true);
        let r = f();
        self.items_in_objects.set(false);
        r
    }

    /// One expression of a class in `hint` (empty: any class).
    pub fn expr(&self, text: &str, hint: &[&str]) -> Option<Expr> {
        let t = text.trim();
        if t.is_empty() || self.depth.get() >= MAX_NESTING {
            return None;
        }
        let key = (
            t.to_owned(),
            format!("{}\u{2}{}", hint.join("\u{1}"), self.items_in_objects.get()),
        );
        if let Some(hit) = self.memo.borrow().get(&key) {
            return hit.clone();
        }
        self.depth.set(self.depth.get() + 1);
        let found = self.expr_inner(t, hint);
        self.depth.set(self.depth.get() - 1);
        let found = found.filter(|e| fits(e, hint));
        self.memo.borrow_mut().insert(key, found.clone());
        found
    }

    /// One expression, or several separated by `, ` / ` and ` / ` or ` (a plural slot).
    /// A list is read as a list first: an item type name would otherwise swallow `64 stone and
    /// 1 flint` as one id.
    pub fn list(&self, text: &str, hint: &[&str]) -> Option<Expr> {
        if let Some(parts) = split_list(text.trim()) {
            let items = parts
                .iter()
                .map(|p| self.expr(p, hint))
                .collect::<Option<Vec<Expr>>>();
            if let Some(items) = items {
                return Some(Expr::List(items));
            }
        }
        self.expr(text, hint)
    }

    fn expr_inner(&self, t: &str, hint: &[&str]) -> Option<Expr> {
        if let Some(parts) = self.quoted(t) {
            return Some(text_expr(parts));
        }
        if let Some(e) = self.binary(t) {
            return Some(e);
        }
        if t.starts_with('(') && matching_paren(t, 0) == Some(t.len() - 1) {
            return self.expr(&t[1..t.len() - 1], hint);
        }
        if let Some(v) = literal(t) {
            return Some(Expr::Lit(v));
        }
        if brace_wraps(t) {
            let inner = &t[1..t.len() - 1];
            if !inner.trim().is_empty() {
                let parts = self.interpolate(inner, false)?;
                return Some(Expr::Var(parts));
            }
        }
        if let Some(e) = self.registered(t, hint) {
            return Some(e);
        }
        if let Some(e) = self.call(t) {
            return Some(e);
        }
        self.bare(t, hint)
    }

    /// Registry expressions whose pattern covers the whole text. A slot never takes the whole
    /// text, which keeps `%object%` from recursing into itself.
    fn registered(&self, t: &str, hint: &[&str]) -> Option<Expr> {
        for entry in &self.reg.expressions {
            for pattern in &entry.patterns {
                let mut parse = |spec: &SlotSpec, text: &str| {
                    if text.len() >= t.len() {
                        return None;
                    }
                    self.slot(spec, text)
                };
                if let Some(m) = pattern.match_text(t, &mut parse) {
                    let e = Expr::Syntax {
                        handler: entry.handler.clone(),
                        m: Matched {
                            exprs: m.slots,
                            marks: m.marks,
                            tags: m.tags,
                            regex: m.regex,
                            variant: m.variant,
                        },
                        returns: entry.returns,
                    };
                    if fits(&e, hint) {
                        return Some(e);
                    }
                }
            }
        }
        None
    }

    /// A pattern slot: the classes of its type names decide how the text is read.
    pub(crate) fn slot(&self, spec: &SlotSpec, text: &str) -> Option<Expr> {
        let plural = spec
            .names
            .iter()
            .any(|n| classes::resolve(n).is_some_and(|t| t.plural));
        let classes: Vec<&str> = spec
            .names
            .iter()
            .map(|n| classes::resolve(n).map_or("object", |t| t.class))
            .collect();
        if plural {
            self.list(text, &classes)
        } else {
            self.expr(text, &classes)
        }
    }

    /// `"text"` as a whole: the parts between the quotes, or `None` when the text is not one
    /// quoted string.
    fn quoted(&self, t: &str) -> Option<Vec<Part>> {
        if !t.starts_with('"') {
            return None;
        }
        let end = closing_quote(t)?;
        if end != t.len() - 1 {
            return None;
        }
        self.interpolate(&t[1..end], true)
    }

    /// Text with `%expr%` parts, `%%` for a percent sign and `\x` for `x`. The colour tags and
    /// `&` codes of a literal are converted when `tags` is set (variable names are not).
    fn interpolate(&self, s: &str, tags: bool) -> Option<Vec<Part>> {
        let chars: Vec<char> = s.chars().collect();
        let mut parts = Vec::new();
        let mut buf = String::new();
        let mut i = 0;
        let flush = |buf: &mut String, parts: &mut Vec<Part>| {
            if !buf.is_empty() {
                let text = std::mem::take(buf);
                let text = if tags {
                    crate::value::format_chat(&text)
                } else {
                    text
                };
                parts.push(Part::Text(text));
            }
        };
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' && i + 1 < chars.len() {
                buf.push(chars[i + 1]);
                i += 2;
            } else if c == '%' && chars.get(i + 1) == Some(&'%') {
                buf.push('%');
                i += 2;
            } else if c == '%' {
                // The part ends at the next `%` outside braces, so `%{a::%{b}%}%` nests.
                let mut depth = 0i32;
                let close = (i + 1..chars.len()).find(|&j| match chars[j] {
                    '{' => {
                        depth += 1;
                        false
                    }
                    '}' => {
                        depth -= 1;
                        false
                    }
                    '%' => depth <= 0,
                    _ => false,
                })?;
                let inner: String = chars[i + 1..close].iter().collect();
                flush(&mut buf, &mut parts);
                parts.push(Part::Expr(self.expr(&inner, &[])?));
                i = close + 1;
            } else {
                buf.push(c);
                i += 1;
            }
        }
        flush(&mut buf, &mut parts);
        Some(parts)
    }

    fn binary(&self, t: &str) -> Option<Expr> {
        let mask = top_mask(t);
        // `a ? b` and `a otherwise b`: the first set value, lowest precedence of all operators.
        if let Some(i) = (0..t.len()).find(|&i| {
            mask[i]
                && t.is_char_boundary(i)
                && (t[i..].starts_with(" ? ") || t[i..].starts_with(" otherwise "))
        }) {
            let len = if t[i..].starts_with(" ? ") { 3 } else { 11 };
            let lhs = self.expr(&t[..i], &[])?;
            let rhs = self.expr(&t[i + len..], &[])?;
            return Some(Expr::Syntax {
                handler: Arc::new(crate::builtins::DefaultValue),
                m: Matched {
                    exprs: vec![Some(lhs), Some(rhs)],
                    ..Matched::default()
                },
                returns: "object",
            });
        }
        let groups: [(&[(&str, Op)], bool); 3] = [
            (&[(" + ", Op::Add), (" - ", Op::Sub)], true),
            (&[(" * ", Op::Mul), (" / ", Op::Div)], true),
            (&[(" ^ ", Op::Pow)], false),
        ];
        for (ops, last) in groups {
            let hits: Vec<(usize, usize, Op)> = (0..t.len())
                .filter(|&i| mask[i] && t.is_char_boundary(i))
                .filter_map(|i| {
                    ops.iter()
                        .find(|(s, _)| t[i..].starts_with(s))
                        .map(|(s, op)| (i, s.len(), *op))
                })
                .collect();
            let chosen = if last { hits.last() } else { hits.first() };
            if let Some(&(i, len, op)) = chosen {
                let lhs = self.expr(&t[..i], &[])?;
                let rhs = self.expr(&t[i + len..], &[])?;
                return Some(Expr::Arith(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        None
    }

    /// `name(args)`: a built-in function when the registry has one that takes these arguments,
    /// else a call of a script function, checked when it runs.
    fn call(&self, t: &str) -> Option<Expr> {
        let open = t.find('(')?;
        let name = &t[..open];
        if name.is_empty() || !t.ends_with(')') || !name.chars().all(is_ident_char) {
            return None;
        }
        if matching_paren(t, open)? != t.len() - 1 {
            return None;
        }
        let inner = t[open + 1..t.len() - 1].trim();
        let mut args = Vec::new();
        if !inner.is_empty() {
            for piece in split_top(inner, ',') {
                args.push(self.expr(piece, &[])?);
            }
        }
        let lower = name.to_ascii_lowercase();
        if let Some(f) = self.reg.function(&lower, args.len()) {
            return Some(Expr::Syntax {
                handler: Arc::new(crate::registry::FunctionCall(f.handler)),
                m: Matched {
                    exprs: args.into_iter().map(Some).collect(),
                    ..Matched::default()
                },
                returns: f.returns,
            });
        }
        Some(Expr::Call(lower, args))
    }

    /// A bare word where the slot asks for a player, item type, entity type or world.
    fn bare(&self, t: &str, hint: &[&str]) -> Option<Expr> {
        for h in hint {
            match *h {
                "player" if is_player_name(t) => return Some(Expr::PlayerName(t.to_owned())),
                "item type" | "entity type" | "world" => {
                    if let Some(v) = classes::parse_value(h, t, self.host) {
                        return Some(Expr::Lit(v));
                    }
                }
                // An untyped slot (`x is air`, `inventory contains 1 golden apple`): Skript reads
                // a bare word there as an item type when one has that name. Only words, so
                // keywords of other syntax (`is set`) are taken by their own pattern first.
                "object" if self.items_in_objects.get() && is_item_words(t) => {
                    if let Some(v) = classes::parse_value("item type", &item_alias(t), self.host) {
                        return Some(Expr::Lit(v));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// `[amount] word [word...]` of letters, digits and `_`: what an item type can look like.
fn is_item_words(t: &str) -> bool {
    !t.is_empty()
        && t.split(' ')
            .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// Skript's item aliases that name a different id: `potion of healing` is a `potion` (its
/// contents are not kept by the item type here).
fn item_alias(t: &str) -> String {
    let lower = t.to_ascii_lowercase();
    for kind in ["splash potion of ", "lingering potion of ", "potion of "] {
        if let Some(i) = lower.find(kind) {
            let amount = lower[..i].trim();
            let id = kind.trim_end_matches(" of ").replace(' ', "_");
            return if amount.is_empty() {
                id
            } else {
                format!("{amount} {id}")
            };
        }
    }
    t.to_owned()
}

fn text_expr(parts: Vec<Part>) -> Expr {
    if parts.iter().all(|p| matches!(p, Part::Text(_))) {
        let s: String = parts
            .into_iter()
            .map(|p| match p {
                Part::Text(s) => s,
                Part::Expr(_) => String::new(),
            })
            .collect();
        Expr::Lit(Value::Text(s))
    } else {
        Expr::Text(parts)
    }
}

/// Whether an expression of a known class fits one of the hinted classes.
pub fn fits(e: &Expr, hint: &[&str]) -> bool {
    if hint.is_empty() {
        return true;
    }
    match e.static_class() {
        None => true,
        Some(have) => hint.iter().any(|h| classes::accepts(h, have)),
    }
}

fn literal(t: &str) -> Option<Value> {
    if t.eq_ignore_ascii_case("true") {
        return Some(Value::Bool(true));
    }
    if t.eq_ignore_ascii_case("false") {
        return Some(Value::Bool(false));
    }
    if let Some(v) = classes::parse_number(t) {
        return Some(v);
    }
    if t.contains(char::is_alphabetic) {
        return classes::parse_timespan(t).map(Value::Timespan);
    }
    None
}

/// Whether `t` is one `{...}` whose opening brace closes at the last character.
fn brace_wraps(t: &str) -> bool {
    if !t.starts_with('{') || !t.ends_with('}') {
        return false;
    }
    let mut depth = 0i32;
    for (i, c) in t.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i == t.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_player_name(t: &str) -> bool {
    (1..=16).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The byte index of the quote that closes the string starting at 0.
fn closing_quote(t: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in t.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(i);
        }
    }
    None
}

/// The index of the `)` closing the `(` at `open`, outside quotes.
fn matching_paren(t: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote = false;
    let mut escaped = false;
    for (i, c) in t.char_indices().skip_while(|(i, _)| *i < open) {
        if quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quote = false;
            }
            continue;
        }
        match c {
            '"' => quote = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// For each byte of `t`: whether it is outside quotes and outside brackets.
fn top_mask(t: &str) -> Vec<bool> {
    let mut mask = vec![false; t.len()];
    let mut quote = false;
    let mut escaped = false;
    let mut depth = 0i32;
    for (i, c) in t.char_indices() {
        let top = !quote && depth == 0;
        for b in i..i + c.len_utf8() {
            mask[b] = top;
        }
        if quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quote = false;
            }
        } else {
            match c {
                '"' => quote = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
    }
    mask
}

/// Splits at `sep` where it is at the top level.
fn split_top(t: &str, sep: char) -> Vec<&str> {
    let mask = top_mask(t);
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in t.char_indices() {
        if c == sep && mask[i] {
            out.push(&t[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&t[start..]);
    out
}

/// Splits a list at its top-level `, `, ` and ` and ` or ` separators.
fn split_list(t: &str) -> Option<Vec<&str>> {
    let mask = top_mask(t);
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < t.len() {
        if mask[i] && t.is_char_boundary(i) {
            // `, or` / `, and` before `, `: Skript's lists end with `..., or "c"`.
            let sep = [", or ", ", and ", ", ", " and ", " or "]
                .into_iter()
                .find(|s| t[i..].starts_with(s));
            if let Some(s) = sep {
                out.push(&t[start..i]);
                i += s.len();
                start = i;
                continue;
            }
        }
        i += t[i..].chars().next().map_or(1, char::len_utf8);
    }
    out.push(&t[start..]);
    if out.len() < 2 || out.iter().any(|p| p.trim().is_empty()) {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arith_ok(op: Op, a: Value, b: Value) -> Value {
        arith(op, &a, &b).unwrap()
    }

    #[test]
    fn integer_and_decimal_arithmetic() {
        assert_eq!(
            arith_ok(Op::Add, Value::Int(2), Value::Int(3)),
            Value::Int(5)
        );
        assert_eq!(
            arith_ok(Op::Div, Value::Int(6), Value::Int(3)),
            Value::Int(2)
        );
        assert_eq!(
            arith_ok(Op::Div, Value::Int(7), Value::Int(2)),
            Value::Num(3.5)
        );
        assert_eq!(
            arith_ok(Op::Pow, Value::Int(2), Value::Int(10)),
            Value::Int(1024)
        );
        assert_eq!(
            arith_ok(Op::Mul, Value::Num(1.5), Value::Int(2)),
            Value::Num(3.0)
        );
        assert!(arith(Op::Div, &Value::Int(1), &Value::Int(0)).is_err());
    }

    #[test]
    fn plus_joins_text_and_adds_timespans() {
        assert_eq!(
            arith_ok(Op::Add, Value::Text("a".into()), Value::Int(1)),
            Value::Text("a1".into())
        );
        assert_eq!(
            arith_ok(Op::Add, Value::Text("2".into()), Value::Int(1)),
            Value::Int(3)
        );
        assert_eq!(
            arith_ok(Op::Add, Value::Timespan(1000), Value::Timespan(500)),
            Value::Timespan(1500)
        );
    }

    #[test]
    fn splits_respect_quotes_brackets_and_depth() {
        assert_eq!(
            split_top("a, (b, c), \"d, e\"", ','),
            vec!["a", " (b, c)", " \"d, e\""]
        );
        assert_eq!(split_list("1, 2 and 3"), Some(vec!["1", "2", "3"]));
        assert_eq!(split_list("\"a and b\""), None);
        assert_eq!(split_list("a, "), None);
        let m = top_mask("1 - (2 - 3)");
        assert!(m[2] && !m[7]);
    }

    #[test]
    fn literals_are_read() {
        assert_eq!(literal("12"), Some(Value::Int(12)));
        assert_eq!(literal("-1.5"), Some(Value::Num(-1.5)));
        assert_eq!(literal("5 seconds"), Some(Value::Timespan(5000)));
        assert_eq!(literal("TRUE"), Some(Value::Bool(true)));
        assert_eq!(literal("1.2.3"), None);
        assert_eq!(literal("word"), None);
    }

    #[test]
    fn quoted_text_closes_at_the_last_quote() {
        assert_eq!(closing_quote("\"a\\\"b\""), Some(5));
        assert_eq!(closing_quote("\"open"), None);
        assert_eq!(matching_paren("(a(b)c)", 0), Some(6));
        assert_eq!(matching_paren("f(\")\")", 1), Some(5));
    }
}
