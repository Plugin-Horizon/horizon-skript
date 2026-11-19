//! More player, entity and world events. Patterns follow Skript's `SimpleEvents` and the `Evt*`
//! classes (`EvtGameMode`, `EvtWorldChange`, `EvtTeleport`, `EvtLevelChange`, `EvtWeatherChange`,
//! `EvtEntity`, ...). The values each event carries, by the names the expressions read them with:
//!
//! | Event | `player` | Other values |
//! |---|---|---|
//! | `teleport` | the player | `from`, `to` (locations) |
//! | `world change` | the player | `from`, `to` (worlds) |
//! | `game mode change` | the player | `from`, `gamemode` (text) |
//! | `flight toggle`, `sneak toggle`, `sprint toggle` | the player | `state` (boolean, the new state) |
//! | `jump`, `riptide`, `arm swing`, `item swap`, `drop`, `trade`, `fish` | the player | |
//! | `inventory open`, `inventory close` | the player | `location` (when a block holds it) |
//! | `item break` | the player | `item` (item type) |
//! | `hunger change` | the player | `from`, `to` (numbers) |
//! | `level change` | the player | `from`, `to` (numbers) |
//! | `experience change` | the player | `number` (the amount) |
//! | `kick` | the player | the event message is the reason |
//! | `bed enter`, `bed leave` | the player | `location` |
//! | `bucket empty` | the player | `location` |
//! | `item mend` | the player | `item`, `number` (repair) |
//! | `book edit`, `book sign` | the player | `number` (pages) |
//! | `pick up arrow`, `shoot` | the player | `item` (pick up arrow) |
//! | `weather change` | | `world`, `weather` (`clear`, `rain`, `thunder`) |
//! | `spawn` | | `entity` (entity type), `location` |
//! | `item spawn` | | `item` (item type), `number` (the amount), `location` |
//! | `lightning strike` | the player who caused it, if one did | |
//! | `tame` | the owner, if there is one | `entity` (entity type) |
//! | `breed`, `explosion prime` | | |
//! | `glide toggle` | the player | `state` |
//! | `mount`, `dismount` | | `entity`, `vehicle` (entity types) |

use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::Registry;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_event("teleport", &["[player] teleport[ing]"])?;
    reg.add_event("world change", &["[player] world chang(e|ing)"])?;
    reg.add_event("game mode change", &["[player] game[ ]mode chang(e|ing)"])?;
    reg.add_event(
        "flight toggle",
        &["[player] (toggl(e|ing) (flight|fly[ing])|flight (toggle|chang(e|ing)))"],
    )?;
    reg.add_event(
        "sneak toggle",
        &["[player] (toggl(e|ing) sneak[ing]|sneak (toggle|chang(e|ing)))"],
    )?;
    reg.add_event(
        "sprint toggle",
        &["[player] (toggl(e|ing) sprint[ing]|sprint (toggle|chang(e|ing)))"],
    )?;
    reg.add_event("jump", &["[player] jump[ing]"])?;
    reg.add_event("inventory open", &["inventory open[ed|ing]"])?;
    reg.add_event("inventory close", &["inventory clos(e|ed|ing)"])?;
    reg.add_event("item break", &["[player] (tool|item) break[ing]"])?;
    reg.add_event(
        "hunger change",
        &["[player] (hunger|food) [meter|level] chang(e|ing)"],
    )?;
    reg.add_event("level change", &["[player] level chang(e|ing)"])?;
    reg.add_event(
        "experience change",
        &["[player] (xp|exp|experience) chang(e|ing)"],
    )?;
    reg.add_event("kick", &["[player] kick[ed|ing]"])?;
    reg.add_event(
        "item swap",
        &["[player] hand item swap", "[player] swap[ping] hand[s]"],
    )?;
    reg.add_event("bed enter", &["[player] bed enter[ing]"])?;
    reg.add_event("bed leave", &["[player] bed leav(e|ing)"])?;
    reg.add_event("drop", &["[player] [item] drop[ping]"])?;
    reg.add_event("riptide", &["[player] riptid(e|ing)"])?;
    reg.add_event("arm swing", &["[player] arm swing[ing]"])?;
    reg.add_event("weather change", &["weather chang(e|ing)"])?;
    reg.add_event("spawn", &["[entity] spawn[ing]"])?;
    reg.add_event("item spawn", &["item spawn[ing]"])?;
    reg.add_event("lightning strike", &["lightning [strike]"])?;
    reg.add_event("bucket empty", &["[player] bucket empt(y|ying)"])?;
    reg.add_event("tame", &["[entity] tam(e|ing)"])?;
    reg.add_event("breed", &["[entity] breed[ing]"])?;
    reg.add_event("glide toggle", &["[entity] gliding state chang(e|ing)"])?;
    reg.add_event("fish", &["[player] fish[ing]"])?;
    reg.add_event("item mend", &["[player] item mend[ing]"])?;
    reg.add_event("mount", &["[entity] mount[ing]"])?;
    reg.add_event("dismount", &["[entity] dismount[ing]"])?;
    reg.add_event("book edit", &["[player] book edit[ing]"])?;
    reg.add_event("book sign", &["[player] book sign[ing]"])?;
    reg.add_event("pick up arrow", &["[player] pick[ ]up arrow"])?;
    reg.add_event("trade", &["[player] trad(e|ing)"])?;
    reg.add_event("shoot", &["[entity] shoot[ing]"])?;
    reg.add_event("explosion prime", &["explosion prime"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::Manager;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::registry::Registry;
    use crate::horizon_skript_core::run::Event;
    use crate::horizon_skript_core::value::Value;
    use std::path::PathBuf;

    fn loads(header: &str) -> bool {
        let host = Recorder::default();
        let mut m = Manager::new(Registry::new(), PathBuf::new());
        let src = format!("on {header}:\n\tbroadcast \"x\"\n");
        m.load_source(&host, "t.sk", &src, true).is_ok()
    }

    #[test]
    fn headers_parse() {
        for header in [
            "teleport",
            "player teleport",
            "world change",
            "gamemode change",
            "game mode change",
            "toggle flight",
            "flight toggle",
            "sneak toggle",
            "toggling sprint",
            "jump",
            "inventory open",
            "inventory close",
            "tool break",
            "item break",
            "hunger meter change",
            "food level change",
            "level change",
            "xp change",
            "kick",
            "hand item swap",
            "bed enter",
            "bed leave",
            "drop",
            "riptide",
            "arm swing",
            "weather change",
            "spawn",
            "entity spawn",
            "item spawn",
            "lightning strike",
            "lightning",
            "bucket empty",
            "tame",
            "breed",
            "gliding state change",
            "fish",
            "item mend",
            "mount",
            "dismount",
            "book edit",
            "book sign",
            "pick up arrow",
            "trade",
            "shoot",
            "explosion prime",
        ] {
            assert!(loads(header), "on {header}: does not load");
        }
        assert!(!loads("no such event"));
    }

    /// The glue fires an event by its name; the trigger reads the values the glue set.
    #[test]
    fn a_trigger_reads_the_values_of_its_event() {
        let host = Recorder::default();
        let mut m = Manager::new(Registry::new(), PathBuf::new());
        let src = "on world change:\n\tbroadcast \"%event-from% to %event-to%\"\n";
        m.load_source(&host, "t.sk", src, true).unwrap();
        let data = Event::default()
            .with("from", Value::World("world".into()))
            .with("to", Value::World("world_nether".into()));
        m.fire(&host, "world change", data);
        assert_eq!(*host.broadcasts.borrow(), vec!["world to world_nether".to_owned()]);
    }
}
