//! More conditions: `is wearing`, `is within N blocks of`, `is within A and B`, `is in the past`,
//! `has passed`, `was more than N ago`, `is alphanumeric`, `matches`, `the event is cancelled`,
//! `is edible` and `is in water`. Ported from Skript's `CondIsWearing`, `CondWithinRadius`,
//! `CondIsWithin` (cuboid), `CondPastFuture`, `CondDate`, `CondAlphanumeric`, `CondMatches`,
//! `CondCancelled`, `CondIsEdible` and `CondEntityIsInLiquid`.
//!
//! Registered at priority -1 where the text would otherwise reach the comparison `is`.
//!
//! Differences from Skript: `matches` uses Rust's regex syntax (no look-around), `is edible`
//! knows the foods of Minecraft 1.21 (an item added later is not edible until the list grows),
//! and `is in water|lava|liquid` reads the block at the player's feet (bubble columns and rain
//! are not counted). Entities other than players do not exist for scripts, so these take players.

use crate::horizon_skript_core::classes;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::syntax::support::{
    items, location_of, negated, number_of, player_of, slot, within,
};
use crate::horizon_skript_core::value::{Location, Value};
use regex::Regex;

/// The foods of Minecraft 1.21, by item id without the namespace.
const EDIBLE: &[&str] = &[
    "apple",
    "baked_potato",
    "beef",
    "beetroot",
    "beetroot_soup",
    "bread",
    "carrot",
    "chicken",
    "chorus_fruit",
    "cod",
    "cooked_beef",
    "cooked_chicken",
    "cooked_cod",
    "cooked_mutton",
    "cooked_porkchop",
    "cooked_rabbit",
    "cooked_salmon",
    "cookie",
    "dried_kelp",
    "enchanted_golden_apple",
    "glow_berries",
    "golden_apple",
    "golden_carrot",
    "honey_bottle",
    "melon_slice",
    "mushroom_stew",
    "mutton",
    "poisonous_potato",
    "porkchop",
    "potato",
    "pufferfish",
    "pumpkin_pie",
    "rabbit",
    "rabbit_stew",
    "rotten_flesh",
    "salmon",
    "spider_eye",
    "suspicious_stew",
    "sweet_berries",
    "tropical_fish",
];

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_condition(
        "wearing",
        &["%players% (is|are)(1¦ not|1¦n't|) wearing %item types%"],
        -1,
        wearing,
    )?;
    reg.add_condition(
        "within radius",
        &["%locations/players% (is|are)(1¦ not|1¦n't|) within %number% (block|metre|meter)[s] (around|of) %locations/players%"],
        -1,
        within_radius,
    )?;
    reg.add_condition(
        "within cuboid",
        &["%locations/players% (is|are)(1¦ not|1¦n't|) within %location% (and|to) %location%"],
        -1,
        within_cuboid,
    )?;
    reg.add_condition(
        "past or future",
        &[
            "%dates% (is|are)(1¦ not|1¦n't|) in the (past|2¦future)",
            "%dates% (has|have)(1¦ not|1¦n't|) passed",
        ],
        -1,
        past_or_future,
    )?;
    reg.add_condition(
        "date was",
        &["%date% (was|were|4¦wasn't|4¦weren't|4¦was not|4¦were not) (1¦more|2¦less) than %timespan%[ ago]"],
        -1,
        date_was,
    )?;
    reg.add_condition(
        "alphanumeric",
        &["%texts% (is|are)(1¦ not|1¦n't|) alphanumeric"],
        -1,
        alphanumeric,
    )?;
    reg.add_condition(
        "matches",
        &["%texts% (match[es]|1¦(does not|doesn't|do not|don't) match) %texts%"],
        0,
        matches_full,
    )?;
    reg.add_condition(
        "partially matches",
        &["%texts% (partially match[es]|1¦(does not|doesn't|do not|don't) partially match) %texts%"],
        0,
        matches_partly,
    )?;
    reg.add_condition(
        "cancelled",
        &["[the] event (is|1¦is not|1¦isn't) cancel[l]ed"],
        -1,
        cancelled,
    )?;
    reg.add_condition(
        "edible",
        &["%item types% (is|are)(1¦ not|1¦n't|) edible"],
        -1,
        edible,
    )?;
    reg.add_condition(
        "in liquid",
        &["%players% (is|are)(1¦ not|1¦n't|) in (2¦water|3¦lava|4¦liquid)"],
        -1,
        in_liquid,
    )?;
    Ok(())
}

/// `players are wearing items`: every item is worn in an armour slot of every player.
fn wearing(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let players = items(slot(cx, m, 0)?);
    let wanted = items(slot(cx, m, 1)?);
    let mut all = !players.is_empty() && !wanted.is_empty();
    for p in &players {
        let (uuid, _) = player_of(p)?;
        for w in &wanted {
            let Value::ItemType { id, .. } = w else {
                return Err(RunError::new(format!(
                    "expected an item type, got {}",
                    w.class()
                )));
            };
            let worn = ["helmet", "chestplate", "leggings", "boots"]
                .iter()
                .any(|s| cx.host.equipment(uuid, s).is_some_and(|i| i.id == *id));
            all &= worn;
        }
    }
    Ok(all != negated(m))
}

/// The locations of a list of locations and players.
fn places(cx: &Ctx<'_>, v: Value) -> Result<Vec<Location>, RunError> {
    items(v).iter().map(|i| location_of(cx, i)).collect()
}

/// `a is within 5 blocks of b`: each place on the left is near at least one centre.
fn within_radius(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = slot(cx, m, 0)?;
    let left = places(cx, left)?;
    let radius = number_of(&slot(cx, m, 1)?)?;
    let centres = slot(cx, m, 2)?;
    let centres = places(cx, centres)?;
    let all = !left.is_empty()
        && left
            .iter()
            .all(|l| centres.iter().any(|c| within(c, l, radius)));
    Ok(all != negated(m))
}

/// `a is within b and c`: inside the box the two corners make, edges included, same world.
fn within_cuboid(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = slot(cx, m, 0)?;
    let left = places(cx, left)?;
    let a = slot(cx, m, 1)?;
    let a = location_of(cx, &a)?;
    let b = slot(cx, m, 2)?;
    let b = location_of(cx, &b)?;
    let inside = |l: &Location| {
        l.world.eq_ignore_ascii_case(&a.world)
            && l.world.eq_ignore_ascii_case(&b.world)
            && (a.x.min(b.x)..=a.x.max(b.x)).contains(&l.x)
            && (a.y.min(b.y)..=a.y.max(b.y)).contains(&l.y)
            && (a.z.min(b.z)..=a.z.max(b.z)).contains(&l.z)
    };
    let all = !left.is_empty() && left.iter().all(inside);
    Ok(all != negated(m))
}

/// The milliseconds of a date value, text in the date format included.
fn date_ms(cx: &Ctx<'_>, v: Value) -> Result<i64, RunError> {
    match classes::convert(v.clone(), "date", cx.host) {
        Some(Value::Date(ms)) => Ok(ms),
        _ => Err(RunError::new(format!("expected a date, got {}", v.class()))),
    }
}

/// `date is in the past|future` and `date has passed`: mark 2 is the future.
fn past_or_future(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let now = cx.host.now_millis();
    let future = m.has_mark(2);
    let dates = items(slot(cx, m, 0)?);
    let mut all = !dates.is_empty();
    for d in dates {
        let ms = date_ms(cx, d)?;
        all &= if future { ms > now } else { ms < now };
    }
    Ok(all != negated(m))
}

/// `date was more|less than timespan ago`: mark 1 more, 2 less, 4 negated.
fn date_was(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let date = slot(cx, m, 0)?;
    let ms = date_ms(cx, date)?;
    let Value::Timespan(delta) = slot(cx, m, 1)? else {
        return Err(RunError::new("expected a timespan"));
    };
    let ago = cx.host.now_millis() - ms;
    let holds = if m.has_mark(1) { ago > delta } else { ago < delta };
    Ok(holds != m.has_mark(4))
}

/// Letters and digits only (Java's `Character.isLetterOrDigit`).
fn is_alphanumeric(s: &str) -> bool {
    s.chars().all(char::is_alphanumeric)
}

fn alphanumeric(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let texts = items(slot(cx, m, 0)?);
    let all = !texts.is_empty() && texts.iter().all(|t| is_alphanumeric(&t.to_text()));
    Ok(all != negated(m))
}

/// Whether every text matches at least one regex; `whole` anchors the regex to the whole text.
fn regex_matches(texts: &[String], regexes: &[String], whole: bool) -> Result<bool, RunError> {
    if texts.is_empty() || regexes.is_empty() {
        return Ok(false);
    }
    let mut compiled = Vec::new();
    for r in regexes {
        let src = if whole {
            format!("^(?:{r})$")
        } else {
            r.clone()
        };
        compiled.push(
            Regex::new(&src).map_err(|e| RunError::new(format!("bad regex {r}: {e}")))?,
        );
    }
    Ok(texts.iter().all(|t| compiled.iter().any(|re| re.is_match(t))))
}

fn matching(cx: &mut Ctx<'_>, m: &Matched, whole: bool) -> Result<bool, RunError> {
    let texts: Vec<String> = items(slot(cx, m, 0)?).iter().map(Value::to_text).collect();
    let regexes: Vec<String> = items(slot(cx, m, 1)?).iter().map(Value::to_text).collect();
    // Skript gives false for nothing on either side, even for `doesn't match`.
    if texts.is_empty() || regexes.is_empty() {
        return Ok(false);
    }
    Ok(regex_matches(&texts, &regexes, whole)? != negated(m))
}

fn matches_full(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    matching(cx, m, true)
}

fn matches_partly(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    matching(cx, m, false)
}

fn cancelled(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    Ok(cx.event.is_cancelled() != negated(m))
}

fn edible(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let types = items(slot(cx, m, 0)?);
    let mut all = !types.is_empty();
    for t in &types {
        let Value::ItemType { id, .. } = t else {
            return Err(RunError::new(format!(
                "expected an item type, got {}",
                t.class()
            )));
        };
        all &= EDIBLE.contains(&classes::short_id(id));
    }
    Ok(all != negated(m))
}

/// `players are in water|lava|liquid`: the block the feet are in.
fn in_liquid(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let players = items(slot(cx, m, 0)?);
    let mut all = !players.is_empty();
    for p in &players {
        let (uuid, _) = player_of(p)?;
        let here = cx.host.player_state(uuid).and_then(|s| {
            let l = s.location;
            cx.host.block_type_at(&Location {
                world: l.world,
                x: l.x.floor(),
                y: l.y.floor(),
                z: l.z.floor(),
                yaw: 0.0,
                pitch: 0.0,
            })
        });
        let hit = match here.as_deref() {
            Some("minecraft:water") => m.has_mark(2) || m.has_mark(4),
            Some("minecraft:lava") => m.has_mark(3) || m.has_mark(4),
            _ => false,
        };
        all &= hit;
    }
    Ok(all != negated(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon_skript_core::host::{ItemStack, Recorder};
    use crate::horizon_skript_core::syntax::effects::fixtures::state;
    use crate::horizon_skript_core::syntax::support::testing::run;

    /// The words a script broadcast: each `if` says its own word when it holds.
    fn said(host: &Recorder, body: &str) -> Vec<String> {
        let logs = run(host, body);
        assert!(logs.is_empty(), "{logs:?}");
        host.broadcasts.borrow().clone()
    }

    #[test]
    fn alphanumeric_text() {
        assert!(is_alphanumeric("abc123"));
        assert!(!is_alphanumeric("a b"));
        assert!(!is_alphanumeric("a_b"));
        let out = said(
            &Recorder::default(),
            "\tif \"abc123\" is alphanumeric:\n\t\tbroadcast \"one\"\n\tif \"a b\" is not alphanumeric:\n\t\tbroadcast \"two\"\n\tif \"a_b\" isn't alphanumeric:\n\t\tbroadcast \"three\"\n",
        );
        assert_eq!(out, vec!["one", "two", "three"]);
    }

    #[test]
    fn regex_conditions() {
        let out = said(
            &Recorder::default(),
            "\tif \"abc123\" matches \"[a-z]+[0-9]+\":\n\t\tbroadcast \"one\"\n\tif \"abc\" does not match \"[0-9]+\":\n\t\tbroadcast \"two\"\n\tif \"abc1\" partially matches \"[0-9]\":\n\t\tbroadcast \"three\"\n\tif \"abc1\" doesn't match \"[a-z]\":\n\t\tbroadcast \"four\"\n",
        );
        assert_eq!(out, vec!["one", "two", "three", "four"]);
        assert!(regex_matches(&["a".into()], &["(".into()], true).is_err());
    }

    #[test]
    fn dates_are_past_future_or_long_ago() {
        let mut host = Recorder::default();
        host.now = 1_000_000_000_000;
        let out = said(
            &host,
            "\tif \"2000-01-01\" is in the past:\n\t\tbroadcast \"one\"\n\tif \"2100-01-01\" is in the future:\n\t\tbroadcast \"two\"\n\tif \"2000-01-01\" has passed:\n\t\tbroadcast \"three\"\n\tif \"2100-01-01\" is not in the past:\n\t\tbroadcast \"four\"\n\tif \"2000-01-01\" was more than 5 days ago:\n\t\tbroadcast \"five\"\n\tif \"2000-01-01\" wasn't less than 5 days ago:\n\t\tbroadcast \"six\"\n\tif \"2000-01-01\" was less than 5 days ago:\n\t\tbroadcast \"never\"\n",
        );
        assert_eq!(out, vec!["one", "two", "three", "four", "five", "six"]);
    }

    #[test]
    fn places_are_within_a_radius_or_a_box() {
        let out = said(
            &Recorder::default(),
            "\tset {_a} to location(0, 64, 0, world \"w\")\n\tset {_b} to location(3, 64, 4, world \"w\")\n\tif {_a} is within 5 blocks of {_b}:\n\t\tbroadcast \"one\"\n\tif {_a} is not within 4 meters around {_b}:\n\t\tbroadcast \"two\"\n\tif {_a} is within location(-1, 60, -1, world \"w\") and location(1, 70, 1, world \"w\"):\n\t\tbroadcast \"three\"\n\tif {_b} is not within location(-1, 60, -1, world \"w\") and location(1, 70, 1, world \"w\"):\n\t\tbroadcast \"four\"\n",
        );
        assert_eq!(out, vec!["one", "two", "three", "four"]);
    }

    #[test]
    fn armour_edible_and_water() {
        let mut host = Recorder::with_players(&[(1, "Alice")]);
        host.equipment.push((
            1,
            "boots".into(),
            ItemStack {
                id: "minecraft:iron_boots".into(),
                amount: 1,
                name: None,
            },
        ));
        host.states.push((1, state()));
        host.blocks.push((
            Location {
                world: "world".into(),
                x: 0.0,
                y: 64.0,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            "minecraft:water".into(),
        ));
        let out = said(
            &host,
            "\tif Alice is wearing iron boots:\n\t\tbroadcast \"one\"\n\tif Alice is not wearing diamond helmet:\n\t\tbroadcast \"two\"\n\tif apple is edible:\n\t\tbroadcast \"three\"\n\tif stone is not edible:\n\t\tbroadcast \"four\"\n\tif Alice is in water:\n\t\tbroadcast \"five\"\n\tif Alice is in liquid:\n\t\tbroadcast \"six\"\n\tif Alice is not in lava:\n\t\tbroadcast \"seven\"\n",
        );
        assert_eq!(
            out,
            vec!["one", "two", "three", "four", "five", "six", "seven"]
        );
    }

    #[test]
    fn the_cancel_flag_and_a_missing_permission() {
        let mut host = Recorder::with_players(&[(1, "Alice")]);
        host.states.push((1, state()));
        host.permissions.push((1, "a.b".into()));
        let out = said(
            &host,
            "\tif the event is not cancelled:\n\t\tbroadcast \"one\"\n\tcancel the event\n\tif the event is cancelled:\n\t\tbroadcast \"two\"\n\tif Alice doesn't have permission \"c.d\":\n\t\tbroadcast \"three\"\n\tif Alice doesn't have permission \"a.b\":\n\t\tbroadcast \"never\"\n",
        );
        assert_eq!(out, vec!["one", "two", "three"]);
    }
}
