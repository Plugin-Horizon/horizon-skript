//! Items: `give`, `remove` and `clear inventory`. Patterns follow Skript's `EffChange` (give,
//! remove) and its inventory changer. They run the vanilla `give`, `clear` as the console.
//!
//! `remove` is registered before the builtin `remove %object% from %object%` (priority -1). It
//! takes only the text of items from players; every other `remove` goes to the builtin, which
//! keeps the variable and list forms.

use super::{console, next, players_in, slot};
use crate::builtins;
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect("give", &["give %item types% to %players%"], 0, give)?;
    reg.add_effect(
        "remove items",
        &["remove %item types% from %players%"],
        -1,
        remove,
    )?;
    reg.add_effect(
        "clear inventory",
        &[
            "clear [the] inventory of %players%",
            "clear %players%'s inventory",
        ],
        -1,
        clear_inventory,
    )?;
    Ok(())
}

/// `give 3 stone to players`: one vanilla `give` per item and player.
fn give(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let items = item_stacks(slot(cx, m, 0)?)?;
    for (_, name) in players_in(cx, m, 1)? {
        for (id, amount) in &items {
            console(cx, &format!("give {name} {id} {amount}"));
        }
    }
    next()
}

/// `remove 3 stone from players`: a vanilla `clear` of that item for each player. Any other
/// `remove` is the builtin's: when the items are not item types or the target is not players,
/// the builtin runs with the same slots.
fn remove(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let items_value = slot(cx, m, 0)?;
    let target_value = slot(cx, m, 1)?;
    if !is_item_list(&items_value) || !is_player_list(&target_value) {
        return builtins::remove(cx, m);
    }
    let items = item_stacks(items_value)?;
    for (_, name) in players_in(cx, m, 1)? {
        for (id, amount) in &items {
            console(cx, &format!("clear {name} {id} {amount}"));
        }
    }
    next()
}

/// `clear the inventory of players`: vanilla `clear` with no item, which empties it.
fn clear_inventory(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for (_, name) in players_in(cx, m, 0)? {
        console(cx, &format!("clear {name}"));
    }
    next()
}

/// The (id, amount) of each item type in a value (one or a list).
fn item_stacks(v: Value) -> Result<Vec<(String, i64)>, RunError> {
    let mut out = Vec::new();
    for item in super::items_of(v) {
        match item {
            Value::ItemType { id, amount } => out.push((id, amount)),
            other => {
                return Err(RunError::new(format!(
                    "expected an item type, got {}",
                    other.class()
                )));
            }
        }
    }
    Ok(out)
}

/// Whether a value is one item type or a non-empty list of them.
fn is_item_list(v: &Value) -> bool {
    let items = match v {
        Value::List(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    !items.is_empty() && items.iter().all(|i| matches!(i, Value::ItemType { .. }))
}

/// Whether a value is one player or a non-empty list of them.
fn is_player_list(v: &Value) -> bool {
    let items = match v {
        Value::List(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    !items.is_empty() && items.iter().all(|i| matches!(i, Value::Player { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone() -> Value {
        Value::ItemType {
            id: "minecraft:stone".into(),
            amount: 3,
        }
    }

    #[test]
    fn only_item_types_and_players_are_taken() {
        assert!(is_item_list(&stone()));
        assert!(is_item_list(&Value::List(vec![stone(), stone()])));
        assert!(!is_item_list(&Value::Int(3)));
        assert!(!is_item_list(&Value::List(Vec::new())));
        let alice = Value::Player {
            uuid: 1,
            name: "Alice".into(),
        };
        assert!(is_player_list(&alice));
        assert!(!is_player_list(&Value::Text("Alice".into())));
    }

    #[test]
    fn item_stacks_keep_ids_and_amounts() {
        assert_eq!(
            item_stacks(stone()).unwrap(),
            vec![("minecraft:stone".to_owned(), 3)]
        );
        assert!(item_stacks(Value::Int(1)).is_err());
    }
}
