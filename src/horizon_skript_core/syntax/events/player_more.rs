//! Player events beyond the first group: picking a block, using items, arrows, sleep, eggs,
//! elytra boosts, buckets and mobs, beacons, the experience cooldown, stonecutter recipes and
//! player input. Patterns from Skript's `SimpleEvents` (pick item, stop using item, ready arrow,
//! deep sleep, egg throw, elytra boost, bucket entity, beacon effect change, player input,
//! experience cooldown change, stonecutter recipe select).

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("pick item", &["[player] pick[ ]item"])?;
    reg.add_event("stop using item", &["[player] stop[(ped|ping)] using [an] item"])?;
    reg.add_event("ready arrow", &["[player] (ready|readying) [an] arrow"])?;
    reg.add_event("deep sleep", &["[player] deep sleep[ing]"])?;
    reg.add_event("egg throw", &["[player] egg throw[ing]"])?;
    reg.add_event("elytra boost", &["[player] elytra boost[ing]"])?;
    reg.add_event("bucket entity", &["[player] bucket entit(y|ies)"])?;
    reg.add_event("beacon effect change", &["[player] beacon effect chang(e|ing)"])?;
    reg.add_event("player input", &["[player] input"])?;
    reg.add_event(
        "experience cooldown change",
        &["[player] experience cooldown chang(e|ing)"],
    )?;
    reg.add_event(
        "stonecutter recipe select",
        &["[player] stonecutter recipe select[ing]"],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::Manager;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::registry::Registry;
    use std::path::PathBuf;

    /// A script with one trigger, loaded through the manager, parses when its event is known.
    fn loads(header: &str) -> bool {
        let host = Recorder::default();
        let dir = PathBuf::new();
        let mut m = Manager::new(Registry::new(), dir);
        let src = format!("on {header}:\n\tbroadcast \"x\"\n");
        m.load_source(&host, "t.sk", &src, true).is_ok()
    }

    #[test]
    fn more_player_events_parse() {
        for header in [
            "pick item",
            "stop using item",
            "ready arrow",
            "deep sleep",
            "egg throw",
            "elytra boost",
            "bucket entity",
            "beacon effect change",
            "player input",
            "experience cooldown change",
            "stonecutter recipe select",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
    }
}
