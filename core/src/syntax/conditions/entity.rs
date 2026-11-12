//! Entity kinds: `<entity> is a player`. A player-type entity is a player value, NPCs included
//! (Citizens NPCs are player-type entities that the online list does not have). Patterns follow
//! Skript's `CondIsOfType`.

use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{negated, slot};
use crate::value::Value;

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    // Before the comparison `is`, which would read `a player` as a value.
    reg.add_condition(
        "is a player",
        &["%object% (is|are)(1¦ not|1¦n't|) [a[n]] player"],
        -1,
        is_player,
    )?;
    Ok(())
}

/// Holds for a player value. Other entities (and nothing) are not players.
fn is_player(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let is = matches!(slot(cx, m, 0)?, Value::Player { .. });
    Ok(is != negated(m))
}

#[cfg(test)]
mod tests {
    use crate::host::{EntityInfo, Recorder};
    use crate::value::Location;
    use crate::syntax::support::testing::run;

    fn host() -> Recorder {
        let mut h = Recorder::with_players(&[(1, "Alice")]);
        let at = Location {
            world: "stage".into(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
        };
        h.entities = vec![
            EntityInfo {
                uuid: 9,
                kind: "minecraft:player".into(),
                name: "Guide".into(),
                location: at.clone(),
            },
            EntityInfo {
                uuid: 10,
                kind: "minecraft:zombie".into(),
                name: "Zombie".into(),
                location: at,
            },
        ];
        h
    }

    #[test]
    fn players_and_npcs_are_players_zombies_are_not() {
        let h = host();
        let logs = run(
            &h,
            "\tloop all entities in world \"stage\":\n\t\tif loop-value is a player:\n\t\t\tbroadcast \"%loop-value%\"\n\t\tif loop-value is not a player:\n\t\t\tbroadcast \"other\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            *h.broadcasts.borrow(),
            vec!["Guide".to_owned(), "other".to_owned()]
        );
    }
}
