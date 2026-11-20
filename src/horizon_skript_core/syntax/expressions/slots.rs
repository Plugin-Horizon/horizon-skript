//! Inventory slots: `slot %number% of %player%`, `hotbar slot`, `tool` and `offhand tool` (the held
//! slots), `first empty slot`, `%slot%'s index` and `item in %slot%`. Ported from Skript's
//! `ExprInventorySlot`, `ExprHotbarSlot`, `ExprTool`, `ExprSlotIndex` and `ExprFirstEmptySlot`.
//!
//! Slots are numbered as the engine numbers them (0 to 35 the main inventory, 0 to 8 the hotbar,
//! 36 to 39 armour from feet to head, 40 the off hand, 41 and 42 the body and saddle). Differences
//! from Skript: a slot belongs to a player (`%player%`), not to an inventory; the item in a slot is
//! read when the expression runs, and an empty slot is air. Writing a slot is in effects/slots.rs.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::host::PlayerState;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{player_of, slot, slot_index, state_at};
use crate::horizon_skript_core::value::Value;

/// The highest slot number the engine has (the saddle).
const LAST_SLOT: u32 = 42;
/// The main inventory's slots: 0 to 35.
const MAIN_INVENTORY: u32 = 35;
/// The off hand's slot.
const OFF_HAND: u32 = 40;

/// The owner and number of a slot value.
fn parts(v: &Value) -> Result<(u128, u32), RunError> {
    match v {
        Value::Slot { owner, index } => Ok((*owner, *index)),
        other => Err(RunError::new(format!(
            "expected a slot, got {}",
            other.class()
        ))),
    }
}

/// A slot of the player in slot 1, with the number the value in slot 0 gives.
fn slot_of_player(cx: &mut Ctx<'_>, m: &Matched, max: u32) -> Result<Value, RunError> {
    let index = slot_index(&slot(cx, m, 0)?, max)?;
    let (owner, _) = player_of(&slot(cx, m, 1)?)?;
    Ok(Value::Slot { owner, index })
}

/// `slot %number% of %player%`.
fn numbered(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    slot_of_player(cx, m, LAST_SLOT)
}

/// `hotbar slot %number% of %player%`: the slots 0 to 8.
fn hotbar(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    slot_of_player(cx, m, 8)
}

/// `tool of %player%`: the hotbar slot the player holds.
fn tool(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, state) = state_at(cx, m, 0)?;
    Ok(Value::Slot {
        owner: uuid,
        index: state.selected_slot,
    })
}

/// `offhand tool of %player%`: the off hand's slot.
fn offhand(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, _) = state_at(cx, m, 0)?;
    Ok(Value::Slot {
        owner: uuid,
        index: OFF_HAND,
    })
}

/// `first empty slot of %player%`: the first main inventory slot without a stack, or nothing when
/// the main inventory is full (Skript's `ExprFirstEmptySlot` looks at the same 36 slots).
fn first_empty(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (uuid, state) = state_at(cx, m, 0)?;
    Ok(match first_free(&state) {
        Some(index) => Value::Slot { owner: uuid, index },
        None => Value::Null,
    })
}

/// The first main inventory slot that holds no stack.
fn first_free(state: &PlayerState) -> Option<u32> {
    (0..=MAIN_INVENTORY).find(|n| !state.inventory_slots.contains(n))
}

/// `index of %slot%` (also `%slot%'s index`).
fn index(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, index) = parts(&slot(cx, m, 0)?)?;
    Ok(Value::Int(i64::from(index)))
}

/// `item in %slot%` (also `%slot%'s item`): the stack in the slot, air when it is empty.
fn item(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (owner, index) = parts(&slot(cx, m, 0)?)?;
    let state = cx
        .host
        .player_state(owner)
        .ok_or_else(|| RunError::new("that player is not online"))?;
    let found = state
        .inventory_slots
        .iter()
        .position(|n| *n == index)
        .and_then(|at| state.inventory.get(at));
    Ok(match found {
        Some(stack) => Value::ItemType {
            id: stack.id.clone(),
            amount: stack.amount,
            name: stack.name.clone(),
        },
        None => Value::ItemType {
            id: "minecraft:air".to_owned(),
            amount: 1,
            name: None,
        },
    })
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "inventory slot",
        &["[the] slot %number% of [the] [inventory of] %player%"],
        0,
        "slot",
        numbered,
    )?;
    reg.add_expression(
        "hotbar slot",
        &["[the] hotbar slot %number% of [the] [inventory of] %player%"],
        0,
        "slot",
        hotbar,
    )?;
    reg.add_expression(
        "tool",
        &["[the] tool of %player%"],
        0,
        "slot",
        tool,
    )?;
    reg.add_expression(
        "offhand tool",
        &["[the] offhand tool of %player%"],
        0,
        "slot",
        offhand,
    )?;
    reg.add_expression(
        "first empty slot",
        &["[the] first empty slot [(in|of)] [the] [inventory of] %player%"],
        0,
        "slot",
        first_empty,
    )?;
    reg.add_expression(
        "slot index",
        &["[the] index of %slot%", "%slot%'[s] index"],
        0,
        "integer",
        index,
    )?;
    reg.add_expression(
        "item in slot",
        &["[the] item (in|of) %slot%", "%slot%'[s] item"],
        0,
        "item type",
        item,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::expr;

    #[test]
    fn slot_values_read_their_number_and_text() {
        let host = Recorder::with_players(&[(7, "Alice")]);
        assert_eq!(expr(&host, "slot 3 of Alice"), "slot 3");
        assert_eq!(expr(&host, "index of slot 36 of Alice"), "36");
        assert_eq!(expr(&host, "hotbar slot 8 of Alice"), "slot 8");
    }

    #[test]
    fn the_first_free_main_slot_skips_the_filled_ones() {
        let state = PlayerState {
            name: String::new(),
            display_name: String::new(),
            location: crate::horizon_skript_core::value::Location {
                world: "w".into(),
                x: 0.0,
                y: 0.0,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            health: 20.0,
            max_health: 20.0,
            food: 20,
            level: 0,
            gamemode: "survival".into(),
            flying: false,
            can_fly: false,
            sneaking: false,
            sprinting: false,
            swimming: false,
            op: false,
            held: "minecraft:air".into(),
            inventory: Vec::new(),
            inventory_slots: vec![0, 1, 2, 36],
            selected_slot: 4,
        };
        assert_eq!(first_free(&state), Some(3));
        let full = PlayerState {
            inventory_slots: (0..=35).collect(),
            ..state
        };
        assert_eq!(first_free(&full), None);
    }

    #[test]
    fn slot_numbers_must_exist() {
        assert_eq!(slot_index(&Value::Int(42), 42).ok(), Some(42));
        assert!(slot_index(&Value::Int(43), 42).is_err());
        assert!(slot_index(&Value::Num(1.5), 42).is_err());
        assert!(slot_index(&Value::Int(-1), 42).is_err());
    }
}
