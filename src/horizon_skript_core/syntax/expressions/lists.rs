//! Lists, text and time: sorting and reversing, repeated and re-cased text, number ranges, the
//! first/last/nth elements of a list, code points, dates from timespans, random characters and
//! UUIDs, the parts of a colour, and `parsed as`. Ported from Skript's `ExprSortedList`,
//! `ExprAlphabetList`, `ExprReversedList`, `ExprRepeat`, `ExprNumbers`, `ExprElement`,
//! `ExprStringCase`, `ExprNumberOfCharacters`, `ExprCodepoint`, `ExprCharacterFromCodepoint`,
//! `ExprUnixDate`, `ExprUnixTicks`, `ExprDateAgoLater`, `ExprTimeSince`, `ExprRandomUUID`,
//! `ExprRandomCharacter`, `ExprARGB`, `ExprXOf`, `ExprAmountOfItems` and `ExprParse`.

use crate::horizon_skript_core::classes;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{
    colour_of, count_in, format_uuid, items, num, number_of, order, slot, state_of, text_of,
};
use crate::horizon_skript_core::value::{format_number, number_accuracy, Value};
use std::cmp::Ordering;

/// The most items a generated list may have (`numbers from 1 to 1000000000` stops here).
const MAX_ITEMS: usize = 1_000_000;

// ---------------------------------------------------------------- sorting

/// The order of two values for `sorted`: numbers, timespans and dates by value, other values of
/// one class by their text, case-insensitively. Values of different classes cannot be sorted.
fn compare(a: &Value, b: &Value) -> Result<Ordering, RunError> {
    match (a, b) {
        (Value::Int(_) | Value::Num(_), Value::Int(_) | Value::Num(_))
        | (Value::Timespan(_), Value::Timespan(_))
        | (Value::Date(_), Value::Date(_)) => order(a, b),
        _ if a.class() == b.class() => Ok(a
            .to_text()
            .to_lowercase()
            .cmp(&b.to_text().to_lowercase())),
        _ => Err(RunError::new(format!(
            "can't sort {} and {} together",
            a.class(),
            b.class()
        ))),
    }
}

/// `sorted %objects%`.
fn sorted(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut v = items(slot(cx, m, 0)?);
    let mut failed: Option<RunError> = None;
    v.sort_by(|a, b| match compare(a, b) {
        Ok(o) => o,
        Err(e) => {
            failed.get_or_insert(e);
            Ordering::Equal
        }
    });
    match failed {
        Some(e) => Err(e),
        None => Ok(Value::List(v)),
    }
}

/// `alphabetically sorted %texts%`: by text, whatever the class.
fn alphabetical(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut v = items(slot(cx, m, 0)?);
    v.sort_by_key(|x| x.to_text().to_lowercase());
    Ok(Value::List(v))
}

/// `reversed %objects%`.
fn reversed(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let mut v = items(slot(cx, m, 0)?);
    v.reverse();
    Ok(Value::List(v))
}

// ---------------------------------------------------------------- text

/// A single text stays single, several texts stay a list.
fn same_shape(input_was_list: bool, mut out: Vec<Value>) -> Value {
    if !input_was_list && out.len() == 1 {
        out.remove(0)
    } else {
        Value::List(out)
    }
}

/// Applies `f` to each text of slot 0.
fn map_texts(
    cx: &mut Ctx<'_>,
    m: &Matched,
    f: impl Fn(&str, u32) -> String,
    mark: u32,
) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    let was_list = matches!(v, Value::List(_));
    let out = items(v)
        .iter()
        .map(|t| Value::Text(f(&text_of(t), mark)))
        .collect();
    Ok(same_shape(was_list, out))
}

/// `%text% repeated %integer% times`.
fn repeated(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    let was_list = matches!(v, Value::List(_));
    let n = slot(cx, m, 1)?
        .as_i64()
        .ok_or_else(|| RunError::new("a repeat count is a whole number"))?;
    let count = n.max(0) as usize;
    let mut out = Vec::new();
    for t in items(v) {
        let s = text_of(&t);
        if s.len().saturating_mul(count) > MAX_ITEMS {
            return Err(RunError::new("that text would be too long"));
        }
        out.push(Value::Text(s.repeat(count)));
    }
    Ok(same_shape(was_list, out))
}

/// Java's `WordUtils.capitalize`: the first letter of every word upper case, the rest as it is.
/// With `fully` the rest is lower case.
fn capitalize_words(s: &str, fully: bool) -> String {
    let source = if fully { s.to_lowercase() } else { s.to_owned() };
    let mut out = String::with_capacity(source.len());
    let mut start = true;
    for c in source.chars() {
        if c.is_whitespace() {
            start = true;
            out.push(c);
        } else if start {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The first letter of a word lower case (`WordUtils.uncapitalize` on one word).
fn uncapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn camel_case(s: &str, strict: bool) -> String {
    let mut words = s.split(' ');
    let mut out = match words.next() {
        Some(w) if strict => w.to_lowercase(),
        Some(w) => uncapitalize(w),
        None => String::new(),
    };
    for w in words {
        out.push_str(&capitalize_words(w, strict));
    }
    out
}

fn pascal_case(s: &str, strict: bool) -> String {
    s.split(' ').map(|w| capitalize_words(w, strict)).collect()
}

/// Snake and kebab case: spaces become `sep`; `mark` 1 lower-cases the rest, any other mark
/// except 0 upper-cases it.
fn separated(s: &str, sep: char, mark: u32) -> String {
    s.chars()
        .flat_map(|c| -> Vec<char> {
            if c == ' ' {
                vec![sep]
            } else if mark == 0 {
                vec![c]
            } else if mark == 1 {
                c.to_lowercase().collect()
            } else {
                c.to_uppercase().collect()
            }
        })
        .collect()
}

macro_rules! case_expr {
    ($name:ident, |$s:ident, $mk:ident| $body:expr) => {
        fn $name(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
            let mark = (1..=4).find(|k| m.has_mark(*k)).unwrap_or(0);
            map_texts(cx, m, |$s: &str, $mk: u32| $body, mark)
        }
    };
}

// Mark 1 is upper case in the first two patterns of Skript (`0¦upper|1¦lower`); here upper is 2.
case_expr!(upper_lower, |s, mk| if mk == 2 { s.to_uppercase() } else { s.to_lowercase() });
case_expr!(capitalised, |s, _mk| s.to_uppercase());
case_expr!(proper, |s, mk| capitalize_words(s, mk == 2));
case_expr!(camel, |s, mk| camel_case(s, mk == 2));
case_expr!(pascal, |s, mk| pascal_case(s, mk == 2));
case_expr!(snake, |s, mk| separated(s, '_', mk));
case_expr!(kebab, |s, mk| separated(s, '-', mk));

/// `number of upper case characters in %text%` (mark 1), lower case (mark 2) and digits (mark 3).
fn count_chars(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let s = text_of(&slot(cx, m, 0)?);
    let n = s
        .chars()
        .filter(|c| match (m.has_mark(1), m.has_mark(2)) {
            (true, _) => c.is_uppercase(),
            (_, true) => c.is_lowercase(),
            _ => c.is_ascii_digit(),
        })
        .count();
    Ok(Value::Int(n as i64))
}

/// `[the] code point of %text%`: the code point of its first character.
fn codepoint(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let s = text_of(&slot(cx, m, 0)?);
    Ok(s.chars()
        .next()
        .map_or(Value::Null, |c| Value::Int(i64::from(u32::from(c)))))
}

/// `character at code point %integer%`.
fn char_from_codepoint(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let n = slot(cx, m, 0)?
        .as_i64()
        .ok_or_else(|| RunError::new("a code point is a whole number"))?;
    Ok(u32::try_from(n)
        .ok()
        .and_then(char::from_u32)
        .map_or(Value::Null, |c| Value::Text(c.to_string())))
}

/// `a random uuid`: a version 4 UUID from the host's random numbers.
fn random_uuid(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    let part = || (cx.host.random_unit() * u64::MAX as f64) as u64;
    let hi = u128::from(part());
    let lo = u128::from(part());
    let mut u = (hi << 64) | lo;
    u = (u & !(0xf_u128 << 76)) | (4_u128 << 76);
    u = (u & !(0x3_u128 << 62)) | (2_u128 << 62);
    Ok(Value::Text(format_uuid(u)))
}

/// `[an integer] random [alphanumeric] character[s] from %text% to %text%`: characters between
/// the first characters of the two texts (by code point), the alphanumeric ones with mark 1.
fn random_character(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let count = match m.expr(0) {
        Some(_) => slot(cx, m, 0)?
            .as_i64()
            .ok_or_else(|| RunError::new("a character count is a whole number"))?
            .max(0) as usize,
        None => 1,
    };
    let from = text_of(&slot(cx, m, 1)?).chars().next();
    let to = text_of(&slot(cx, m, 2)?).chars().next();
    let (Some(a), Some(b)) = (from, to) else {
        return Err(RunError::new("the bounds of a random character are characters"));
    };
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let pool: Vec<char> = (u32::from(lo)..=u32::from(hi))
        .filter_map(char::from_u32)
        .filter(|c| !m.has_mark(1) || c.is_alphanumeric())
        .collect();
    if pool.is_empty() || count > MAX_ITEMS {
        return Ok(Value::Null);
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let i = ((cx.host.random_unit() * pool.len() as f64) as usize).min(pool.len() - 1);
        out.push(Value::Text(pool[i].to_string()));
    }
    Ok(same_shape(m.expr(0).is_some(), out))
}

// ---------------------------------------------------------------- numbers

/// `numbers|integers|decimals from A to B`. `numbers` steps by one from A; `integers` are the
/// whole numbers in the range; `decimals` step by the precision of A (at most `number accuracy`).
/// A range given backwards comes out backwards.
fn numbers_between(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let a = number_of(&slot(cx, m, 0)?)?;
    let b = number_of(&slot(cx, m, 1)?)?;
    let reverse = a > b;
    let (s, f) = if reverse { (b, a) } else { (a, b) };
    let mut out: Vec<Value> = Vec::new();
    if m.has_mark(1) {
        let (lo, hi) = (s.ceil() as i64, f.floor() as i64);
        if hi >= lo && (hi - lo) as usize >= MAX_ITEMS {
            return Err(RunError::new("that range has too many numbers"));
        }
        out.extend((lo..=hi).map(Value::Int));
    } else if m.has_mark(2) {
        let shown = format_number(a);
        let places = shown.split_once('.').map_or(0, |(_, d)| d.len());
        let precision = places.min(number_accuracy() as usize) as i32;
        let mult = 10f64.powi(precision);
        let (lo, hi) = ((s * mult).ceil() as i64, (f * mult).floor() as i64);
        if hi >= lo && (hi - lo) as usize >= MAX_ITEMS {
            return Err(RunError::new("that range has too many numbers"));
        }
        out.extend((lo..=hi).map(|i| num(i as f64 / mult)));
    } else {
        let amount = (f - s + 1.0).floor();
        if amount >= MAX_ITEMS as f64 {
            return Err(RunError::new("that range has too many numbers"));
        }
        out.extend((0..amount.max(0.0) as i64).map(|i| num(s + i as f64)));
    }
    if reverse {
        out.reverse();
    }
    Ok(Value::List(out))
}

/// `%number% of %item type%`: the item type with that amount.
fn x_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let n = slot(cx, m, 0)?
        .as_i64()
        .ok_or_else(|| RunError::new("an amount is a whole number"))?;
    match slot(cx, m, 1)? {
        Value::ItemType { id, name, .. } => Ok(Value::ItemType {
            id,
            amount: n,
            name,
        }),
        other => Err(RunError::new(format!(
            "expected an item type, got {}",
            other.class()
        ))),
    }
}

/// `the amount of %item type% in %inventory%`: the items of that type in a player's inventory or
/// in a list of item stacks.
fn amount_of_items(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let id = match slot(cx, m, 0)? {
        Value::ItemType { id, .. } => id,
        other => {
            return Err(RunError::new(format!(
                "expected an item type, got {}",
                other.class()
            )))
        }
    };
    let holder = slot(cx, m, 1)?;
    if let Value::Player { .. } = holder {
        let (_, state) = state_of(cx, &holder)?;
        return Ok(Value::Int(count_in(&state, &id)));
    }
    let total: i64 = items(holder)
        .iter()
        .map(|v| match v {
            Value::ItemType {
                id: have, amount, ..
            } if *have == id => *amount,
            _ => 0,
        })
        .sum();
    Ok(Value::Int(total))
}

// ---------------------------------------------------------------- elements

/// The list in slot `n` as items.
fn list_at(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<Vec<Value>, RunError> {
    Ok(items(slot(cx, m, n)?))
}

fn whole(v: Value) -> Result<i64, RunError> {
    v.as_i64()
        .ok_or_else(|| RunError::new("an index is a whole number"))
}

/// `the first|last element of %objects%`.
fn first_last(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = list_at(cx, m, 0)?;
    let pick = if m.has_mark(1) { v.last() } else { v.first() };
    Ok(pick.cloned().unwrap_or(Value::Null))
}

/// `the first|last %integer% elements of %objects%`.
fn first_last_n(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let n = whole(slot(cx, m, 0)?)?.max(0) as usize;
    let v = list_at(cx, m, 1)?;
    let n = n.min(v.len());
    Ok(Value::List(if m.has_mark(1) {
        v[v.len() - n..].to_vec()
    } else {
        v[..n].to_vec()
    }))
}

/// `the 2nd element of %objects%` and `the 2nd to last element of ...` (mark 1).
fn nth(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let n = whole(slot(cx, m, 0)?)?;
    let v = list_at(cx, m, 1)?;
    if n < 1 || n as usize > v.len() {
        return Ok(Value::Null);
    }
    let i = if m.has_mark(1) {
        v.len() - n as usize
    } else {
        n as usize - 1
    };
    Ok(v[i].clone())
}

/// `the elements from A to B of %objects%` (1-based, both ends included).
fn range_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let a = whole(slot(cx, m, 0)?)?;
    let b = whole(slot(cx, m, 1)?)?;
    let v = list_at(cx, m, 2)?;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let lo = (lo.max(1) - 1) as usize;
    let hi = (hi.max(0) as usize).min(v.len());
    Ok(Value::List(if lo >= hi {
        Vec::new()
    } else {
        v[lo..hi].to_vec()
    }))
}

// ---------------------------------------------------------------- time

/// `%number% unix date`: a date from seconds since 1970.
fn unix_date(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    let was_list = matches!(v, Value::List(_));
    let mut out = Vec::new();
    for n in items(v) {
        out.push(Value::Date((number_of(&n)? * 1000.0) as i64));
    }
    Ok(same_shape(was_list, out))
}

/// `unix timestamp of %date%`: whole seconds since 1970.
fn unix_timestamp(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let v = slot(cx, m, 0)?;
    let was_list = matches!(v, Value::List(_));
    let mut out = Vec::new();
    for d in items(v) {
        match d {
            Value::Date(ms) => out.push(Value::Int(ms.div_euclid(1000))),
            other => {
                return Err(RunError::new(format!(
                    "expected a date, got {}",
                    other.class()
                )))
            }
        }
    }
    Ok(same_shape(was_list, out))
}

/// The timespan in slot 0 before (`back`) or after the date in slot 1, now when there is none.
fn shifted(cx: &mut Ctx<'_>, m: &Matched, back: bool) -> Result<Value, RunError> {
    let span = match slot(cx, m, 0)? {
        Value::Timespan(ms) => ms,
        other => {
            return Err(RunError::new(format!(
                "expected a timespan, got {}",
                other.class()
            )))
        }
    };
    let base = match m.expr(1) {
        Some(_) => match slot(cx, m, 1)? {
            Value::Date(ms) => ms,
            Value::Null => cx.host.now_millis(),
            other => {
                return Err(RunError::new(format!(
                    "expected a date, got {}",
                    other.class()
                )))
            }
        },
        None => cx.host.now_millis(),
    };
    Ok(Value::Date(if back { base - span } else { base + span }))
}

fn ago(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    shifted(cx, m, true)
}

fn later(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    shifted(cx, m, false)
}

/// `the time since %date%` (mark 1) and `the time until %date%`: zero when the date is on the
/// other side of now.
fn time_since(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let now = cx.host.now_millis();
    match slot(cx, m, 0)? {
        Value::Date(d) => {
            let span = if m.has_mark(1) { now - d } else { d - now };
            Ok(Value::Timespan(span.max(0)))
        }
        other => Err(RunError::new(format!(
            "expected a date, got {}",
            other.class()
        ))),
    }
}

// ---------------------------------------------------------------- colours, parsing

/// `red value of %colour%`: one channel of a colour (`rgb(r, g, b)`), alpha is always 255.
fn colour_part(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let channel = |c: [u8; 3]| -> i64 {
        match (1..=4).find(|k| m.has_mark(*k)) {
            Some(2) => i64::from(c[0]),
            Some(3) => i64::from(c[1]),
            Some(4) => i64::from(c[2]),
            _ => 255,
        }
    };
    let v = slot(cx, m, 0)?;
    if let Some(c) = colour_of(&v) {
        return Ok(Value::Int(channel(c)));
    }
    let mut out = Vec::new();
    for item in items(v) {
        let c = colour_of(&item)
            .ok_or_else(|| RunError::new(format!("expected a colour, got {}", item.class())))?;
        out.push(Value::Int(channel(c)));
    }
    Ok(Value::List(out))
}

/// `%text% parsed as <type>`: the text read as a value of the type, nothing when it is not one.
fn parsed_as(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let text = text_of(&slot(cx, m, 0)?);
    let name = m.regex(0).unwrap_or_default();
    let Some(class) = classes::resolve(name) else {
        return Err(RunError::new(format!("unknown type '{name}'")));
    };
    Ok(classes::parse_value(class.class, &text, cx.host).unwrap_or(Value::Null))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression("sorted", &["sorted %objects%"], 0, "object", sorted)?;
    reg.add_expression(
        "alphabetically sorted",
        &["alphabetically sorted %objects%"],
        0,
        "object",
        alphabetical,
    )?;
    reg.add_expression("reversed", &["reversed %objects%"], 0, "object", reversed)?;
    reg.add_expression(
        "repeated",
        &["%texts% repeated %integer% time[s]"],
        0,
        "object",
        repeated,
    )?;
    reg.add_expression(
        "numbers between",
        &["[(all [[of] the]|the)] (numbers|1¦integers|2¦decimals) (between|from) %number% (and|to) %number%"],
        0,
        "object",
        numbers_between,
    )?;
    reg.add_expression("random uuid", &["[a] random uuid"], 0, "text", random_uuid)?;
    reg.add_expression(
        "random character",
        &["[a|%-integer%] random [1¦alphanumeric] character[s] (from|between) %text% (to|and) %text%"],
        0,
        "object",
        random_character,
    )?;

    // Text case. Marks: 1 lower or lenient, 2 upper or strict; snake and kebab: 1 lower, 2 upper.
    reg.add_expression(
        "case change",
        &[
            "%texts% in (2¦upper|1¦lower)[ ]case",
            "(2¦upper|1¦lower)[ ]case %texts%",
        ],
        -1,
        "object",
        upper_lower,
    )?;
    reg.add_expression(
        "capitalized",
        &["capitali(s|z)ed %texts%"],
        0,
        "object",
        capitalised,
    )?;
    reg.add_expression(
        "proper case",
        &[
            "%texts% in [(1¦lenient|2¦strict) ](proper|title)[ ]case",
            "[(1¦lenient|2¦strict) ](proper|title)[ ]case %texts%",
        ],
        0,
        "object",
        proper,
    )?;
    reg.add_expression(
        "camel case",
        &[
            "%texts% in [(1¦lenient|2¦strict) ]camel[ ]case",
            "[(1¦lenient|2¦strict) ]camel[ ]case %texts%",
        ],
        0,
        "object",
        camel,
    )?;
    reg.add_expression(
        "pascal case",
        &[
            "%texts% in [(1¦lenient|2¦strict) ]pascal[ ]case",
            "[(1¦lenient|2¦strict) ]pascal[ ]case %texts%",
        ],
        0,
        "object",
        pascal,
    )?;
    reg.add_expression(
        "snake case",
        &[
            "%texts% in [(1¦lower|2¦upper|3¦capital|4¦screaming)[ ]]snake[ ]case",
            "[(1¦lower|2¦upper|3¦capital|4¦screaming)[ ]]snake[ ]case %texts%",
        ],
        0,
        "object",
        snake,
    )?;
    reg.add_expression(
        "kebab case",
        &[
            "%texts% in [(1¦lower|2¦upper|3¦capital)[ ]]kebab[ ]case",
            "[(1¦lower|2¦upper|3¦capital)[ ]]kebab[ ]case %texts%",
        ],
        0,
        "object",
        kebab,
    )?;
    reg.add_expression(
        "count characters",
        &[
            "number of (1¦upper|2¦lower)[ ]case char(acters|s) in %text%",
            "number of (3¦digit) char(acters|s) in %text%",
        ],
        -1,
        "integer",
        count_chars,
    )?;
    reg.add_expression(
        "code point",
        &[
            "[the] [unicode|character] code[ ]point of %text%",
            "[the] [unicode|character] code position of %text%",
            "%text%'[s] [unicode|character] code[ ]point",
        ],
        0,
        "integer",
        codepoint,
    )?;
    reg.add_expression(
        "character from code point",
        &["character (from|at|with) code([ ]point| position) %integer%"],
        0,
        "text",
        char_from_codepoint,
    )?;

    // Items.
    reg.add_expression("x of", &["%number% of %item type%"], 1, "item type", x_of)?;
    reg.add_expression(
        "amount of items",
        &["[the] (amount|number) of %item type% (in|of) %objects%"],
        -1,
        "integer",
        amount_of_items,
    )?;

    // Elements of a list.
    reg.add_expression(
        "first or last element",
        &["[the] (first|1¦last) element [out] (of|in) %objects%"],
        0,
        "object",
        first_last,
    )?;
    reg.add_expression(
        "first or last elements",
        &["[the] (first|1¦last) %integer% elements [out] (of|in) %objects%"],
        0,
        "object",
        first_last_n,
    )?;
    reg.add_expression(
        "nth element",
        &["[the] %integer%(st|nd|rd|th) (element|1¦last element|1¦to last element) [out] (of|in) %objects%"],
        0,
        "object",
        nth,
    )?;
    reg.add_expression(
        "range of elements",
        &["[the] elements (from|between) %integer% (to|and) %integer% [out] (of|in) %objects%"],
        0,
        "object",
        range_of,
    )?;

    // Time.
    reg.add_expression(
        "unix date",
        &[
            "[the] unix date of %numbers%",
            "%numbers%'[s] unix date",
        ],
        0,
        "date",
        unix_date,
    )?;
    reg.add_expression(
        "unix timestamp",
        &[
            "[the] unix timestamp of %dates%",
            "%dates%'[s] unix timestamp",
        ],
        0,
        "integer",
        unix_timestamp,
    )?;
    reg.add_expression(
        "ago",
        &["%timespan% (ago|in the past|before [the] [date] %-date%)"],
        0,
        "date",
        ago,
    )?;
    reg.add_expression(
        "later",
        &["%timespan% (later|(from|after) [the] [date] %-date%)"],
        0,
        "date",
        later,
    )?;
    reg.add_expression(
        "time since or until",
        &[
            "[the] time (1¦since) %date%",
            "[the] (time [remaining]|remaining time) until %date%",
        ],
        0,
        "timespan",
        time_since,
    )?;

    reg.add_expression(
        "colour part",
        &["(1¦alpha|2¦red|3¦green|4¦blue) (value|component) of %objects%"],
        0,
        "object",
        colour_part,
    )?;
    reg.add_expression(
        "parsed as",
        &["%text% parsed as [an|a] <([a-z]+( [a-z]+)*)>"],
        0,
        "object",
        parsed_as,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{expr, run};

    /// The items of a list expression, as the text of the list (`a, b`).
    fn list(h: &Recorder, e: &str) -> String {
        let logs = run(
            h,
            &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r::*}}%\"\n"),
        );
        assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
        let out = h.broadcasts.borrow().clone();
        assert_eq!(out.len(), 1, "no broadcast for `{e}`");
        out[0].clone()
    }

    fn fresh() -> Recorder {
        Recorder::default()
    }

    #[test]
    fn sorting_and_reversing() {
        assert_eq!(list(&fresh(), "sorted 3, 1 and 2"), "1, 2, 3");
        assert_eq!(list(&fresh(), "sorted \"b\", \"A\" and \"c\""), "A, b, c");
        assert_eq!(list(&fresh(), "reversed 1, 2 and 3"), "3, 2, 1");
        assert_eq!(
            list(&fresh(), "alphabetically sorted \"pear\", \"Apple\" and \"fig\""),
            "Apple, fig, pear"
        );
    }

    #[test]
    fn repeating_text() {
        assert_eq!(expr(&fresh(), "\"ab\" repeated 3 times"), "ababab");
        assert_eq!(expr(&fresh(), "\"ab\" repeated 0 times"), "");
    }

    #[test]
    fn number_ranges() {
        assert_eq!(list(&fresh(), "numbers from 1 to 5"), "1, 2, 3, 4, 5");
        assert_eq!(list(&fresh(), "integers from 1.5 to 4.5"), "2, 3, 4");
        assert_eq!(list(&fresh(), "numbers from 3 to 1"), "3, 2, 1");
        assert_eq!(list(&fresh(), "decimals from 0.5 to 1.5"), "0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.2, 1.3, 1.4, 1.5");
    }

    #[test]
    fn case_changes() {
        assert_eq!(expr(&fresh(), "\"Hello World\" in upper case"), "HELLO WORLD");
        assert_eq!(expr(&fresh(), "\"Hello World\" in lowercase"), "hello world");
        assert_eq!(
            expr(&fresh(), "\"hellO i'm steve!\" in proper case"),
            "HellO I'm Steve!"
        );
        assert_eq!(
            expr(&fresh(), "\"hellO i'm steve!\" in strict proper case"),
            "Hello I'm Steve!"
        );
        assert_eq!(
            expr(&fresh(), "\"spAwn neW boSs ()\" in camel case"),
            "spAwnNeWBoSs()"
        );
        assert_eq!(
            expr(&fresh(), "\"spAwn neW boSs ()\" in strict camel case"),
            "spawnNewBoss()"
        );
        assert_eq!(
            expr(&fresh(), "\"geneRate ranDom numBer ()\" in strict pascal case"),
            "GenerateRandomNumber()"
        );
        assert_eq!(expr(&fresh(), "\"Hello Player!\" in snake case"), "Hello_Player!");
        assert_eq!(
            expr(&fresh(), "\"Hello Player!\" in lower snake case"),
            "hello_player!"
        );
        assert_eq!(
            expr(&fresh(), "\"What is it?\" in upper kebab case"),
            "WHAT-IS-IT?"
        );
    }

    #[test]
    fn counting_characters_and_code_points() {
        assert_eq!(expr(&fresh(), "number of uppercase characters in \"aBcD1\""), "2");
        assert_eq!(expr(&fresh(), "number of lowercase characters in \"aBcD1\""), "2");
        assert_eq!(expr(&fresh(), "number of digit characters in \"aBcD12\""), "2");
        assert_eq!(expr(&fresh(), "code point of \"A\""), "65");
        assert_eq!(expr(&fresh(), "character at code point 66"), "B");
    }

    #[test]
    fn elements_of_a_list() {
        assert_eq!(expr(&fresh(), "the first element of 4, 5 and 6"), "4");
        assert_eq!(expr(&fresh(), "the last element of 4, 5 and 6"), "6");
        assert_eq!(list(&fresh(), "the first 2 elements of 4, 5 and 6"), "4, 5");
        assert_eq!(list(&fresh(), "the last 2 elements of 4, 5 and 6"), "5, 6");
        assert_eq!(expr(&fresh(), "the 2nd element of 4, 5 and 6"), "5");
        assert_eq!(expr(&fresh(), "the 2nd to last element of 4, 5 and 6"), "5");
        assert_eq!(expr(&fresh(), "the 1st last element of 4, 5 and 6"), "6");
        assert_eq!(list(&fresh(), "the elements from 2 to 3 of 4, 5, 6 and 7"), "5, 6");
    }

    #[test]
    fn times_and_dates() {
        let mut h = fresh();
        h.now = 1_000_000;
        assert_eq!(expr(&h, "unix timestamp of unix date of 946684800"), "946684800");
        let mut h = fresh();
        h.now = 90_000;
        assert_eq!(expr(&h, "unix timestamp of 1 minute ago"), "30");
        let mut h = fresh();
        h.now = 90_000;
        assert_eq!(expr(&h, "unix timestamp of 30 seconds later"), "120");
        let mut h = fresh();
        h.now = 100_000;
        assert_eq!(expr(&h, "the time since unix date of 40"), "1 minute");
        let mut h = fresh();
        h.now = 100_000;
        assert_eq!(expr(&h, "the time until unix date of 40"), "0 seconds");
    }

    #[test]
    fn random_uuid_has_the_version_and_variant_bits() {
        let mut h = fresh();
        h.rand = 0.5;
        let id = expr(&h, "a random uuid");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5, "{id}");
        assert!(parts[2].starts_with('4'), "{id}");
        assert!(parts[3].starts_with(['8', '9', 'a', 'b']), "{id}");
    }

    #[test]
    fn random_characters_stay_in_range() {
        let mut h = fresh();
        h.rand = 0.0;
        assert_eq!(expr(&h, "a random character from \"a\" to \"e\""), "a");
        let mut h = fresh();
        h.rand = 0.99;
        assert_eq!(expr(&h, "a random character from \"a\" to \"e\""), "e");
        let mut h = fresh();
        h.rand = 0.0;
        assert_eq!(expr(&h, "a random alphanumeric character from \"/\" to \"1\""), "0");
    }

    #[test]
    fn colours_item_amounts_and_parsing() {
        assert_eq!(expr(&fresh(), "red value of rgb(10, 20, 30)"), "10");
        assert_eq!(expr(&fresh(), "blue component of rgb(10, 20, 30)"), "30");
        assert_eq!(expr(&fresh(), "alpha value of rgb(10, 20, 30)"), "255");
        assert_eq!(expr(&fresh(), "3 of stone"), "3 stone");
        assert_eq!(expr(&fresh(), "\"42\" parsed as an integer"), "42");
        assert_eq!(expr(&fresh(), "\"4.5\" parsed as a number"), "4.5");
        assert_eq!(expr(&fresh(), "amount of stone in 3 stone and 4 stone"), "7");
    }
}
