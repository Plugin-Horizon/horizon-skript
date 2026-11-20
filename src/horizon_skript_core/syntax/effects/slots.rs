//! Writing inventory slots: `set slot %number% of %players% to %item type%` and `clear slot
//! %number% of %players%`. Ported from Skript's `EffChange` on a slot; the write is the engine's
//! `Inventory.setItem` by slot number (`Actions::set_inventory_slot`). A custom name of the item is
//! not written. Reading slots is in expressions/slots.rs.

use super::{next, players_in, slot};
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::host::ItemStack;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::slot_index;
use crate::horizon_skript_core::value::Value;

/// The highest slot number the engine has (the saddle).
const LAST_SLOT: u32 = 42;

/// `set slot %number% of %players% to %item type%`: each player's slot gets the stack.
fn set_slot(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let index = slot_index(&slot(cx, m, 0)?, LAST_SLOT)?;
    let targets = players_in(cx, m, 1)?;
    let stack = match slot(cx, m, 2)? {
        Value::ItemType { id, amount, name } => ItemStack { id, amount, name },
        other => {
            return Err(RunError::new(format!(
                "expected an item type, got {}",
                other.class()
            )));
        }
    };
    for (uuid, name) in targets {
        if !cx.host.actions().set_inventory_slot(uuid, index, Some(&stack)) {
            return Err(RunError::new(format!(
                "the slot {index} of {name} could not be set"
            )));
        }
    }
    next()
}

/// `clear slot %number% of %players%`: each player's slot is emptied.
fn clear_slot(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let index = slot_index(&slot(cx, m, 0)?, LAST_SLOT)?;
    let targets = players_in(cx, m, 1)?;
    for (uuid, name) in targets {
        if !cx.host.actions().set_inventory_slot(uuid, index, None) {
            return Err(RunError::new(format!(
                "the slot {index} of {name} could not be cleared"
            )));
        }
    }
    next()
}

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "set slot",
        &["set slot %number% of %players% to %item type%"],
        -1,
        set_slot,
    )?;
    reg.add_effect(
        "clear slot",
        &["clear slot %number% of %players%"],
        -1,
        clear_slot,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    #[test]
    fn set_and_clear_write_the_slot_by_its_number() {
        let h = Recorder::with_players(&[(7, "Alice")]);
        let logs = run(
            &h,
            "\tset slot 4 of Alice to diamond\n\tclear slot 36 of Alice\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "slot 7 4 minecraft:diamond 1".to_owned(),
                "slot 7 36 clear".to_owned()
            ]
        );
    }
}
