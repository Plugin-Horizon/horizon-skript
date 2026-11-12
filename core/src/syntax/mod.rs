//! The syntax library: conditions and expressions beyond the builtins, the built-in functions
//! and the comparison rules they share. Read `README.md` in this folder before adding anything.
//!
//! Effects and events live in their own groups (`effects_*.rs`, `events_*.rs`), registered from
//! here as well.

pub mod conditions;
pub mod effects;
pub mod events;
pub mod expressions;
pub mod functions;
pub mod support;

use crate::registry::Registry;

/// Registers every syntax group. Each group is a module with its own `register` function; add
/// its call here.
pub fn register(reg: &mut Registry) {
    conditions::register(reg).expect("condition patterns compile");
    expressions::register(reg).expect("expression patterns compile");
    functions::register(reg);
    effects::register(reg).expect("effect patterns compile");
    events::register(reg).expect("event patterns compile");
}
