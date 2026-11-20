//! Vectors: `vector x, y, z`, the `x`, `y` and `z` components, the length and squared length,
//! normalized vectors, dot and cross products, the angle between two vectors (degrees), the
//! projection of one onto another, the vector of a location and the vector between two locations,
//! and vectors from a yaw and a pitch. The `is normalized` condition is here too. Ported from
//! Skript's `ExprVectorFromXYZ`, `ExprXYZComponent`, `ExprVectorLength`, `ExprVectorSquaredLength`,
//! `ExprVectorNormalize`, `ExprVectorDotProduct`, `ExprVectorCrossProduct`, `ExprVectorAngleBetween`,
//! `ExprVectorProjection`, `ExprVectorOfLocation`, `ExprVectorBetweenLocations`,
//! `ExprVectorFromYawAndPitch` and `CondIsVectorNormalized`.
//!
//! Differences from Skript: the vector arithmetic (`+ - * /`) is in `expr.rs`. The bare `x of
//! %vector%` form is not registered (the location coordinate owns `x of`, and it does not read a
//! vector); `x component of` is. Not ported: random, spherical and cylindrical vectors, and the
//! vector of a direction (it needs the location direction).

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{location_at, negated, num, number_of, slot};
use crate::horizon_skript_core::value::Value;

/// The three components of a vector value. Anything else is an error that names its class.
fn parts(v: &Value) -> Result<(f64, f64, f64), RunError> {
    match v {
        Value::Vector { x, y, z } => Ok((*x, *y, *z)),
        other => Err(RunError::new(format!(
            "expected a vector, got {}",
            other.class()
        ))),
    }
}

/// The vector in the first slot, as components.
fn first(cx: &mut Ctx<'_>, m: &Matched) -> Result<(f64, f64, f64), RunError> {
    parts(&slot(cx, m, 0)?)
}

/// The vectors in the first and second slot, as components.
fn first_two(
    cx: &mut Ctx<'_>,
    m: &Matched,
) -> Result<((f64, f64, f64), (f64, f64, f64)), RunError> {
    let a = parts(&slot(cx, m, 0)?)?;
    let b = parts(&slot(cx, m, 1)?)?;
    Ok((a, b))
}

fn length_of((x, y, z): (f64, f64, f64)) -> f64 {
    (x * x + y * y + z * z).sqrt()
}

fn squared_length_of((x, y, z): (f64, f64, f64)) -> f64 {
    x * x + y * y + z * z
}

fn dot_of((ax, ay, az): (f64, f64, f64), (bx, by, bz): (f64, f64, f64)) -> f64 {
    ax * bx + ay * by + az * bz
}

/// `vector x, y, z` from three numbers.
fn vector_xyz(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let x = number_of(&slot(cx, m, 0)?)?;
    let y = number_of(&slot(cx, m, 1)?)?;
    let z = number_of(&slot(cx, m, 2)?)?;
    Ok(Value::Vector { x, y, z })
}

/// `x component of %vector%` (marks 1, 2, 3 for x, y, z).
fn component(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (x, y, z) = first(cx, m)?;
    let v = if m.has_mark(1) {
        x
    } else if m.has_mark(2) {
        y
    } else {
        z
    };
    Ok(num(v))
}

/// `(vector|standard|normal) length of %vector%`.
fn length(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(num(length_of(first(cx, m)?)))
}

/// `squared length of %vector%`.
fn squared_length(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(num(squared_length_of(first(cx, m)?)))
}

/// `normalized %vector%`: the vector divided by its length. A zero vector stays zero (Skript does
/// not normalize it), so there is no division by zero.
fn normalized(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (x, y, z) = first(cx, m)?;
    let len = length_of((x, y, z));
    if len == 0.0 {
        return Ok(Value::Vector { x, y, z });
    }
    Ok(Value::Vector {
        x: x / len,
        y: y / len,
        z: z / len,
    })
}

/// `%vector% dot %vector%`.
fn dot(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (a, b) = first_two(cx, m)?;
    Ok(num(dot_of(a, b)))
}

/// `%vector% cross %vector%`: the right-hand rule, as Bukkit's `crossProduct`.
fn cross(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let ((ax, ay, az), (bx, by, bz)) = first_two(cx, m)?;
    Ok(Value::Vector {
        x: ay * bz - az * by,
        y: az * bx - ax * bz,
        z: ax * by - ay * bx,
    })
}

/// `angle between %vector% and %vector%`, in degrees (Bukkit's angle is radians; Skript turns it).
fn angle(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (a, b) = first_two(cx, m)?;
    let cos = dot_of(a, b) / (length_of(a) * length_of(b));
    let radians = cos.clamp(-1.0, 1.0).acos();
    Ok(num(radians.to_degrees()))
}

/// `projection of %vector% on %vector%`: the second vector scaled by `dot / |second|^2`, as Skript.
fn projection(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (a, b) = first_two(cx, m)?;
    let scalar = dot_of(a, b) / squared_length_of(b);
    Ok(Value::Vector {
        x: b.0 * scalar,
        y: b.1 * scalar,
        z: b.2 * scalar,
    })
}

/// `vector of %location%`: the position as a vector (Bukkit's `Location.toVector`, no direction).
fn of_location(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let l = location_at(cx, m, 0)?;
    Ok(Value::Vector {
        x: l.x,
        y: l.y,
        z: l.z,
    })
}

/// `vector from %location% to %location%`: the second position minus the first.
fn between_locations(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let from = location_at(cx, m, 0)?;
    let to = location_at(cx, m, 1)?;
    Ok(Value::Vector {
        x: to.x - from.x,
        y: to.y - from.y,
        z: to.z - from.z,
    })
}

/// The unit vector of a yaw and a pitch, in degrees, with Skript's formula
/// (`ExprYawPitch.fromYawAndPitch`): y is the sine of the pitch, x and z are the yaw's cosine and
/// sine scaled by the pitch's cosine.
fn from_yaw_pitch(yaw: f64, pitch: f64) -> Value {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let scale = pitch.cos();
    Value::Vector {
        x: yaw.cos() * scale,
        y: pitch.sin(),
        z: yaw.sin() * scale,
    }
}

/// `vector from yaw %number% and pitch %number%`.
fn yaw_then_pitch(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let yaw = number_of(&slot(cx, m, 0)?)?;
    let pitch = number_of(&slot(cx, m, 1)?)?;
    Ok(from_yaw_pitch(yaw, pitch))
}

/// `vector from pitch %number% and yaw %number%`.
fn pitch_then_yaw(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let pitch = number_of(&slot(cx, m, 0)?)?;
    let yaw = number_of(&slot(cx, m, 1)?)?;
    Ok(from_yaw_pitch(yaw, pitch))
}

/// `%vector% is normalized`: the squared length is within 1e-6 of one (Bukkit's `isNormalized`).
fn is_normalized(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let v = first(cx, m)?;
    let holds = (squared_length_of(v) - 1.0).abs() < 1.0e-6;
    Ok(holds != negated(m))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "vector",
        &["[a] [new] vector [(from|at|to)] %number%, %number%(,| and) %number%"],
        0,
        "vector",
        vector_xyz,
    )?;
    reg.add_expression(
        "vector from yaw and pitch",
        &["[a] [new] vector (from|with) yaw %number% and pitch %number%"],
        0,
        "vector",
        yaw_then_pitch,
    )?;
    reg.add_expression(
        "vector from pitch and yaw",
        &["[a] [new] vector (from|with) pitch %number% and yaw %number%"],
        0,
        "vector",
        pitch_then_yaw,
    )?;
    reg.add_expression(
        "component of vector",
        &["(1¦x|2¦y|3¦z) component[s] of %vector%"],
        0,
        "number",
        component,
    )?;
    reg.add_expression(
        "vector length",
        &["[the] (vector|standard|normal) length[s] of %vector%"],
        0,
        "number",
        length,
    )?;
    reg.add_expression(
        "squared vector length",
        &["[the] squared length[s] of %vector%"],
        0,
        "number",
        squared_length,
    )?;
    reg.add_expression(
        "normalized vector",
        &["normali(z|s)e[d] %vector%", "%vector% normali(z|s)ed"],
        0,
        "vector",
        normalized,
    )?;
    reg.add_expression(
        "dot product",
        &["%vector% dot %vector%"],
        0,
        "number",
        dot,
    )?;
    reg.add_expression(
        "cross product",
        &["%vector% cross %vector%"],
        0,
        "vector",
        cross,
    )?;
    reg.add_expression(
        "angle between vectors",
        &["[the] angle between [[the] vectors] %vector% and %vector%"],
        0,
        "number",
        angle,
    )?;
    reg.add_expression(
        "vector projection",
        &["[vector] projection [of] %vector% on[to] %vector%"],
        0,
        "vector",
        projection,
    )?;
    reg.add_expression(
        "vector of location",
        &["[the] vector (of|from|to) %location%"],
        0,
        "vector",
        of_location,
    )?;
    reg.add_expression(
        "vector between locations",
        &["[the] vector (from|between) %location% (to|and) %location%"],
        0,
        "vector",
        between_locations,
    )?;
    reg.add_condition(
        "is normalized",
        &["%vector% (is|are)(1¦ not|1¦n't|) normali(z|s)ed"],
        0,
        is_normalized,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{expr, run};

    fn fresh() -> Recorder {
        Recorder::default()
    }

    #[test]
    fn vector_text_form_and_components() {
        assert_eq!(expr(&fresh(), "vector 1, 2, 3"), "x: 1, y: 2, z: 3");
        assert_eq!(expr(&fresh(), "x component of vector 1, 2, 3"), "1");
        assert_eq!(expr(&fresh(), "y component of vector 1, 2.5, 3"), "2.5");
        assert_eq!(expr(&fresh(), "z component of vector 1, 2, 3"), "3");
    }

    #[test]
    fn lengths() {
        assert_eq!(expr(&fresh(), "vector length of vector 3, 4, 0"), "5");
        assert_eq!(expr(&fresh(), "normal length of vector 3, 4, 0"), "5");
        assert_eq!(expr(&fresh(), "squared length of vector 3, 4, 0"), "25");
    }

    #[test]
    fn normalized_vectors() {
        assert_eq!(
            expr(&fresh(), "normalized vector 3, 4, 0"),
            "x: 0.6, y: 0.8, z: 0"
        );
        assert_eq!(expr(&fresh(), "normalized vector 0, 0, 0"), "x: 0, y: 0, z: 0");
    }

    #[test]
    fn dot_cross_and_angle() {
        assert_eq!(expr(&fresh(), "vector 1, 2, 3 dot vector 4, 5, 6"), "32");
        assert_eq!(
            expr(&fresh(), "vector 1, 0, 0 cross vector 0, 1, 0"),
            "x: 0, y: 0, z: 1"
        );
        assert_eq!(
            expr(&fresh(), "angle between vector 1, 0, 0 and vector 0, 1, 0"),
            "90"
        );
        assert_eq!(
            expr(&fresh(), "angle between vector 1, 0, 0 and vector 2, 0, 0"),
            "0"
        );
    }

    #[test]
    fn projection_onto_an_axis() {
        assert_eq!(
            expr(&fresh(), "vector projection of vector 1, 2, 3 onto vector 1, 0, 0"),
            "x: 1, y: 0, z: 0"
        );
    }

    #[test]
    fn vector_from_yaw_and_pitch() {
        assert_eq!(
            expr(&fresh(), "vector from yaw 0 and pitch 0"),
            "x: 1, y: 0, z: 0"
        );
        assert_eq!(
            expr(&fresh(), "vector from pitch 90 and yaw 0"),
            "x: 0, y: 1, z: 0"
        );
    }

    #[test]
    fn vector_of_locations() {
        assert_eq!(
            expr(&fresh(), "vector of location at 1, 2, 3"),
            "x: 1, y: 2, z: 3"
        );
        assert_eq!(
            expr(&fresh(), "vector from location at 1, 2, 3 to location at 4, 6, 3"),
            "x: 3, y: 4, z: 0"
        );
    }

    #[test]
    fn arithmetic_on_vectors() {
        assert_eq!(
            expr(&fresh(), "vector 1, 2, 3 + vector 1, 1, 1"),
            "x: 2, y: 3, z: 4"
        );
        assert_eq!(
            expr(&fresh(), "vector 1, 2, 3 - vector 1, 1, 1"),
            "x: 0, y: 1, z: 2"
        );
        assert_eq!(expr(&fresh(), "vector 1, 2, 3 * 2"), "x: 2, y: 4, z: 6");
        assert_eq!(expr(&fresh(), "2 * vector 1, 2, 3"), "x: 2, y: 4, z: 6");
        assert_eq!(expr(&fresh(), "vector 2, 4, 6 / 2"), "x: 1, y: 2, z: 3");
        assert_eq!(expr(&fresh(), "vector 2, 4, 6 * vector 1, 0.5, 2"), "x: 2, y: 2, z: 12");
    }

    #[test]
    fn normalized_condition() {
        let h = fresh();
        let logs = run(
            &h,
            "\tif vector 0.6, 0.8, 0 is normalized:\n\t\tbroadcast \"unit\"\n\tif vector 3, 4, 0 is normalized:\n\t\tbroadcast \"wrong\"\n\tif vector 3, 4, 0 is not normalized:\n\t\tbroadcast \"long\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["unit".to_owned(), "long".to_owned()]
        );
    }
}
