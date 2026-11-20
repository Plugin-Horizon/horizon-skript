//! The general `is a <type>` condition: `5 is a number`, `{_x} is an integer`, `"a" is not a
//! location`, `{_list::*} are texts`. Ported from the general form of Skript's `CondIsOfType`
//! (the entity group keeps `is a player`).
//!
//! Differences from Skript: only the core classes, `entity` and `vector` are checked (text,
//! number, integer, boolean, timespan, date, world, location, item type, entity type, entity,
//! vector). A value the port keeps as text (an enum word, a UUID, a colour list) is not a number
//! or a location, and `is an object` is not registered.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{items, negated, slot};
use crate::horizon_skript_core::value::Value;

/// Whether one value is of the class `class` (a singular class name, as the pattern gives it).
fn is_class(v: &Value, class: &str) -> bool {
    match class {
        "text" => matches!(v, Value::Text(_)),
        "number" => matches!(v, Value::Int(_) | Value::Num(_)),
        "integer" => matches!(v, Value::Int(_)),
        "boolean" => matches!(v, Value::Bool(_)),
        "timespan" => matches!(v, Value::Timespan(_)),
        "date" => matches!(v, Value::Date(_)),
        "world" => matches!(v, Value::World(_)),
        "location" => matches!(v, Value::Location(_)),
        "item type" => matches!(v, Value::ItemType { .. }),
        "entity type" => matches!(v, Value::EntityType(_)),
        "entity" => matches!(v, Value::Player { .. } | Value::EntityType(_)),
        "vector" => matches!(v, Value::Vector { .. }),
        _ => false,
    }
}

/// Holds when every item of the first slot is of the class the pattern names. Nothing is no
/// item of any class, so an empty value is not of one.
fn of_class(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let class = m.regex(0).unwrap_or_default().to_ascii_lowercase();
    let values = items(slot(cx, m, 0)?);
    let holds = !values.is_empty() && values.iter().all(|v| is_class(v, &class));
    Ok(holds != negated(m))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    // Before the comparison `is`, which would read `a number` as a value. `player` is left to the
    // entity group (`is a player`), so it is not in the list.
    reg.add_condition(
        "is of class",
        &["%objects% (is|are)(1¦ not|1¦n't|) [a[n]] <(?i)entity type|item type|entity|text|number|integer|boolean|timespan|date|world|location|vector>"],
        -1,
        of_class,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::run;

    #[test]
    fn core_classes_of_literal_values() {
        let h = Recorder::with_players(&[(1, "Alice")]);
        let logs = run(
            &h,
            "\tif 5 is a number:\n\t\tbroadcast \"number\"\n\tif 5 is an integer:\n\t\tbroadcast \"integer\"\n\tif 5.5 is an integer:\n\t\tbroadcast \"wrong\"\n\tif \"5\" is a number:\n\t\tbroadcast \"wrong\"\n\tif \"5\" is a text:\n\t\tbroadcast \"text\"\n\tif \"5\" is not a number:\n\t\tbroadcast \"not number\"\n\tif 5 is a text:\n\t\tbroadcast \"wrong\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec![
                "number".to_owned(),
                "integer".to_owned(),
                "text".to_owned(),
                "not number".to_owned()
            ]
        );
    }
}
