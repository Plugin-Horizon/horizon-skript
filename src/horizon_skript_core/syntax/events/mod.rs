//! Events a trigger can start with (`on death:`). The core only knows their names and patterns;
//! the glue fires them with [`crate::horizon_skript_core::Manager::fire`] and fills the event's values.
//!
//! Patterns follow Skript's registrations (`EvtFirstJoin`, `SimpleEvents`, `EvtEntity`,
//! `EvtDamage`, `EvtBlock`, `EvtClick`, `EvtMove`, `EvtSkript`). Skript's filters (`on break of
//! stone`, `on death of zombie`) are not parsed yet: the patterns take no filter.
//!
//! The values an event carries, by the names the expressions read them with
//! (`cx.event.value("victim")`, see [`crate::horizon_skript_core::run::Event`]). Only the player and the message
//! are in the core's own fields; the rest are in `Event::values`:
//!
//! | Event | `player` | Other values |
//! |---|---|---|
//! | `join` / `quit` (builtins), `first join` | the player | |
//! | `respawn` | the player | `location`: where they respawn |
//! | `death` | the player who died | `victim` (player or entity type text), `attacker` (optional), `damage` (points) |
//! | `damage` | the player, when the victim is one | `victim`, `attacker` (optional), `damage` (points), `cause` (text) |
//! | `break`, `place` | the player | `block` (block id text), `location` (the block) |
//! | `click`, `left click`, `right click` | the player | `block` (id text, when a block), `location` (the block), `item` (item type, when held) |
//! | `move` | the player | `from`, `to` (locations) |
//! | `command` | the player | `message`: the command without its slash |
//! | `chat` (builtin) | the player | `message` |
//! | `server start`, `server stop` | | |
//!
//! `click` fires for both kinds of click; `left click` and `right click` only for their own, so
//! the glue fires the specific name and then `click`. Cancelling (`cancel event`) stops the
//! action where the engine lets the hook cancel it (see GAPS.md in the glue).

mod block;
mod craft;
mod misc;
mod player;
mod player_more;
mod world;

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

/// Registers every event of this group.
pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    player::register(reg)?;
    block::register(reg)?;
    misc::register(reg)?;
    world::register(reg)?;
    player_more::register(reg)?;
    craft::register(reg)?;
    reg.add_event("server start", &["(server|skript) (start|load|enable)"])?;
    reg.add_event("server stop", &["(server|skript) (stop|unload|disable)"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::registry::Registry;

    #[test]
    fn event_names_are_registered_with_skripts_patterns() {
        let reg = Registry::new();
        let (_, _, _, events) = reg.counts();
        // join, quit, chat, load, unload (builtins), the 13 of this group, the 38 of `misc`,
        // the 12 of `world`, the 11 of `player_more` and the 7 of `craft`.
        assert_eq!(events, 86);
    }
}
