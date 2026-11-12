//! Block events: break, place and the clicks. Patterns from Skript's `EvtBlock` (break and
//! place) and `EvtClick` (the clicks, which Skript keeps in one event with a left or right mark;
//! here each kind has its own name, and `click` is the one for both).

use crate::pattern::PatternError;
use crate::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("break", &["[block] (break[ing]|min(e|ing))"])?;
    reg.add_event("place", &["[block] (plac(e|ing)|build[ing])"])?;
    reg.add_event("click", &["[mouse ]click[ing]"])?;
    reg.add_event(
        "left click",
        &["[mouse ]left click[ing]", "[mouse ]left-click[ing]"],
    )?;
    reg.add_event(
        "right click",
        &["[mouse ]right click[ing]", "[mouse ]right-click[ing]"],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Manager;
    use crate::host::Recorder;
    use crate::registry::Registry;
    use std::path::PathBuf;

    fn loads(header: &str) -> bool {
        let host = Recorder::default();
        let dir = PathBuf::new();
        let mut m = Manager::new(Registry::new(), dir);
        let src = format!("on {header}:\n\tbroadcast \"x\"\n");
        m.load_source(&host, "t.sk", &src, true).is_ok()
    }

    #[test]
    fn block_events_parse() {
        for header in [
            "break",
            "mine",
            "place",
            "build",
            "click",
            "left click",
            "right click",
            "right-click",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
    }
}
