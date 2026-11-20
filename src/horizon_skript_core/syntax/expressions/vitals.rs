//! Player properties the engine reports beyond the basic state: fly and walk speed, total
//! experience, fire time, remaining air, saturation, the vehicle and passengers, the respawn
//! (bed) location, and a world's time, weather and difficulty and a location's biome. Ported from
//! Skript's `ExprSpeed`, `ExprTotalExperience`, `ExprFireTicks`, `ExprRemainingAir`,
//! `ExprSaturation`, `ExprVehicle`, `ExprPassenger`, `ExprBed`, `ExprBiome`, `ExprTime`,
//! `ExprWeather` and `ExprDifficulty`.
//!
//! Fly and walk speed use Bukkit's scale (twice the vanilla ability), as Skript's do. Time is the
//! tick of the day (there is no `time` class), and a biome is its name without `minecraft:`.
//! There is no engine read for the fall distance, the ping or the light level, and the port has
//! no vector class for velocity (see GAPS_UPSTREAM.md).

use crate::horizon_skript_core::classes;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::host::Vitals;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Change, ExprSyntax, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{location_at, num, player_of, slot, world_at};
use crate::horizon_skript_core::value::Value;

/// The extra read-outs of the player in slot 0.
fn vitals_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<(u128, Vitals), RunError> {
    let (uuid, name) = player_of(&slot(cx, m, 0)?)?;
    cx.host
        .vitals(uuid)
        .map(|v| (uuid, v))
        .ok_or_else(|| RunError::new(format!("{name} is not online")))
}

/// How a property of a player is changed: the new value from the old one and the change.
type Setter = fn(&mut Ctx<'_>, u128, &Vitals, Change, &Value) -> Result<bool, RunError>;

/// A property of one player: how to read it and, if it can change, how.
struct Prop {
    get: fn(&Vitals) -> Value,
    set: Option<Setter>,
}

impl ExprSyntax for Prop {
    fn eval(&self, cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
        let (_, v) = vitals_at(cx, m)?;
        Ok((self.get)(&v))
    }

    fn change(
        &self,
        cx: &mut Ctx<'_>,
        m: &Matched,
        how: Change,
        by: &Value,
    ) -> Option<Result<(), RunError>> {
        let set = self.set?;
        Some((|| {
            let (uuid, v) = vitals_at(cx, m)?;
            if set(cx, uuid, &v, how, by)? {
                Ok(())
            } else {
                Err(RunError::new("the server refused the change"))
            }
        })())
    }
}

/// `old` changed by `by` as `how` says.
fn changed(old: f64, how: Change, by: f64) -> f64 {
    match how {
        Change::Set => by,
        Change::Add => old + by,
        Change::Remove => old - by,
    }
}

fn number(by: &Value) -> Result<f64, RunError> {
    by.as_f64()
        .ok_or_else(|| RunError::new("this changes by a number"))
}

/// A timespan in ticks (a timespan value is in milliseconds, 50 per tick).
fn ticks(by: &Value) -> Result<f64, RunError> {
    match by {
        Value::Timespan(ms) => Ok(*ms as f64 / 50.0),
        _ => Err(RunError::new("this changes by a timespan")),
    }
}

/// Speeds use Bukkit's scale, twice the ability value, and stay within -1 and 1.
fn set_speed(
    cx: &mut Ctx<'_>,
    uuid: u128,
    old_ability: f64,
    how: Change,
    by: &Value,
    walk: bool,
) -> Result<bool, RunError> {
    let new = changed(old_ability * 2.0, how, number(by)?).clamp(-1.0, 1.0) / 2.0;
    Ok(if walk {
        cx.host.actions().set_abilities(uuid, None, None, None, Some(new))
    } else {
        cx.host.actions().set_abilities(uuid, None, None, Some(new), None)
    })
}

fn register_prop(
    reg: &mut Registry,
    name: &'static str,
    property: &str,
    returns: &'static str,
    prop: Prop,
) -> Result<(), PatternError> {
    let of = format!("[the] {property} of %player%");
    let possessive = format!("%player%'[s] {property}");
    reg.add_expression(name, &[of.as_str(), possessive.as_str()], 0, returns, prop)
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    register_prop(
        reg,
        "fly speed",
        "fl(y[ing]|ight)[( |-)]speed",
        "number",
        Prop {
            get: |v| num(v.fly_speed * 2.0),
            set: Some(|cx, uuid, v, how, by| set_speed(cx, uuid, v.fly_speed, how, by, false)),
        },
    )?;
    register_prop(
        reg,
        "walk speed",
        "walk[ing][( |-)]speed",
        "number",
        Prop {
            get: |v| num(v.walk_speed * 2.0),
            set: Some(|cx, uuid, v, how, by| set_speed(cx, uuid, v.walk_speed, how, by, true)),
        },
    )?;
    register_prop(
        reg,
        "total experience",
        "total [e]xp[erience]",
        "integer",
        Prop {
            get: |v| Value::Int(v.exp_total),
            set: None,
        },
    )?;
    register_prop(
        reg,
        "fire time",
        "(burn[ing]|fire) (time|duration)",
        "timespan",
        Prop {
            get: |v| Value::Timespan(v.fire_ticks * 50),
            set: Some(|cx, uuid, v, how, by| {
                let new = changed(v.fire_ticks as f64, how, ticks(by)?).max(0.0);
                Ok(cx.host.actions().set_fire_ticks(uuid, new as i64))
            }),
        },
    )?;
    register_prop(
        reg,
        "remaining air",
        "remaining air",
        "timespan",
        Prop {
            get: |v| Value::Timespan(v.air * 50),
            set: Some(|cx, uuid, v, how, by| {
                let new = changed(v.air as f64, how, ticks(by)?);
                Ok(cx.host.actions().set_air(uuid, new as i64))
            }),
        },
    )?;
    register_prop(
        reg,
        "saturation",
        "saturation",
        "number",
        Prop {
            get: |v| num(v.saturation),
            set: Some(|cx, uuid, v, how, by| {
                let new = changed(v.saturation, how, number(by)?).max(0.0);
                Ok(cx.host.actions().set_saturation(uuid, new))
            }),
        },
    )?;
    reg.add_expression("vehicle", &["[the] vehicle of %player%", "%player%'[s] vehicle"], 0, "player", vehicle)?;
    reg.add_expression(
        "passengers",
        &["[the] passenger[s] of %player%", "%player%'[s] passenger[s]"],
        0,
        "player",
        passengers,
    )?;
    reg.add_expression(
        "bed location",
        &[
            "[the] (bed|respawn) [location] of %player%",
            "%player%'[s] (bed|respawn) [location]",
        ],
        0,
        "location",
        bed_location,
    )?;
    reg.add_expression(
        "biome",
        &["[the] biome (at|of) %location%", "%location%'[s] biome"],
        0,
        "text",
        biome,
    )?;
    reg.add_expression(
        "world time",
        &["[the] time (in|of) %world%", "%world%'[s] time"],
        0,
        "integer",
        world_time,
    )?;
    reg.add_expression(
        "world weather",
        &["[the] weather (in|of) %world%", "%world%'[s] weather"],
        0,
        "text",
        world_weather,
    )?;
    reg.add_expression(
        "world difficulty",
        &["[the] difficult(y|ies) (in|of) %world%", "%world%'[s] difficult(y|ies)"],
        0,
        "text",
        world_difficulty,
    )?;
    Ok(())
}

/// The player value of an entity the host knows by UUID; `None` for any other.
fn entity_player(cx: &Ctx<'_>, uuid: u128) -> Option<Value> {
    let name = cx
        .host
        .player_state(uuid)
        .or_else(|| cx.host.entity_state(uuid))?
        .name;
    Some(Value::Player { uuid, name })
}

fn vehicle(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, v) = vitals_at(cx, m)?;
    Ok(v.vehicle
        .and_then(|u| entity_player(cx, u))
        .unwrap_or(Value::Null))
}

fn passengers(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, v) = vitals_at(cx, m)?;
    Ok(Value::List(
        v.passengers
            .into_iter()
            .filter_map(|u| entity_player(cx, u))
            .collect(),
    ))
}

fn bed_location(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let (_, v) = vitals_at(cx, m)?;
    Ok(v.respawn.map_or(Value::Null, Value::Location))
}

fn biome(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let at = location_at(cx, m, 0)?;
    Ok(cx.host.biome_at(&at).map_or(Value::Null, |id| {
        Value::Text(classes::short_id(&id).replace('_', " "))
    }))
}

fn info_at(cx: &mut Ctx<'_>, m: &Matched) -> Result<crate::horizon_skript_core::host::WorldInfo, RunError> {
    let world = world_at(cx, m, 0)?;
    cx.host
        .world_info(&world)
        .ok_or_else(|| RunError::new(format!("the world {world} is not loaded")))
}

fn world_time(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::Int(info_at(cx, m)?.day_time.rem_euclid(24_000)))
}

fn world_weather(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let i = info_at(cx, m)?;
    Ok(Value::Text(
        if i.thundering {
            "thunder"
        } else if i.raining {
            "rain"
        } else {
            "clear"
        }
        .to_owned(),
    ))
}

fn world_difficulty(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    Ok(Value::Text(info_at(cx, m)?.difficulty))
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::{Recorder, Vitals, WorldInfo};
    use crate::horizon_skript_core::syntax::support::testing::{acted, expr, run};
    use crate::horizon_skript_core::value::Location;

    fn host() -> Recorder {
        let mut h = Recorder::with_players(&[(1, "Alice"), (2, "Bob")]);
        h.vitals = vec![
            (
                1,
                Vitals {
                    fly_speed: 0.05,
                    walk_speed: 0.1,
                    exp_total: 37,
                    saturation: 5.0,
                    fire_ticks: 100,
                    air: 200,
                    vehicle: Some(2),
                    passengers: vec![2],
                    respawn: Some(Location {
                        world: "world".into(),
                        x: 1.5,
                        y: 64.0,
                        z: 2.5,
                        yaw: 0.0,
                        pitch: 0.0,
                    }),
                    ..Vitals::default()
                },
            ),
            (2, Vitals::default()),
        ];
        h.states = vec![
            (1, crate::horizon_skript_core::syntax::effects::fixtures::state()),
            (2, {
                let mut s = crate::horizon_skript_core::syntax::effects::fixtures::state();
                s.name = "Bob".into();
                s
            }),
        ];
        h.worlds = vec![(
            "world".into(),
            WorldInfo {
                day_time: 30_000,
                game_time: 5,
                raining: true,
                thundering: false,
                difficulty: "hard".into(),
            },
        )];
        h
    }

    #[test]
    fn speeds_use_the_bukkit_scale() {
        let h = host();
        assert_eq!(expr(&h, "fly speed of Alice"), "0.1");
        assert_eq!(expr(&h, "walk speed of Alice"), "0.2");
    }

    #[test]
    fn numbers_and_timespans_come_from_the_vitals() {
        let h = host();
        assert_eq!(expr(&h, "total experience of Alice"), "37");
        assert_eq!(expr(&h, "saturation of Alice"), "5");
        assert_eq!(expr(&h, "fire time of Alice"), "5 seconds");
        assert_eq!(expr(&h, "remaining air of Alice"), "10 seconds");
    }

    #[test]
    fn vehicle_passengers_and_bed() {
        let h = host();
        assert_eq!(expr(&h, "vehicle of Alice"), "Bob");
        assert_eq!(expr(&h, "passengers of Alice"), "Bob");
        assert!(expr(&h, "bed location of Alice").contains("64"));
    }

    #[test]
    fn world_time_weather_and_difficulty() {
        let h = host();
        assert_eq!(expr(&h, "time in world \"world\""), "6000");
        assert_eq!(expr(&h, "weather in world \"world\""), "rain");
        assert_eq!(expr(&h, "difficulty of world \"world\""), "hard");
    }

    #[test]
    fn setting_fire_time_and_speed_asks_the_host() {
        let h = host();
        let logs = run(
            &h,
            "\tset fire time of Alice to 3 seconds\n\tset fly speed of Alice to 0.5\n\tadd 0.1 to walk speed of Alice\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&h),
            vec![
                "fire 1 60",
                "abilities 1 may_fly=None flying=None fly_speed=Some(0.25) walk_speed=None",
                "abilities 1 may_fly=None flying=None fly_speed=None walk_speed=Some(0.15000000000000002)",
            ]
        );
    }
}
