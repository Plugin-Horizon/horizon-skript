//! Conditions about players: online, operator, alive, flying, sneaking, sprinting, swimming,
//! holding, permissions, inventory contents, `can see`, and `is in world` (players and
//! locations). Ported from Skript's `CondIsOnline`, `CondIsOp`, `CondIsAlive`, `CondIsFlying`,
//! `CondIsSneaking`, `CondIsSprinting`, `CondIsSwimming`, `CondItemInHand`, `CondPermission`,
//! `CondContains` (inventories), `CondCanSee` and `CondIsWithin` (worlds).

use crate::error::RunError;
use crate::host::PlayerState;
use crate::pattern::PatternError;
use crate::registry::{Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::{self, count_in, items, negated, player_of, slot};
use crate::value::Value;

/// Whether every player in the first slot has a property. Negation (mark 1) is applied here.
fn every_player(
    cx: &mut Ctx<'_>,
    m: &Matched,
    check: impl Fn(u128, Option<PlayerState>) -> bool,
) -> Result<bool, RunError> {
    let players = items(slot(cx, m, 0)?);
    if players.is_empty() {
        return Ok(negated(m));
    }
    let mut all = true;
    for p in &players {
        let (uuid, _) = player_of(p)?;
        if !check(uuid, cx.host.player_state(uuid)) {
            all = false;
        }
    }
    Ok(all != negated(m))
}

/// Holds for a player who is online and has the property `f`.
fn with_state(
    cx: &mut Ctx<'_>,
    m: &Matched,
    f: impl Fn(&PlayerState) -> bool,
) -> Result<bool, RunError> {
    every_player(cx, m, |_, s| s.as_ref().is_some_and(&f))
}

fn online(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    every_player(cx, m, |_, s| s.is_some())
}

fn op(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.op)
}

/// `is alive` (health above zero) and `is dead` (mark 2).
fn alive(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let dead = m.has_mark(2);
    every_player(cx, m, |_, s| s.is_some_and(|s| (s.health > 0.0) != dead))
}

fn flying(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.flying)
}

fn can_fly(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.can_fly)
}

fn sneaking(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.sneaking)
}

fn sprinting(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.sprinting)
}

fn swimming(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    with_state(cx, m, |s| s.swimming)
}

/// Holding one of the item types in the main hand (matched by type, not by amount).
fn holding(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let wanted: Vec<String> = items(slot(cx, m, 1)?)
        .into_iter()
        .filter_map(|v| match v {
            Value::ItemType { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    with_state(cx, m, |s| wanted.iter().any(|w| *w == s.held))
}

fn permission(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let nodes: Vec<String> = items(slot(cx, m, 1)?)
        .iter()
        .map(support::text_of)
        .collect();
    let host = cx.host;
    every_player(cx, m, |uuid, s| {
        s.is_some() && nodes.iter().all(|n| host.has_permission(uuid, n))
    })
}

/// `player has 4 flint and 2 iron ingots`: the inventory holds every item in at least that amount.
fn inventory_has(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let wanted = items(slot(cx, m, 1)?);
    with_state(cx, m, |s| {
        wanted.iter().all(|w| match w {
            Value::ItemType { id, amount, .. } => count_in(s, id) >= *amount,
            _ => false,
        })
    })
}

fn can_see(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let viewer = slot(cx, m, 0)?;
    let target = slot(cx, m, 1)?;
    let (v, _) = player_of(&viewer)?;
    let (t, _) = player_of(&target)?;
    Ok(cx.host.can_see(v, t) != negated(m))
}

fn in_world(cx: &mut Ctx<'_>, m: &Matched) -> Result<bool, RunError> {
    let left = items(slot(cx, m, 0)?);
    let world = match slot(cx, m, 1)? {
        Value::World(w) => w,
        other => {
            return Err(RunError::new(format!(
                "expected a world, got {}",
                other.class()
            )));
        }
    };
    let mut all = !left.is_empty();
    for v in &left {
        if !support::world_of(cx, v)?.eq_ignore_ascii_case(&world) {
            all = false;
        }
    }
    Ok(all != negated(m))
}

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_condition(
        "online",
        &["%players% (is|are)(1¦ not|1¦n't|) online"],
        0,
        online,
    )?;
    reg.add_condition(
        "operator",
        &["%players% (is|are)(1¦ not|1¦n't|) [a|an] [server] op[erator][s]"],
        0,
        op,
    )?;
    reg.add_condition(
        "alive",
        &["%players% (is|are)(1¦ not|1¦n't|) (alive|2¦dead)"],
        0,
        alive,
    )?;
    reg.add_condition(
        "flying",
        &["%players% (is|are)(1¦ not|1¦n't|) flying"],
        0,
        flying,
    )?;
    reg.add_condition(
        "can fly",
        &["%players% (can|1¦can't|1¦cannot|1¦can not) fly"],
        0,
        can_fly,
    )?;
    reg.add_condition(
        "sneaking",
        &["%players% (is|are)(1¦ not|1¦n't|) sneaking"],
        0,
        sneaking,
    )?;
    reg.add_condition(
        "sprinting",
        &["%players% (is|are)(1¦ not|1¦n't|) sprinting"],
        0,
        sprinting,
    )?;
    reg.add_condition(
        "swimming",
        &["%players% (is|are)(1¦ not|1¦n't|) swimming"],
        0,
        swimming,
    )?;
    reg.add_condition(
        "holding",
        &["%players% (is|are)(1¦ not|1¦n't|) holding %item types%"],
        0,
        holding,
    )?;
    // Before the inventory test: `has permission x` must not read `permission x` as an item.
    reg.add_condition(
        "permission",
        &["%players% (has|have)(1¦ not|1¦n't|) [the] permission[s] %texts%"],
        0,
        permission,
    )?;
    reg.add_condition(
        "inventory",
        &["%players% (has|have|1¦(does not|doesn't|do not|don't) have) %item types%"],
        1,
        inventory_has,
    )?;
    reg.add_condition(
        "can see",
        &["%player% (can|1¦can't|1¦cannot|1¦can not) see %player%"],
        0,
        can_see,
    )?;
    reg.add_condition(
        "in world",
        &["%locations/players% (is|are)(1¦ not|1¦n't|) in [the] world %world%"],
        0,
        in_world,
    )?;
    Ok(())
}
