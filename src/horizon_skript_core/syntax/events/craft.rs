//! Crafting, enchanting, furnace and brewing events. Patterns from Skript's `EvtCraft`,
//! `EvtEnchant`, `EvtFurnace` and `EvtBrewing`. The brewing and furnace events carry no player and
//! no dimension in the engine; they read the overworld, as the item spawn event does.

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("craft", &["[player] craft[ing]"])?;
    reg.add_event("prepare enchant", &["prepare enchant[ing]"])?;
    reg.add_event("enchant", &["[player] enchant[ing]"])?;
    reg.add_event("furnace smelt", &["[furnace] smelt[ing]"])?;
    reg.add_event("brewing complete", &["brewing complete[d]"])?;
    reg.add_event("brewing start", &["brewing start[ed]"])?;
    reg.add_event("brewing fuel", &["brewing fuel[ing]"])?;
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
    fn crafting_events_parse() {
        for header in [
            "craft",
            "prepare enchant",
            "enchant",
            "furnace smelt",
            "brewing complete",
            "brewing start",
            "brewing fuel",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
    }
}
