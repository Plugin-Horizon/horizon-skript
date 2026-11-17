//! Expressions: values read from the game and computed from others. One module per group; see
//! `core/src/syntax/README.md` for how an element is written.

pub mod entity;
pub mod event;
pub mod location;
pub mod misc;
pub mod player;
pub mod text;

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

/// Registers the expressions of every group.
pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    player::register(reg)?;
    location::register(reg)?;
    entity::register(reg)?;
    text::register(reg)?;
    misc::register(reg)?;
    event::register(reg)?;
    Ok(())
}
