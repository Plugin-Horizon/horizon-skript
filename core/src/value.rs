//! Runtime values and their text form.

use crate::classes;

/// A Skript value. `List` only exists as the result of a list expression (`{x::*}`, `1, 2 and 3`);
/// variables store single values, a list variable is a set of indexed variables.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Text(String),
    /// Whole numbers (`integer`).
    Int(i64),
    /// Any number (`number`); finite.
    Num(f64),
    Bool(bool),
    /// Milliseconds.
    Timespan(i64),
    /// Milliseconds since the Unix epoch, UTC.
    Date(i64),
    Player {
        uuid: u128,
        name: String,
    },
    World(String),
    Location(Location),
    /// Namespaced item id (`minecraft:stone`) and amount.
    ItemType {
        id: String,
        amount: i64,
    },
    /// Namespaced entity id (`minecraft:zombie`).
    EntityType(String),
    List(Vec<Value>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub world: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

impl Value {
    /// The class name this value belongs to, as written in patterns (`%number%`).
    pub fn class(&self) -> &'static str {
        match self {
            Value::Null | Value::List(_) => "object",
            Value::Text(_) => "text",
            Value::Int(_) => "integer",
            Value::Num(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Timespan(_) => "timespan",
            Value::Date(_) => "date",
            Value::Player { .. } => "player",
            Value::World(_) => "world",
            Value::Location(_) => "location",
            Value::ItemType { .. } => "item type",
            Value::EntityType(_) => "entity type",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The number this value is, if it is one (numbers, integers, and text that reads as one).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n),
            Value::Text(s) => classes::parse_number(s).map(|v| v.as_f64().unwrap_or(0.0)),
            _ => None,
        }
    }

    /// The whole number this value is, if it is one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
            Value::Text(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Booleans, and the texts `true` and `false`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Text(s) if s.eq_ignore_ascii_case("true") => Some(true),
            Value::Text(s) if s.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }

    /// How the value reads in a message: no `.0` on whole numbers, lists joined with `, `.
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => "<none>".to_owned(),
            Value::Text(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Num(n) => format_number(*n),
            Value::Bool(b) => b.to_string(),
            Value::Timespan(ms) => classes::format_timespan(*ms),
            Value::Date(ms) => classes::format_date(*ms),
            Value::Player { name, .. } => name.clone(),
            Value::World(w) => w.clone(),
            Value::Location(l) => format!(
                "{} at {}, {}, {}",
                l.world,
                format_number(round2(l.x)),
                format_number(round2(l.y)),
                format_number(round2(l.z))
            ),
            Value::ItemType { id, amount } => {
                let name = classes::short_id(id).replace('_', " ");
                if *amount == 1 {
                    name
                } else {
                    format!("{amount} {name}")
                }
            }
            Value::EntityType(id) => classes::short_id(id).replace('_', " "),
            Value::List(items) => items
                .iter()
                .map(Value::to_text)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A number as Skript prints it: `5` for 5.0, `2.5` for 2.5.
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1.0e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
