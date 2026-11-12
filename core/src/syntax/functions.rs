//! Skript's built-in functions: the math functions (`abs`, `round`, `floor`, `ceil`, `min`,
//! `max`, `sqrt`, the trigonometry in degrees, ...), `now()`, `date(...)`, `location(...)`,
//! `world(...)` and `concat(...)`. Ported from Skript's `DefaultFunctions`. Arguments arrive
//! already evaluated; a list argument (`{list::*}`) counts as its items where a function takes
//! several numbers.

use crate::classes;
use crate::error::RunError;
use crate::registry::Registry;
use crate::run::Ctx;
use crate::syntax::support::{items, num, number_of};
use crate::value::{Location, Value};

/// Every number among the arguments, lists flattened.
fn numbers(args: &[Value]) -> Result<Vec<f64>, RunError> {
    let mut out = Vec::new();
    for a in args {
        for v in items(a.clone()) {
            out.push(number_of(&v)?);
        }
    }
    Ok(out)
}

fn arg(args: &[Value], i: usize) -> Result<f64, RunError> {
    number_of(args.get(i).unwrap_or(&Value::Null))
}

/// Rounds half up, to `digits` decimals, as Skript's `round`.
fn round_to(x: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (x * f + 0.5).floor() / f
}

fn abs(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.abs()))
}

fn ceil(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(Value::Int(arg(&a, 0)?.ceil() as i64))
}

fn floor(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(Value::Int(arg(&a, 0)?.floor() as i64))
}

fn round(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let digits = if a.len() > 1 { arg(&a, 1)? as i32 } else { 0 };
    Ok(num(round_to(arg(&a, 0)?, digits)))
}

/// `mod(d, m)`: the remainder with the sign of `m` (`mod(-1, 10) = 9`).
fn modulo(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let (d, m) = (arg(&a, 0)?, arg(&a, 1)?);
    if m == 0.0 {
        return Err(RunError::new("can't divide by zero"));
    }
    Ok(num(((d % m) + m) % m))
}

fn exp(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.exp()))
}

fn ln(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.ln()))
}

/// `log(n)` is base 10; `log(n, base)` takes the base.
fn log(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let base = if a.len() > 1 { arg(&a, 1)? } else { 10.0 };
    Ok(num(arg(&a, 0)?.ln() / base.ln()))
}

fn sqrt(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.sqrt()))
}

fn sin(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.to_radians().sin()))
}

fn cos(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.to_radians().cos()))
}

fn tan(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.to_radians().tan()))
}

fn asin(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.asin().to_degrees()))
}

fn acos(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.acos().to_degrees()))
}

fn atan(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.atan().to_degrees()))
}

fn atan2(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(arg(&a, 0)?.atan2(arg(&a, 1)?).to_degrees()))
}

fn sum(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(numbers(&a)?.iter().sum()))
}

fn product(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    Ok(num(numbers(&a)?.iter().product()))
}

fn min(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    numbers(&a)?
        .into_iter()
        .reduce(f64::min)
        .map(num)
        .ok_or_else(|| RunError::new("min needs at least one number"))
}

fn max(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    numbers(&a)?
        .into_iter()
        .reduce(f64::max)
        .map(num)
        .ok_or_else(|| RunError::new("max needs at least one number"))
}

fn mean(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let n = numbers(&a)?;
    if n.is_empty() {
        return Err(RunError::new("mean needs at least one number"));
    }
    Ok(num(n.iter().sum::<f64>() / n.len() as f64))
}

/// `clamp(value, low, high)`.
fn clamp(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let (v, lo, hi) = (arg(&a, 0)?, arg(&a, 1)?, arg(&a, 2)?);
    if lo > hi {
        return Err(RunError::new("clamp needs low <= high"));
    }
    Ok(num(v.clamp(lo, hi)))
}

fn now(cx: &mut Ctx<'_>, _: Vec<Value>) -> Result<Value, RunError> {
    Ok(Value::Date(cx.host.now_millis()))
}

/// `date(year, month, day[, hour, minute, second])`, in UTC.
fn date(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let mut parts = [0i64; 6];
    for (i, p) in parts.iter_mut().enumerate().take(a.len()) {
        *p = arg(&a, i)? as i64;
    }
    let [y, mo, d, h, mi, s] = parts;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || !(0..24).contains(&h) {
        return Err(RunError::new("date has a month, day or hour out of range"));
    }
    let days = classes::days_from_civil(y, mo, d);
    Ok(Value::Date((days * 86_400 + h * 3600 + mi * 60 + s) * 1000))
}

/// `location(x, y, z[, world])`; the world defaults to `world`. Yaw and pitch are 0.
fn location(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let world = match a.get(3) {
        Some(Value::World(w)) => w.clone(),
        Some(Value::Text(w)) => w.clone(),
        Some(other) => {
            return Err(RunError::new(format!(
                "location needs a world, got {}",
                other.class()
            )));
        }
        None => "world".to_owned(),
    };
    Ok(Value::Location(Location {
        world,
        x: arg(&a, 0)?,
        y: arg(&a, 1)?,
        z: arg(&a, 2)?,
        yaw: 0.0,
        pitch: 0.0,
    }))
}

fn world(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    match a.first() {
        Some(Value::Text(w)) => Ok(Value::World(w.clone())),
        Some(Value::World(w)) => Ok(Value::World(w.clone())),
        _ => Err(RunError::new("world needs a name")),
    }
}

/// `concat(a, b, ...)`: the text forms joined without a delimiter.
fn concat(_: &mut Ctx<'_>, a: Vec<Value>) -> Result<Value, RunError> {
    let mut out = String::new();
    for v in a {
        for item in items(v) {
            out.push_str(&item.to_text());
        }
    }
    Ok(Value::Text(out))
}

pub fn register(reg: &mut Registry) {
    reg.add_function("abs", 1, 1, "number", abs);
    reg.add_function("ceil", 1, 1, "integer", ceil);
    reg.add_function("ceiling", 1, 1, "integer", ceil);
    reg.add_function("floor", 1, 1, "integer", floor);
    reg.add_function("round", 1, 2, "number", round);
    reg.add_function("mod", 2, 2, "number", modulo);
    reg.add_function("exp", 1, 1, "number", exp);
    reg.add_function("ln", 1, 1, "number", ln);
    reg.add_function("log", 1, 2, "number", log);
    reg.add_function("sqrt", 1, 1, "number", sqrt);
    reg.add_function("sin", 1, 1, "number", sin);
    reg.add_function("cos", 1, 1, "number", cos);
    reg.add_function("tan", 1, 1, "number", tan);
    reg.add_function("asin", 1, 1, "number", asin);
    reg.add_function("acos", 1, 1, "number", acos);
    reg.add_function("atan", 1, 1, "number", atan);
    reg.add_function("atan2", 2, 2, "number", atan2);
    reg.add_function("sum", 1, usize::MAX, "number", sum);
    reg.add_function("product", 1, usize::MAX, "number", product);
    reg.add_function("min", 1, usize::MAX, "number", min);
    reg.add_function("max", 1, usize::MAX, "number", max);
    reg.add_function("mean", 1, usize::MAX, "number", mean);
    reg.add_function("clamp", 3, 3, "number", clamp);
    reg.add_function("now", 0, 0, "date", now);
    reg.add_function("date", 3, 6, "date", date);
    reg.add_function("location", 3, 4, "location", location);
    reg.add_function("world", 1, 1, "world", world);
    reg.add_function("concat", 1, usize::MAX, "text", concat);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_up() {
        assert_eq!(round_to(2.5, 0), 3.0);
        assert_eq!(round_to(2.34, 1), 2.3);
        assert_eq!(round_to(-2.5, 0), -2.0);
    }

    #[test]
    fn date_matches_its_parts() {
        let mut host = crate::host::Recorder::default();
        host.now = 0;
        let v = {
            let mut reg = Registry::empty();
            register(&mut reg);
            let f = reg.function("date", 3).unwrap().handler;
            let mut vars = crate::vars::Variables::new();
            let mut locals = crate::vars::Variables::new();
            let event = crate::run::Event::default();
            let mut cx = Ctx {
                vars: &mut vars,
                locals: &mut locals,
                funcs: &Default::default(),
                host: &host,
                event: &event,
                depth: 0,
            };
            f(
                &mut cx,
                vec![Value::Int(1970), Value::Int(1), Value::Int(2)],
            )
            .unwrap()
        };
        assert_eq!(v, Value::Date(86_400_000));
    }
}
