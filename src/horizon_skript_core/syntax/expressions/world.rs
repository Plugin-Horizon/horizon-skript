//! The world around a script: entities of a type, offsets from a location (`2 blocks above`),
//! locations from coordinates, and how many players are online. Ported from Skript's
//! `ExprEntities`, `ExprDirection`, `ExprLocationAt` and `ExprOnlinePlayersCount`.

use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{
    entity_value, items, location_at, location_of, number_of, slot, state_of, within,
};
use crate::horizon_skript_core::value::{Location, Value};

/// Whether an entity of `kind` is what the script named by `wanted`. Scripts write the plural
/// (`zombies`, `witches`, `endermen`), which the bare word keeps.
fn kind_matches(kind: &str, wanted: &str) -> bool {
    if kind == wanted || format!("{kind}s") == wanted || format!("{kind}es") == wanted {
        return true;
    }
    kind.strip_suffix("man")
        .is_some_and(|stem| format!("{stem}men") == wanted)
}

/// The entity ids a slot names (`zombies and skeletons`).
fn wanted_types(v: Value) -> Result<Vec<String>, RunError> {
    items(v)
        .into_iter()
        .map(|t| match t {
            Value::EntityType(id) => Ok(id),
            other => Err(RunError::new(format!(
                "expected an entity type, got {}",
                other.class()
            ))),
        })
        .collect()
}

/// The entities of the wanted types near a place: `near` is the centre and radius, or `None` for
/// the whole world. Players are the online players; other player-type entities (NPCs) are not
/// counted as players here, as in Skript.
fn entities_where(
    cx: &Ctx<'_>,
    wanted: &[String],
    world: &str,
    near: Option<(&Location, f64)>,
) -> Vec<Value> {
    let inside = |at: &Location| match near {
        Some((center, radius)) => within(center, at, radius),
        None => at.world.eq_ignore_ascii_case(world),
    };
    let mut out = Vec::new();
    if wanted.iter().any(|w| kind_matches("minecraft:player", w)) {
        for (uuid, name) in cx.host.online_players() {
            if let Some(state) = cx.host.player_state(uuid) {
                if inside(&state.location) {
                    out.push(Value::Player { uuid, name });
                }
            }
        }
    }
    for e in cx.host.entities_in_world(world) {
        if e.kind != "minecraft:player"
            && inside(&e.location)
            && wanted.iter().any(|w| kind_matches(&e.kind, w))
        {
            out.push(entity_value(&e));
        }
    }
    out
}

/// `all zombies in world "w"`.
fn entities_in_worlds(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let wanted = wanted_types(slot(cx, m, 0)?)?;
    let mut out = Vec::new();
    for w in items(slot(cx, m, 1)?) {
        let world = match w {
            Value::World(name) | Value::Text(name) => name,
            other => {
                return Err(RunError::new(format!(
                    "expected a world, got {}",
                    other.class()
                )))
            }
        };
        out.extend(entities_where(cx, &wanted, &world, None));
    }
    Ok(Value::List(out))
}

/// `all zombies within 10 blocks of %location%`.
fn entities_near(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let wanted = wanted_types(slot(cx, m, 0)?)?;
    let radius = number_of(&slot(cx, m, 1)?)?;
    let center = location_at(cx, m, 2)?;
    let world = center.world.clone();
    Ok(Value::List(entities_where(
        cx,
        &wanted,
        &world,
        Some((&center, radius)),
    )))
}

/// `the player count`: how many players are online.
fn player_count(cx: &mut Ctx<'_>, _: &Matched) -> Result<Value, RunError> {
    Ok(Value::Int(cx.host.online_players().len() as i64))
}

/// `2 blocks above %location%`, `north of %location%`: the location moved. North is -z, south
/// +z, east +x, west -x. Marks: 1 above, 2 below, 3 north, 4 south, 5 east, 6 west.
fn offset(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let by = match m.expr(0) {
        Some(_) => number_of(&slot(cx, m, 0)?)?,
        None => 1.0,
    };
    let v = slot(cx, m, 1)?;
    let mut at = location_of(cx, &v)?;
    match (1..=6).find(|k| m.has_mark(*k)) {
        Some(1) => at.y += by,
        Some(2) => at.y -= by,
        Some(3) => at.z -= by,
        Some(4) => at.z += by,
        Some(5) => at.x += by,
        Some(6) => at.x -= by,
        _ => {}
    }
    Ok(Value::Location(at))
}

/// `location at 1, 2, 3 in world "w"`: without a world, the world of the event's player.
fn location_from_coordinates(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let x = number_of(&slot(cx, m, 0)?)?;
    let y = number_of(&slot(cx, m, 1)?)?;
    let z = number_of(&slot(cx, m, 2)?)?;
    let world = match m.expr(3) {
        Some(_) => match slot(cx, m, 3)? {
            Value::World(name) | Value::Text(name) => name,
            other => {
                return Err(RunError::new(format!(
                    "expected a world, got {}",
                    other.class()
                )))
            }
        },
        None => match cx.event.player.clone() {
            Some(p) => state_of(cx, &p)?.1.location.world,
            None => return Err(RunError::new("this location needs a world")),
        },
    };
    Ok(Value::Location(Location {
        world,
        x,
        y,
        z,
        yaw: 0.0,
        pitch: 0.0,
    }))
}

/// `nearest %entity types% (to|from|of|around|relative to) %location/player%`: the closest entity of
/// the wanted types in the centre's world, nothing when there is none. Online players count as
/// `minecraft:player` and other player-type entities (NPCs) come from the world's entities, as in
/// `entities_where`. Ties keep the first entity found.
fn nearest(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let wanted = wanted_types(slot(cx, m, 0)?)?;
    let at = slot(cx, m, 1)?;
    let centre = location_of(cx, &at)?;
    let dist = |p: &Location| {
        ((p.x - centre.x).powi(2) + (p.y - centre.y).powi(2) + (p.z - centre.z).powi(2)).sqrt()
    };
    let mut candidates: Vec<(f64, Value)> = Vec::new();
    if wanted.iter().any(|w| kind_matches("minecraft:player", w)) {
        for (uuid, name) in cx.host.online_players() {
            if let Some(state) = cx.host.player_state(uuid) {
                if state.location.world.eq_ignore_ascii_case(&centre.world) {
                    candidates.push((dist(&state.location), Value::Player { uuid, name }));
                }
            }
        }
    }
    for e in cx.host.entities_in_world(&centre.world) {
        if e.kind != "minecraft:player"
            && e.location.world.eq_ignore_ascii_case(&centre.world)
            && wanted.iter().any(|w| kind_matches(&e.kind, w))
        {
            candidates.push((dist(&e.location), entity_value(&e)));
        }
    }
    let mut best: Option<(f64, Value)> = None;
    for (d, v) in candidates {
        match &best {
            Some((b, _)) if *b <= d => {}
            _ => best = Some((d, v)),
        }
    }
    Ok(best.map_or(Value::Null, |(_, v)| v))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_expression(
        "nearest entity",
        &["[the] nearest %entity types% (to|from|of|around|relative to) %location/player%"],
        0,
        "object",
        nearest,
    )?;
    reg.add_expression(
        "entities of type in worlds",
        &["[(all [[of] the]|the)] %entity types% in %worlds%"],
        2,
        "object",
        entities_in_worlds,
    )?;
    reg.add_expression(
        "entities of type near",
        &["[(all [[of] the]|the)] %entity types% (within|in radius|within radius) %number% [block[s]] (of|around) %location/player%"],
        2,
        "object",
        entities_near,
    )?;
    reg.add_expression(
        "player count",
        &[
            "[the] [online] player (count|amount|number)",
            "[the] (count|amount|number|size) of online players",
        ],
        -1,
        "integer",
        player_count,
    )?;
    reg.add_expression(
        "offset",
        &[
            "[%-number% (block|met(er|re))[s]] [to the] (1¦above|1¦over|2¦below|2¦under|3¦north of|4¦south of|5¦east of|6¦west of) %location/player%",
        ],
        0,
        "location",
        offset,
    )?;
    reg.add_expression(
        "location at",
        &["[the] (location|position) [at] %number%, %number%(,| and) %number% [(in|of) %world%]"],
        0,
        "location",
        location_from_coordinates,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::kind_matches;
    use crate::horizon_skript_core::host::{EntityInfo, Recorder};
    use crate::horizon_skript_core::syntax::support::testing::{expr, run};
    use crate::horizon_skript_core::value::Location;

    fn at(world: &str, x: f64) -> Location {
        Location {
            world: world.into(),
            x,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    fn mob(uuid: u128, kind: &str, world: &str, x: f64) -> EntityInfo {
        EntityInfo {
            uuid,
            kind: kind.into(),
            name: String::new(),
            location: at(world, x),
        }
    }

    fn list(h: &Recorder, e: &str) -> String {
        let logs = run(
            h,
            &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r::*}}%\"\n"),
        );
        assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
        h.broadcasts.borrow()[0].clone()
    }

    #[test]
    fn plural_names_match_the_entity_kind() {
        assert!(kind_matches("minecraft:zombie", "minecraft:zombies"));
        assert!(kind_matches("minecraft:witch", "minecraft:witches"));
        assert!(kind_matches("minecraft:enderman", "minecraft:endermen"));
        assert!(kind_matches("minecraft:cow", "minecraft:cow"));
        assert!(!kind_matches("minecraft:cow", "minecraft:pigs"));
    }

    #[test]
    fn entities_of_a_type_in_a_world_and_near_a_place() {
        let mut h = Recorder::default();
        h.entities = vec![
            mob(1, "minecraft:zombie", "w", 0.0),
            mob(2, "minecraft:zombie", "w", 50.0),
            mob(3, "minecraft:cow", "w", 1.0),
            mob(4, "minecraft:zombie", "other", 0.0),
        ];
        assert_eq!(list(&h, "all zombies in world \"w\""), "zombie, zombie");
        let mut h2 = Recorder::default();
        h2.entities = h.entities.clone();
        assert_eq!(
            list(
                &h2,
                "all zombies within 10 blocks of location(0, 64, 0, world \"w\")"
            ),
            "zombie"
        );
    }

    #[test]
    fn nearest_entity_of_a_type_and_the_type_of_a_player() {
        let mut h = Recorder::with_players(&[(7, "Alice")]);
        h.entities = vec![
            mob(1, "minecraft:zombie", "w", 50.0),
            mob(2, "minecraft:cow", "w", 1.0),
        ];
        assert_eq!(
            expr(&h, "nearest cow to location(0, 64, 0, world \"w\")"),
            "cow"
        );
        assert_eq!(
            expr(&h, "nearest zombie to location(0, 64, 0, world \"w\")"),
            "zombie"
        );
        assert_eq!(expr(&h, "entity type of Alice"), "player");
    }

    #[test]
    fn offsets_and_locations_from_coordinates() {
        let h = Recorder::default();
        assert_eq!(
            expr(&h, "2 blocks above location(1, 2, 3, world \"w\")"),
            "w at 1, 4, 3"
        );
        let h = Recorder::default();
        assert_eq!(
            expr(&h, "1 block north of location(1, 2, 3, world \"w\")"),
            "w at 1, 2, 2"
        );
        let h = Recorder::default();
        assert_eq!(
            expr(&h, "3 blocks to the east of location(1, 2, 3, world \"w\")"),
            "w at 4, 2, 3"
        );
        let h = Recorder::default();
        assert_eq!(
            expr(&h, "below location(1, 2, 3, world \"w\")"),
            "w at 1, 1, 3"
        );
        let h = Recorder::default();
        assert_eq!(
            expr(&h, "location at 1, 2, 3 in world \"w\""),
            "w at 1, 2, 3"
        );
    }

    #[test]
    fn the_player_count_is_the_number_online() {
        let h = Recorder::with_players(&[(1, "Alice"), (2, "Bob")]);
        assert_eq!(expr(&h, "the player count"), "2");
    }
}
