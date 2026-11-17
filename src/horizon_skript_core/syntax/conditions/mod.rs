//! Conditions: the tests of `if` and `while` (`is`, `contains`, `is online`, `chance of`, ...).
//! One module per group; see `core/src/syntax/README.md` for how an element is written.

pub mod comparison;
pub mod entity;
pub mod misc;
pub mod player;

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

/// Registers the conditions of every group.
pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    comparison::register(reg)?;
    player::register(reg)?;
    entity::register(reg)?;
    misc::register(reg)?;
    Ok(())
}
