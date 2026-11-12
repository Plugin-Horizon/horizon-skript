//! Player events: first join, respawn, death, damage, move and command. Patterns from Skript's
//! `SimpleEvents` (respawn), `EvtFirstJoin`, `EvtEntity` (death), `EvtDamage`, `EvtMove`. Join,
//! quit and chat are builtins.

use crate::pattern::PatternError;
use crate::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("first join", &["first (join|login)"])?;
    reg.add_event("respawn", &["[player] respawn[ing]"])?;
    reg.add_event("death", &["death"])?;
    reg.add_event("damage", &["damag(e|ing)"])?;
    reg.add_event("move", &["[player] (move|walk|step)"])?;
    reg.add_event("command", &["[player ]command"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Manager;
    use crate::host::Recorder;
    use crate::registry::Registry;
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
    fn player_events_parse() {
        for header in [
            "first join",
            "respawn",
            "death",
            "damage",
            "move",
            "command",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
    }
}
