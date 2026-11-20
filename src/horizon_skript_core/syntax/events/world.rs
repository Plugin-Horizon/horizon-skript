//! World events: vehicles, portals, chunks, explosions, item despawn, anvils and projectile hits.
//! Patterns from Skript's `SimpleEvents` (vehicle create, destroy, damage and move, portal create
//! and enter, chunk load and unload, item despawn, projectile hit), `EvtExplode` and `EvtAnvil`
//! (anvil damage). The engine gives some of these without a dimension; those read the overworld,
//! as the item spawn event does.

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("vehicle create", &["vehicle create[d]"])?;
    reg.add_event("vehicle destroy", &["vehicle destroy[ed]"])?;
    reg.add_event("vehicle damage", &["vehicle damag(e|ing)"])?;
    reg.add_event("vehicle move", &["vehicle move[ment]"])?;
    reg.add_event("portal create", &["portal creat(e|ing)"])?;
    reg.add_event("portal enter", &["portal enter[ing]"])?;
    reg.add_event("chunk load", &["chunk load[ing]"])?;
    reg.add_event("chunk unload", &["chunk unload[ing]"])?;
    reg.add_event("explode", &["explo(de|sion)"])?;
    reg.add_event("item despawn", &["item despawn[ing]"])?;
    reg.add_event("anvil damage", &["anvil damag(e|ing)"])?;
    reg.add_event("projectile hit", &["projectile hit[ting]"])?;
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
    fn world_events_parse() {
        for header in [
            "vehicle create",
            "vehicle destroy",
            "vehicle damage",
            "vehicle move",
            "portal create",
            "portal enter",
            "chunk load",
            "chunk unload",
            "explode",
            "item despawn",
            "anvil damage",
            "projectile hit",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
    }
}
