//! Turns the lines of a file into triggers, functions and statement trees. Statements are
//! matched against the registry first (effects, conditions, events); the structures Skript has
//! in every version (`if`, `else`, `loop`, `while`, `on`, `command`, `every`, `function`,
//! `options`) are read here.

use crate::classes;
use crate::error::{RunError, ScriptError};
use crate::expr::{Expr, Parser};
use crate::host::Host;
use crate::pattern::{Match, SlotSpec};
use crate::registry::{Condition, Effect, Matched, Registry};
use crate::run::Ctx;
use crate::script::{self, Item};
use crate::value::Value;
use std::sync::Arc;

pub type Body = Arc<Vec<Node>>;

/// A condition after `if` and `while`.
#[derive(Clone)]
pub enum Cond {
    Syntax {
        handler: Arc<dyn Condition>,
        m: Matched,
    },
    /// A boolean expression.
    Expr(Expr),
}

impl Cond {
    pub fn check(&self, cx: &mut Ctx<'_>) -> Result<bool, RunError> {
        match self {
            Cond::Syntax { handler, m } => handler.check(cx, m),
            Cond::Expr(e) => match e.eval(cx)? {
                // An unset variable is false, as in Skript.
                Value::Null => Ok(false),
                v => v
                    .as_bool()
                    .ok_or_else(|| RunError::new(format!("expected a boolean, got {}", v.class()))),
            },
        }
    }
}

#[derive(Clone)]
pub struct Branch {
    /// `None` for the `else` branch.
    pub cond: Option<Arc<Cond>>,
    pub body: Body,
}

#[derive(Clone)]
pub enum Node {
    Effect {
        handler: Arc<dyn Effect>,
        m: Matched,
        line: usize,
    },
    /// A statement that is an expression (a function call), its value dropped.
    Expr {
        e: Expr,
        line: usize,
    },
    If {
        branches: Vec<Branch>,
        line: usize,
    },
    /// `loop <list>:` (`times` false) or `loop <number> times:` (`times` true, the values 1 to n).
    Loop {
        iter: Expr,
        body: Body,
        times: bool,
        line: usize,
    },
    While {
        cond: Arc<Cond>,
        body: Body,
        line: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// `on <event>:`, with the event name the glue fires.
    Event(&'static str),
    /// `command /<name>:`, lower case without the slash.
    Command(String),
    /// `every <n> <unit>:`, in server ticks.
    Every(u64),
}

/// The lines of a command section besides its body: `permission:`, `permission message:`,
/// `usage:`, `aliases:` and `description:`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandInfo {
    pub permission: Option<String>,
    pub permission_message: Option<String>,
    pub usage: Option<String>,
    pub aliases: Vec<String>,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub body: Body,
    pub line: usize,
    /// Only command triggers have any of these.
    pub command: CommandInfo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub class: &'static str,
}

/// `function name(p: type) :: type:`. Its parameters are the local variables `{_p}`.
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub returns: Option<&'static str>,
    pub body: Body,
    pub file: Arc<str>,
    pub line: usize,
}

/// A parsed file.
pub struct Script {
    pub name: String,
    pub file: Arc<str>,
    pub triggers: Vec<Trigger>,
    pub functions: Vec<Arc<Function>>,
}

/// Parses a file's text. All problems are returned together, in line order.
pub fn parse_script(
    reg: &Registry,
    host: &dyn Host,
    name: &str,
    source: &str,
) -> Result<Script, Vec<ScriptError>> {
    let mut errors = Vec::new();
    let items = script::read(name, source, &mut errors);
    let mut p = Reader {
        reg,
        parser: Parser::new(reg, host),
        file: name.to_owned(),
        file_arc: Arc::from(name),
        errors: Vec::new(),
    };
    let mut triggers = Vec::new();
    let mut functions: Vec<Arc<Function>> = Vec::new();
    for item in &items {
        if !item.is_section() {
            p.error(
                item.line.number,
                "Can't understand this line here, expected a trigger or a function",
                &item.line.raw,
            );
            continue;
        }
        let header = item.header().to_owned();
        let line = item.line.number;
        if header.eq_ignore_ascii_case("options") {
            continue;
        }
        if let Some(event) = p.event(&header) {
            let body = p.statements(&item.children, line);
            triggers.push(Trigger {
                kind: TriggerKind::Event(event),
                body,
                line,
                command: CommandInfo::default(),
            });
        } else if let Some(rest) = strip_ci(&header, "command /") {
            let name = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if name.is_empty() {
                p.error(line, "a command needs a name", &item.line.raw);
                continue;
            }
            let (info, body_items) = p.command_section(&item.children);
            let body = p.statements(&body_items, line);
            triggers.push(Trigger {
                kind: TriggerKind::Command(name),
                body,
                line,
                command: info,
            });
        } else if let Some(rest) = strip_ci(&header, "every ") {
            match every_ticks(rest) {
                Some(ticks) => {
                    let body = p.statements(&item.children, line);
                    triggers.push(Trigger {
                        kind: TriggerKind::Every(ticks),
                        body,
                        line,
                        command: CommandInfo::default(),
                    });
                }
                None => p.error(line, "Can't understand this interval", &item.line.raw),
            }
        } else if let Some(rest) = strip_ci(&header, "function ") {
            match p.function(rest, item) {
                Some(f) => {
                    if functions.iter().any(|g| g.name == f.name) {
                        p.error(
                            line,
                            &format!("the function {} is defined twice in this file", f.name),
                            &item.line.raw,
                        );
                    } else {
                        functions.push(Arc::new(f));
                    }
                }
                None => {}
            }
        } else {
            p.error(line, "Can't understand this section", &item.line.raw);
        }
    }
    errors.extend(p.errors);
    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        return Err(errors);
    }
    // Function bodies were read with the other triggers; they are complete now.
    Ok(Script {
        name: name.to_owned(),
        file: Arc::from(name),
        triggers,
        functions,
    })
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// `5 seconds`, `1 tick`, `second`: the interval in ticks (at least one).
fn every_ticks(rest: &str) -> Option<u64> {
    let t = rest.trim();
    let ms = classes::parse_timespan(t).or_else(|| {
        let with_one = format!("1 {t}");
        classes::parse_timespan(&with_one)
    })?;
    Some(((ms as f64 / 50.0).round() as u64).max(1))
}

struct Reader<'a> {
    reg: &'a Registry,
    parser: Parser<'a>,
    file: String,
    file_arc: Arc<str>,
    errors: Vec<ScriptError>,
}

impl Reader<'_> {
    fn error(&mut self, line: usize, message: &str, text: &str) {
        self.errors.push(ScriptError {
            file: self.file.clone(),
            line,
            message: message.to_owned(),
            text: text.to_owned(),
        });
    }

    /// The event a trigger header names (`on join`), if one is registered.
    fn event(&self, header: &str) -> Option<&'static str> {
        let rest = strip_ci(header, "on ")?.trim();
        self.reg.events.iter().find_map(|entry| {
            entry
                .patterns
                .iter()
                .any(|p| {
                    p.match_text(rest, &mut |_: &SlotSpec, _: &str| None::<()>)
                        .is_some()
                })
                .then_some(entry.event)
        })
    }

    fn function(&mut self, rest: &str, item: &Item) -> Option<Function> {
        let line = item.line.number;
        let raw = item.line.raw.clone();
        let (sig, returns) = match rest.split_once("::") {
            Some((s, r)) => (s.trim(), Some(r.trim())),
            None => (rest.trim(), None),
        };
        let open = sig.find('(');
        let (name, params_text) = match open {
            Some(o) if sig.ends_with(')') => (&sig[..o], &sig[o + 1..sig.len() - 1]),
            _ => {
                self.error(line, "a function needs a name and parentheses", &raw);
                return None;
            }
        };
        let name = name.trim();
        if !is_name(name) {
            self.error(line, "invalid function name", &raw);
            return None;
        }
        let mut params = Vec::new();
        for piece in params_text
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let parsed = piece.split_once(':').and_then(|(n, t)| {
                let n = n.trim();
                let t = classes::resolve(t)?;
                (is_name(n) && !t.plural).then(|| Param {
                    name: n.to_owned(),
                    class: t.class,
                })
            });
            match parsed {
                Some(p) => params.push(p),
                None => {
                    self.error(line, "parameters look like `name: type`", &raw);
                    return None;
                }
            }
        }
        let returns = match returns {
            None => None,
            Some(r) => match classes::resolve(r) {
                Some(t) if !t.plural => Some(t.class),
                _ => {
                    self.error(line, "unknown return type", &raw);
                    return None;
                }
            },
        };
        let body = self.statements(&item.children, line);
        Some(Function {
            name: name.to_ascii_lowercase(),
            params,
            returns,
            body,
            file: self.file_arc.clone(),
            line,
        })
    }

    /// Reads the lines of a command section that are not its body, and returns the body's items:
    /// the `trigger:` section when there is one, else every item that is not a metadata line.
    fn command_section(&mut self, items: &[Item]) -> (CommandInfo, Vec<Item>) {
        let mut info = CommandInfo::default();
        self.command_meta(items, &mut info);
        if let Some(t) = items
            .iter()
            .find(|i| i.is_section() && i.header().eq_ignore_ascii_case("trigger"))
        {
            return (info, t.children.clone());
        }
        let body = items
            .iter()
            .filter(|i| i.is_section() || command_key(&i.line.text).is_none())
            .cloned()
            .collect();
        (info, body)
    }

    /// Fills `info` from the metadata lines of a command section; returns nothing else.
    fn command_meta(&mut self, items: &[Item], info: &mut CommandInfo) {
        for item in items.iter().filter(|i| !i.is_section()) {
            let Some((key, value)) = command_key(&item.line.text) else {
                continue;
            };
            let value = value.trim();
            match key {
                "permission" => info.permission = Some(value.to_owned()),
                "permission message" => {
                    info.permission_message = Some(crate::value::format_chat(value))
                }
                "usage" => info.usage = Some(value.to_owned()),
                "aliases" => {
                    info.aliases = value
                        .split(',')
                        .map(|a| a.trim().trim_start_matches('/').to_owned())
                        .filter(|a| !a.is_empty())
                        .collect();
                }
                "description" => info.description = Some(value.to_owned()),
                _ => {}
            }
        }
    }

    /// The statements of a section. Problems are recorded and the statement is left out.
    fn statements(&mut self, items: &[Item], section_line: usize) -> Body {
        if items.is_empty() {
            self.error(section_line, "this section has no lines", "");
        }
        let mut nodes: Vec<Node> = Vec::new();
        // The last node is an `if` chain that an `else` may continue.
        let mut open_if = false;
        for item in items {
            let line = item.line.number;
            let raw = item.line.raw.clone();
            if !item.is_section() {
                open_if = false;
                let text = item.line.text.clone();
                if text.eq_ignore_ascii_case("else") {
                    self.error(line, "'else' without a section", &raw);
                } else if let Some((handler, m)) = self.effect(&text) {
                    nodes.push(Node::Effect { handler, m, line });
                } else {
                    match self.parser.expr(&text, &[]) {
                        Some(e @ Expr::Call(..)) => nodes.push(Node::Expr { e, line }),
                        _ => self.error(line, "Can't understand this effect", &raw),
                    }
                }
                continue;
            }
            let header = item.header().to_owned();
            if let Some(rest) = header
                .strip_prefix("else")
                .filter(|r| r.is_empty() || r.starts_with(' '))
            {
                let cond = if rest.trim().is_empty() {
                    None
                } else {
                    match strip_ci(rest.trim_start(), "if ") {
                        Some(c) => match self.condition(c.trim()) {
                            Some(c) => Some(Arc::new(c)),
                            None => {
                                self.error(line, "Can't understand this condition", &raw);
                                open_if = false;
                                continue;
                            }
                        },
                        None => {
                            self.error(
                                line,
                                "'else' takes no condition here, did you mean 'else if'?",
                                &raw,
                            );
                            open_if = false;
                            continue;
                        }
                    }
                };
                let body = self.statements(&item.children, line);
                match nodes.last_mut() {
                    Some(Node::If { branches, .. }) if open_if => {
                        open_if = cond.is_some();
                        branches.push(Branch { cond, body });
                    }
                    _ => {
                        open_if = false;
                        self.error(line, "'else' without a matching 'if'", &raw);
                    }
                }
                continue;
            }
            open_if = false;
            if let Some(c) = strip_ci(&header, "if ") {
                match self.condition(c.trim()) {
                    Some(cond) => {
                        let body = self.statements(&item.children, line);
                        nodes.push(Node::If {
                            branches: vec![Branch {
                                cond: Some(Arc::new(cond)),
                                body,
                            }],
                            line,
                        });
                        open_if = true;
                    }
                    None => self.error(line, "Can't understand this condition", &raw),
                }
            } else if let Some(c) = strip_ci(&header, "while ") {
                match self.condition(c.trim()) {
                    Some(cond) => {
                        let body = self.statements(&item.children, line);
                        nodes.push(Node::While {
                            cond: Arc::new(cond),
                            body,
                            line,
                        });
                    }
                    None => self.error(line, "Can't understand this condition", &raw),
                }
            } else if let Some(e) = strip_ci(&header, "loop ") {
                let (source, times) = match e.trim().len().checked_sub(6) {
                    Some(cut) if e.trim()[cut..].eq_ignore_ascii_case(" times") => {
                        (e.trim()[..cut].trim(), true)
                    }
                    _ => (e.trim(), false),
                };
                let iter = if times {
                    self.parser.expr(source, &["number"])
                } else {
                    self.parser.list(source, &[])
                };
                match iter {
                    Some(iter) => {
                        let body = self.statements(&item.children, line);
                        nodes.push(Node::Loop {
                            iter,
                            body,
                            times,
                            line,
                        });
                    }
                    None => self.error(line, "Can't understand this loop", &raw),
                }
            } else {
                self.error(line, "Can't understand this section", &raw);
            }
        }
        Arc::new(nodes)
    }

    /// The effect a statement is, from the registry.
    fn effect(&self, text: &str) -> Option<(Arc<dyn Effect>, Matched)> {
        self.effect_pass(text)
            .or_else(|| self.parser.with_items_in_objects(|| self.effect_pass(text)))
    }

    fn effect_pass(&self, text: &str) -> Option<(Arc<dyn Effect>, Matched)> {
        for entry in &self.reg.effects {
            for pattern in &entry.patterns {
                let mut parse = |spec: &SlotSpec, slice: &str| {
                    if slice.len() >= text.len() {
                        return None;
                    }
                    self.parser.slot(spec, slice)
                };
                if let Some(m) = pattern.match_text(text, &mut parse) {
                    return Some((entry.handler.clone(), to_matched(m)));
                }
            }
        }
        None
    }

    /// A condition: a registered one, else a boolean expression.
    fn condition(&self, text: &str) -> Option<Cond> {
        self.condition_pass(text)
            .or_else(|| self.parser.with_items_in_objects(|| self.condition_pass(text)))
    }

    fn condition_pass(&self, text: &str) -> Option<Cond> {
        for entry in &self.reg.conditions {
            for pattern in &entry.patterns {
                let mut parse = |spec: &SlotSpec, slice: &str| {
                    if slice.len() >= text.len() {
                        return None;
                    }
                    self.parser.slot(spec, slice)
                };
                if let Some(m) = pattern.match_text(text, &mut parse) {
                    return Some(Cond::Syntax {
                        handler: entry.handler.clone(),
                        m: to_matched(m),
                    });
                }
            }
        }
        self.parser.expr(text, &["boolean"]).map(Cond::Expr)
    }
}

/// `permission: node` and the other metadata lines of a command: the key (lower case) and the
/// text after the colon.
fn command_key(text: &str) -> Option<(&'static str, &str)> {
    const KEYS: [&str; 5] = [
        "permission message",
        "permission",
        "usage",
        "aliases",
        "description",
    ];
    let (head, value) = text.split_once(':')?;
    let head = head.trim().to_ascii_lowercase();
    KEYS.iter().find(|k| **k == head).map(|k| (*k, value))
}

fn to_matched(m: Match<Expr>) -> Matched {
    Matched {
        exprs: m.slots,
        marks: m.marks,
        tags: m.tags,
        regex: m.regex,
        variant: m.variant,
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
