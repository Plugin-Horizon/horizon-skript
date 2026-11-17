//! Other conditions: `chance of`.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{number_of, slot};

/// `chance of 25` and `chance of 25%`: true with that chance, out of 100.
fn chance(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let percent = match m.regex(0) {
        Some(text) => text
            .parse::<f64>()
            .map_err(|_| RunError::new(format!("can't read the chance {text}")))?,
        None => number_of(&slot(cx, m, 0)?)?,
    };
    Ok(cx.host.random_unit() * 100.0 < percent)
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_condition(
        "chance",
        &["chance of %number%", "chance of <[0-9.]+>\\%"],
        0,
        chance,
    )
}
