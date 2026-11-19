//! Effects at a place: `strike lightning`, `create an explosion`, `drop` items and `break` blocks.
//! Patterns follow Skript's `EffLightning`, `EffExplosion`, `EffDrop` and `EffBreakNaturally`,
//! with `at %locations%` where Skript has `%directions% %locations%`. Each runs a vanilla command
//! at the place (see [`super::at`] for how the world becomes a dimension).
//!
//! Differences from Skript, all from what a command can do: the explosion is a creeper that
//! blows up at once (power is its radius, whole numbers up to 127; blocks burn only as the
//! `mob_griefing` rule allows, and there is no `with fire` or `safe` form), the lightning effect
//! is sparks and thunder rather than a harmless bolt, and a dropped item has no random velocity.

use super::{at, console, in_dimension, locations_in, next, number_in, slot};
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::Value;

/// The largest stack a drop is cut into.
const STACK: i64 = 64;
/// The largest explosion radius a creeper's `ExplosionRadius` byte holds.
const MAX_POWER: f64 = 127.0;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    // The effect form first: `lightning effect at x` must not read `effect at x` as a location.
    reg.add_effect(
        "lightning effect",
        &["(create|strike) lightning[ ]effect [at] %locations%"],
        0,
        lightning_effect,
    )?;
    reg.add_effect(
        "lightning",
        &["(create|strike) lightning [at] %locations%"],
        0,
        lightning,
    )?;
    reg.add_effect(
        "explosion",
        &["(create|make) [an] explosion (of|with) (force|strength|power) %number% [at %-locations%]"],
        0,
        explosion,
    )?;
    reg.add_effect(
        "fake explosion",
        &[
            "(create|make) [a] fake explosion [at %-locations%]",
            "(create|make) [an] explosion[ ]effect [at %-locations%]",
        ],
        0,
        fake_explosion,
    )?;
    reg.add_effect(
        "drop",
        &["drop %item types% [at %-locations%] [without velocity]"],
        0,
        drop_items,
    )?;
    reg.add_effect(
        "break naturally",
        &["break %locations% naturally"],
        0,
        break_naturally,
    )?;
    reg.add_effect("break", &["break %locations%"], 0, break_block)?;
    Ok(())
}

fn lightning(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for loc in locations_in(cx, m, 0)? {
        console(
            cx,
            &format!("{}summon minecraft:lightning_bolt ~ ~ ~", at(&loc)),
        );
    }
    next()
}

/// Sparks and a thunder clap for everyone near; nothing is hurt or set on fire.
fn lightning_effect(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for loc in locations_in(cx, m, 0)? {
        let at = at(&loc);
        console(
            cx,
            &format!("{at}particle minecraft:electric_spark ~ ~1 ~ 0.3 1 0.3 0.1 40 force"),
        );
        console(
            cx,
            &format!("{at}playsound minecraft:entity.lightning_bolt.thunder weather @a ~ ~ ~ 10 1"),
        );
    }
    next()
}

/// `create an explosion of force N at LOCATIONS`: force 0 is only the show.
fn explosion(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let power = number_in(cx, m, 0)?.unwrap_or(0.0);
    let power = power.round().clamp(0.0, MAX_POWER) as i64;
    for loc in locations_in(cx, m, 1)? {
        if power == 0 {
            explosion_effect(cx, &loc);
        } else {
            console(
                cx,
                &format!(
                    "{}summon minecraft:creeper ~ ~ ~ {{Fuse:0s,ignited:1b,ExplosionRadius:{power}b}}",
                    at(&loc)
                ),
            );
        }
    }
    next()
}

fn fake_explosion(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for loc in locations_in(cx, m, 0)? {
        explosion_effect(cx, &loc);
    }
    next()
}

fn explosion_effect(cx: &mut Ctx<'_>, loc: &crate::horizon_skript_core::value::Location) {
    let at = at(loc);
    console(cx, &format!("{at}particle minecraft:explosion_emitter ~ ~ ~"));
    console(
        cx,
        &format!("{at}playsound minecraft:entity.generic.explode block @a ~ ~ ~ 4 1"),
    );
}

/// `drop 3 stone at LOCATIONS`: item entities, cut into stacks of 64. The slot of the places is
/// empty for the event's player.
fn drop_items(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let mut stacks = Vec::new();
    for item in super::items_of(slot(cx, m, 0)?) {
        let Value::ItemType { id, amount, name } = item else {
            return Err(RunError::new(format!(
                "drop needs item types, got {}",
                item.class()
            )));
        };
        let mut left = amount;
        while left > 0 {
            let n = left.min(STACK);
            stacks.push(item_nbt(&id, n, name.as_deref()));
            left -= n;
        }
    }
    for loc in locations_in(cx, m, 1)? {
        for nbt in &stacks {
            console(
                cx,
                &format!("{}summon minecraft:item ~ ~ ~ {{Item:{nbt}}}", at(&loc)),
            );
        }
    }
    next()
}

/// The item compound of a dropped stack.
fn item_nbt(id: &str, count: i64, name: Option<&str>) -> String {
    match name {
        None => format!("{{id:\"{id}\",count:{count}}}"),
        Some(name) => format!(
            "{{id:\"{id}\",count:{count},components:{{\"minecraft:custom_name\":\"{}\"}}}}",
            name.replace('\\', "\\\\").replace('"', "\\\"")
        ),
    }
}

/// `break LOCATIONS naturally`: the block goes and drops its items.
fn break_naturally(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    set_air(cx, m, "destroy")
}

/// `break LOCATIONS`: the block becomes air with no drops.
fn break_block(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    set_air(cx, m, "replace")
}

fn set_air(cx: &mut Ctx<'_>, m: &Matched, mode: &str) -> Result<Flow, RunError> {
    for loc in locations_in(cx, m, 0)? {
        console(
            cx,
            &format!(
                "{} run setblock {} {} {} minecraft:air {mode}",
                in_dimension(&loc),
                loc.x.floor() as i64,
                loc.y.floor() as i64,
                loc.z.floor() as i64
            ),
        );
    }
    next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    const AT: &str = "location(1.5, 64, 2, world \"world\")";
    const HERE: &str = "execute in minecraft:overworld positioned 1.5 64 2 run";

    #[test]
    fn lightning_and_its_effect() {
        let host = Recorder::default();
        let logs = run(
            &host,
            &format!("\tstrike lightning at {AT}\n\tcreate lightning effect at {AT}\n"),
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                format!("console {HERE} summon minecraft:lightning_bolt ~ ~ ~"),
                format!("console {HERE} particle minecraft:electric_spark ~ ~1 ~ 0.3 1 0.3 0.1 40 force"),
                format!("console {HERE} playsound minecraft:entity.lightning_bolt.thunder weather @a ~ ~ ~ 10 1"),
            ]
        );
    }

    #[test]
    fn explosions_are_creepers_with_the_power_as_radius() {
        let host = Recorder::default();
        let logs = run(
            &host,
            &format!("\tcreate an explosion of force 3 at {AT}\n\tmake a fake explosion at {AT}\n"),
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                format!("console {HERE} summon minecraft:creeper ~ ~ ~ {{Fuse:0s,ignited:1b,ExplosionRadius:3b}}"),
                format!("console {HERE} particle minecraft:explosion_emitter ~ ~ ~"),
                format!("console {HERE} playsound minecraft:entity.generic.explode block @a ~ ~ ~ 4 1"),
            ]
        );
    }

    #[test]
    fn drops_are_cut_into_stacks_of_sixty_four() {
        let host = Recorder::default();
        let logs = run(&host, &format!("\tdrop 70 stone at {AT}\n"));
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                format!("console {HERE} summon minecraft:item ~ ~ ~ {{Item:{{id:\"minecraft:stone\",count:64}}}}"),
                format!("console {HERE} summon minecraft:item ~ ~ ~ {{Item:{{id:\"minecraft:stone\",count:6}}}}"),
            ]
        );
    }

    #[test]
    fn a_named_item_escapes_its_name() {
        assert_eq!(
            item_nbt("minecraft:stick", 1, Some("The \"Rod\"")),
            "{id:\"minecraft:stick\",count:1,components:{\"minecraft:custom_name\":\"The \\\"Rod\\\"\"}}"
        );
    }

    #[test]
    fn breaking_a_block_with_and_without_drops() {
        let host = Recorder::default();
        let logs = run(
            &host,
            &format!("\tbreak {AT}\n\tbreak {AT} naturally\n"),
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console execute in minecraft:overworld run setblock 1 64 2 minecraft:air replace",
                "console execute in minecraft:overworld run setblock 1 64 2 minecraft:air destroy",
            ]
        );
    }
}
