//! Skript's pattern language, compiled once and matched by backtracking. The matcher knows
//! nothing about values: a slot (`%number%`) is handed to a parse callback with its type names,
//! and the callback says whether the text fills it and what it becomes.
//!
//! Syntax, as Skript writes it:
//! - literal text, matched case-insensitively (write the spaces the input has, e.g. `set `);
//! - `[optional part]`, `(alternative|alternative)`, and `|` at the top level;
//! - `%names%`: a slot. `%number/integer%` takes either type, `%-object%` may be absent, and
//!   `%~object%` is a single value (kept for patterns that Skript writes with it; the core does
//!   not treat it differently);
//! - `<regex>`: a regular expression (anchored at the current position); the first group, or the
//!   whole match when there is none, is kept in [`Match::regex`];
//! - parse marks: `1¦` inside an alternative sets mark 1 when that alternative matches, and
//!   `1:` does the same; `name:` (any non-number) is a named tag, see [`Match::tags`].
//!
//! Patterns with optional parts and alternatives are expanded into flat variants at compile
//! time, so matching walks one flat sequence per variant.

use regex::Regex;
use std::fmt;

/// The most variants one pattern may expand to (Skript patterns stay far below this).
const MAX_VARIANTS: usize = 4096;

/// One `%...%` slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotSpec {
    /// Position of the slot among the pattern's slots, left to right.
    pub index: usize,
    /// Type names as written (`number`, `numbers`, `item type`, `object`).
    pub names: Vec<String>,
    /// `%-type%`: the slot may be left out.
    pub nullable: bool,
    /// `%~type%`.
    pub single: bool,
}

#[derive(Clone, Debug)]
enum Flat {
    Lit(String),
    Slot(SlotSpec),
    Regex(Regex),
    Mark(u32),
    Tag(String),
}

#[derive(Clone, Debug)]
enum Raw {
    Lit(String),
    Slot(SlotSpec),
    Regex(Regex),
    Mark(u32),
    Tag(String),
    /// Present form, and the form when it is left out (the spaces around it, if any).
    Opt(Vec<Raw>, Vec<Raw>),
    Alt(Vec<Vec<Raw>>),
}

/// A compiled pattern.
#[derive(Clone, Debug)]
pub struct Pattern {
    source: String,
    variants: Vec<Vec<Flat>>,
    slots: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternError {
    pub pattern: String,
    pub message: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "malformed pattern \"{}\": {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for PatternError {}

/// A successful match: one entry per slot (`None` for an absent nullable slot), and what the
/// matched variant set.
#[derive(Clone, Debug)]
pub struct Match<T> {
    pub slots: Vec<Option<T>>,
    pub marks: Vec<u32>,
    pub tags: Vec<String>,
    pub regex: Vec<String>,
    /// Which variant matched (0 for a pattern without optional parts or alternatives).
    pub variant: usize,
}

impl Pattern {
    /// Compiles a pattern. Errors name the pattern and what is wrong with it.
    pub fn compile(source: &str) -> Result<Pattern, PatternError> {
        let err = |message: String| PatternError {
            pattern: source.to_owned(),
            message,
        };
        let mut p = Parser {
            chars: source.chars().collect(),
            pos: 0,
            slots: 0,
        };
        let alts = p.parse_alts(None).map_err(&err)?;
        let seq = if alts.len() == 1 {
            alts.into_iter().next().unwrap_or_default()
        } else {
            vec![Raw::Alt(alts)]
        };
        let variants =
            expand(&seq).ok_or_else(|| err(format!("more than {MAX_VARIANTS} variants")))?;
        Ok(Pattern {
            source: source.to_owned(),
            variants,
            slots: p.slots,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn slot_count(&self) -> usize {
        self.slots
    }

    /// Matches the whole input. `parse` is asked for each slot with the text it would fill:
    /// longer texts are tried first, so `set %object% to %object%` takes the longest first
    /// value that still leaves a ` to ` for the rest of the pattern.
    pub fn match_text<T: Clone>(
        &self,
        input: &str,
        parse: &mut dyn FnMut(&SlotSpec, &str) -> Option<T>,
    ) -> Option<Match<T>> {
        for (variant, seq) in self.variants.iter().enumerate() {
            let st = State {
                slots: vec![None; self.slots],
                marks: Vec::new(),
                tags: Vec::new(),
                regex: Vec::new(),
            };
            if let Some(st) = run(seq, 0, input, st, parse) {
                return Some(Match {
                    slots: st.slots,
                    marks: st.marks,
                    tags: st.tags,
                    regex: st.regex,
                    variant,
                });
            }
        }
        None
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    slots: usize,
}

impl Parser {
    fn fail<T>(&self, message: impl Into<String>) -> Result<T, String> {
        Err(message.into())
    }

    /// Alternatives up to `stop` (`)` or `]`, consumed) or the end of the pattern.
    fn parse_alts(&mut self, stop: Option<char>) -> Result<Vec<Vec<Raw>>, String> {
        let mut alts = Vec::new();
        let mut cur: Vec<Raw> = Vec::new();
        let mut buf = String::new();
        loop {
            let Some(&c) = self.chars.get(self.pos) else {
                if let Some(s) = stop {
                    return self.fail(format!("missing closing '{s}'"));
                }
                flush(&mut buf, &mut cur);
                alts.push(cur);
                return Ok(alts);
            };
            self.pos += 1;
            match c {
                '|' => {
                    flush(&mut buf, &mut cur);
                    alts.push(std::mem::take(&mut cur));
                }
                c if Some(c) == stop => {
                    flush(&mut buf, &mut cur);
                    alts.push(cur);
                    return Ok(alts);
                }
                ')' | ']' => return self.fail(format!("unexpected '{c}'")),
                '[' => {
                    // The spaces around an optional part belong to it: `return [%object%]` matches
                    // `return` and `return 5`; `[the] player` matches `the player` and `player`;
                    // `stop [1|a] loop` matches `stop loop` and `stop 1 loop`.
                    let before = buf.ends_with(' ');
                    if before {
                        buf.pop();
                    }
                    // A part glued to a word (`max[imum] health`, `permission[s] %texts%`) does not
                    // take the space after it: the absent form keeps that space. A part that starts
                    // a word (after a space, `(` or `|`, or at the start) may take it.
                    let idx = self.pos - 1;
                    let word_start =
                        idx == 0 || matches!(self.chars.get(idx - 1), Some(' ' | '(' | '|'));
                    flush(&mut buf, &mut cur);
                    let mut inner = self.parse_alts(Some(']'))?;
                    let after = self.chars.get(self.pos) == Some(&' ') && (before || word_start);
                    if after {
                        self.pos += 1;
                    }
                    let mut absent = Vec::new();
                    if before && after {
                        absent.push(Raw::Lit(" ".to_owned()));
                    }
                    for alt in &mut inner {
                        if before {
                            alt.insert(0, Raw::Lit(" ".to_owned()));
                        }
                        if after {
                            alt.push(Raw::Lit(" ".to_owned()));
                        }
                    }
                    cur.push(Raw::Opt(vec![Raw::Alt(inner)], absent));
                }
                '(' => {
                    flush(&mut buf, &mut cur);
                    let inner = self.parse_alts(Some(')'))?;
                    cur.push(Raw::Alt(inner));
                }
                '%' => {
                    let end = self.chars[self.pos..]
                        .iter()
                        .position(|&c| c == '%')
                        .ok_or_else(|| "single '%' without a closing '%'".to_owned())?;
                    let body: String = self.chars[self.pos..self.pos + end].iter().collect();
                    self.pos += end + 1;
                    let spec = self.slot(&body)?;
                    if spec.nullable && buf.ends_with(' ') {
                        // `stop %-object%` also matches `stop`: the space goes with the slot.
                        buf.pop();
                        flush(&mut buf, &mut cur);
                        let present = vec![Raw::Lit(" ".to_owned()), Raw::Slot(spec)];
                        cur.push(Raw::Opt(present, Vec::new()));
                    } else {
                        flush(&mut buf, &mut cur);
                        cur.push(Raw::Slot(spec));
                    }
                }
                '<' => {
                    flush(&mut buf, &mut cur);
                    let end = self.chars[self.pos..]
                        .iter()
                        .position(|&c| c == '>')
                        .ok_or_else(|| "missing closing '>' of a regex".to_owned())?;
                    let body: String = self.chars[self.pos..self.pos + end].iter().collect();
                    self.pos += end + 1;
                    let re = Regex::new(&format!("^(?:{body})"))
                        .map_err(|e| format!("invalid regex '{body}': {e}"))?;
                    cur.push(Raw::Regex(re));
                }
                '¦' | ':' => {
                    let tag = std::mem::take(&mut buf).trim().to_owned();
                    if let Ok(mark) = tag.parse::<u32>() {
                        cur.push(Raw::Mark(mark));
                    } else if c == '¦' {
                        return self.fail(format!("invalid parse mark '{tag}'"));
                    } else if tag.is_empty() {
                        return self.fail("empty tag before ':'");
                    } else {
                        cur.push(Raw::Tag(tag));
                    }
                }
                '\\' => {
                    if let Some(next) = self.chars.get(self.pos).copied() {
                        buf.push(next);
                        self.pos += 1;
                    }
                }
                other => buf.push(other),
            }
        }
    }

    fn slot(&mut self, body: &str) -> Result<SlotSpec, String> {
        let mut s = body.trim();
        let mut nullable = false;
        let mut single = false;
        loop {
            if let Some(rest) = s.strip_prefix('-') {
                nullable = true;
                s = rest;
            } else if let Some(rest) = s.strip_prefix('~') {
                single = true;
                s = rest;
            } else {
                break;
            }
        }
        let names: Vec<String> = s
            .split('/')
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            return self.fail("empty type in '%%'");
        }
        let index = self.slots;
        self.slots += 1;
        Ok(SlotSpec {
            index,
            names,
            nullable,
            single,
        })
    }
}

fn flush(buf: &mut String, cur: &mut Vec<Raw>) {
    if !buf.is_empty() {
        cur.push(Raw::Lit(std::mem::take(buf)));
    }
}

/// Every flat sequence the raw items can stand for; `None` past [`MAX_VARIANTS`].
fn expand(seq: &[Raw]) -> Option<Vec<Vec<Flat>>> {
    let mut acc: Vec<Vec<Flat>> = vec![Vec::new()];
    for item in seq {
        let options = expand_one(item)?;
        let mut next = Vec::with_capacity(acc.len() * options.len());
        for a in &acc {
            for o in &options {
                let mut v = a.clone();
                v.extend(o.iter().cloned());
                next.push(v);
            }
        }
        if next.len() > MAX_VARIANTS {
            return None;
        }
        acc = next;
    }
    Some(acc)
}

fn expand_one(item: &Raw) -> Option<Vec<Vec<Flat>>> {
    Some(match item {
        Raw::Lit(s) => vec![vec![Flat::Lit(s.clone())]],
        Raw::Slot(s) => vec![vec![Flat::Slot(s.clone())]],
        Raw::Regex(r) => vec![vec![Flat::Regex(r.clone())]],
        Raw::Mark(m) => vec![vec![Flat::Mark(*m)]],
        Raw::Tag(t) => vec![vec![Flat::Tag(t.clone())]],
        Raw::Opt(present, absent) => {
            // The present forms first, the absent one last (matches what Skript tries first).
            let mut v = expand(present)?;
            v.extend(expand(absent)?);
            v
        }
        Raw::Alt(alts) => {
            let mut v = Vec::new();
            for alt in alts {
                v.extend(expand(alt)?);
            }
            v
        }
    })
}

#[derive(Clone)]
struct State<T> {
    slots: Vec<Option<T>>,
    marks: Vec<u32>,
    tags: Vec<String>,
    regex: Vec<String>,
}

fn run<T: Clone>(
    seq: &[Flat],
    pos: usize,
    input: &str,
    mut st: State<T>,
    parse: &mut dyn FnMut(&SlotSpec, &str) -> Option<T>,
) -> Option<State<T>> {
    let Some((first, rest)) = seq.split_first() else {
        return (pos == input.len()).then_some(st);
    };
    match first {
        Flat::Lit(lit) => {
            let end = pos + lit.len();
            if input
                .get(pos..end)
                .is_some_and(|s| s.eq_ignore_ascii_case(lit))
            {
                run(rest, end, input, st, parse)
            } else {
                None
            }
        }
        Flat::Mark(m) => {
            st.marks.push(*m);
            run(rest, pos, input, st, parse)
        }
        Flat::Tag(t) => {
            st.tags.push(t.clone());
            run(rest, pos, input, st, parse)
        }
        Flat::Regex(re) => {
            let caps = re.captures(input.get(pos..)?)?;
            let whole = caps.get(0)?;
            let group = caps.get(1).unwrap_or(whole).as_str().to_owned();
            let end = pos + whole.end();
            st.regex.push(group);
            run(rest, end, input, st, parse)
        }
        Flat::Slot(spec) => {
            // Longest text first; a slot never starts or ends with a space.
            let mut ends: Vec<usize> = input[pos..]
                .char_indices()
                .map(|(i, c)| pos + i + c.len_utf8())
                .collect();
            ends.reverse();
            // Cheap checks before the (costly) parse of a candidate: the element after the slot
            // must be able to start at `end`, and a slot text is balanced. Without them every
            // end position of every slot is parsed, which made long `draw ... at ...` lines take
            // seconds to minutes.
            let after = rest
                .iter()
                .find(|f| !matches!(f, Flat::Mark(_) | Flat::Tag(_)));
            for end in ends {
                let text = &input[pos..end];
                if text.starts_with(' ') || text.ends_with(' ') {
                    continue;
                }
                match after {
                    None if end != input.len() => continue,
                    Some(Flat::Lit(lit))
                        if !input
                            .get(end..end + lit.len())
                            .is_some_and(|s| s.eq_ignore_ascii_case(lit)) =>
                    {
                        continue;
                    }
                    _ => {}
                }
                if !balanced(text) {
                    continue;
                }
                if let Some(value) = parse(spec, text) {
                    let mut next = st.clone();
                    next.slots[spec.index] = Some(value);
                    if let Some(done) = run(rest, end, input, next, parse) {
                        return Some(done);
                    }
                }
            }
            if spec.nullable {
                st.slots[spec.index] = None;
                return run(rest, pos, input, st, parse);
            }
            None
        }
    }
}

/// Whether brackets and braces pair up and quotes close in `text` (outside double-quoted
/// strings; `""` inside a string is an escaped quote). A slot never holds half of either.
fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut in_str = false;
    for c in text.chars() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && !in_str
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Accepts any non-empty text as a slot value.
    fn any(_: &SlotSpec, t: &str) -> Option<String> {
        (!t.is_empty()).then(|| t.to_owned())
    }

    fn m(pattern: &str, input: &str) -> Option<Match<String>> {
        Pattern::compile(pattern)
            .unwrap()
            .match_text(input, &mut any)
    }

    #[test]
    fn slot_texts_must_be_balanced_and_fit_before_the_next_literal() {
        assert!(balanced("location({_x}, 1, 2, world \"stage\")"));
        assert!(balanced("\"a ( b\""));
        assert!(!balanced("rgb(1, 2"));
        assert!(!balanced("a) + (b"));
        let m = m("draw %number% at %location%", "draw (1 + 2) at loc(1, (2))").unwrap();
        assert_eq!(m.slots, vec![Some("(1 + 2)".into()), Some("loc(1, (2))".into())]);
    }

    #[test]
    fn literals_are_case_insensitive_and_whole() {
        assert!(m("broadcast %object%", "BROADCAST hi").is_some());
        assert!(m("broadcast", "broadcast hi").is_none());
        assert!(m("broadcast %object%", "broadcast").is_none());
    }

    #[test]
    fn slot_takes_the_longest_value_that_leaves_the_rest() {
        let got = m("set %object% to %object%", "set a to b to c").unwrap();
        assert_eq!(got.slots[0].as_deref(), Some("a to b"));
        assert_eq!(got.slots[1].as_deref(), Some("c"));
    }

    #[test]
    fn optional_parts_and_absent_nullable_slots() {
        let p = Pattern::compile("return [%object%]").unwrap();
        assert_eq!(p.slot_count(), 1);
        let bare = p.match_text("return", &mut any).unwrap();
        assert_eq!(bare.slots, vec![None]);
        assert_eq!(bare.variant, 1);
        let with = p.match_text("return 5", &mut any).unwrap();
        assert_eq!(with.slots, vec![Some("5".to_owned())]);
        let nullable = Pattern::compile("stop %-object%").unwrap();
        assert_eq!(
            nullable.match_text("stop", &mut any).unwrap().slots,
            vec![None]
        );
    }

    #[test]
    fn parts_glued_to_a_word_keep_the_space_after_them() {
        // `max[imum] health`: the space belongs to the literal, so both forms match.
        assert!(m("[the] max[imum] health of %object%", "max health of x").is_some());
        assert!(m("[the] max[imum] health of %object%", "maximum health of x").is_some());
        assert!(m("permission[s] %texts%", "permissions a").is_some());
        assert!(m("permission[s] %texts%", "permission a").is_some());
        // Words in a row: each optional word takes its own space.
        let p = "[all] [the] [online] players";
        assert!(m(p, "online players").is_some());
        assert!(m(p, "all the online players").is_some());
        assert!(m(p, "all players").is_some());
    }

    #[test]
    fn alternatives_and_parse_marks() {
        let p = Pattern::compile("(1¦is|2¦are) %object%").unwrap();
        let a = p.match_text("is x", &mut any).unwrap();
        assert_eq!(a.marks, vec![1]);
        let b = p.match_text("ARE x", &mut any).unwrap();
        assert_eq!(b.marks, vec![2]);
        assert!(p.match_text("was x", &mut any).is_none());
    }

    #[test]
    fn numbered_tags_with_colon_are_marks_and_words_are_tags() {
        let p = Pattern::compile("(exit|stop) (1:loops|2:sections)").unwrap();
        let got = p.match_text("stop sections", &mut any).unwrap();
        assert_eq!(got.marks, vec![2]);
        let q = Pattern::compile("go (fast:now|slow)").unwrap();
        let got = q.match_text("go now", &mut any).unwrap();
        assert_eq!(got.tags, vec!["fast".to_owned()]);
    }

    #[test]
    fn regex_captures_its_group() {
        let p = Pattern::compile("(stop|exit) <([0-9]+)> loops").unwrap();
        let got = p.match_text("exit 3 loops", &mut any).unwrap();
        assert_eq!(got.regex, vec!["3".to_owned()]);
        assert!(p.match_text("exit x loops", &mut any).is_none());
        let plain = Pattern::compile("say <[a-z]+>!").unwrap();
        assert_eq!(
            plain.match_text("say hi!", &mut any).unwrap().regex,
            vec!["hi".to_owned()]
        );
    }

    #[test]
    fn slots_with_several_types_are_passed_on() {
        let p = Pattern::compile("add %number/integer% to %object%").unwrap();
        let mut seen = Vec::new();
        let mut parse = |spec: &SlotSpec, t: &str| {
            seen.push(spec.names.clone());
            Some(t.to_owned())
        };
        p.match_text("add 1 to x", &mut parse).unwrap();
        assert_eq!(seen[0], vec!["number".to_owned(), "integer".to_owned()]);
    }

    #[test]
    fn spaces_around_optional_parts_go_with_them() {
        let p = Pattern::compile("[the] player").unwrap();
        assert!(p.match_text("player", &mut any).is_some());
        assert!(p.match_text("the player", &mut any).is_some());
        assert!(p.match_text("the  player", &mut any).is_none());
        let q = Pattern::compile("stop [1|a] loop").unwrap();
        assert!(q.match_text("stop loop", &mut any).is_some());
        assert!(q.match_text("stop a loop", &mut any).is_some());
        assert!(q.match_text("stoploop", &mut any).is_none());
    }

    #[test]
    fn malformed_patterns_are_rejected() {
        assert!(Pattern::compile("set [a").is_err());
        assert!(Pattern::compile("set a)").is_err());
        assert!(Pattern::compile("set %object").is_err());
        assert!(Pattern::compile("<[a>").is_err());
        assert!(Pattern::compile("x¦y").is_err());
    }
}
