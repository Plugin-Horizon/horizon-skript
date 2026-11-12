//! Particles: `draw` with a dust colour (`draw 12 rgb(255, 0, 0) dust particle of size 1 at ...`)
//! and with the other kinds (`draw 1 portal particle at ...`). Patterns follow Skript's
//! `EffPlayEffect` (`[:force] (play|show|draw) ...`) and `DataParticles` (`%color% dust particle[s]
//! [of size %number%]`). `force` is mark 1: the particle is sent at any distance.
//!
//! The effect only describes the particle to the engine (`Actions::draw_particles`); how it is
//! sent is the glue's concern. `with offset <vector>` is not supported (there is no vector class).

use super::{items_of, number_in, slot};
use crate::error::RunError;
use crate::host::ParticleSpec;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::syntax::support::colour_of;
use crate::value::Value;

/// The particle kinds a script can draw by name, as Skript writes them. `dust` is separate because
/// it takes a colour. Longer names come first so that `soul fire flame` is not read as `soul`.
pub const NAMED: &[&str] = &[
    "soul fire flame",
    "dripping water",
    "electric spark",
    "happy villager",
    "large smoke",
    "end rod",
    "explosion",
    "firework",
    "enchant",
    "portal",
    "splash",
    "flame",
    "cloud",
    "snowflake",
    "heart",
    "note",
    "smoke",
    "crit",
    "witch",
    "soul",
    "totem",
    "lava",
];

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "draw dust",
        &["[1¦force ]draw %number% %object% dust particle[s] [of size %-number%] at %locations%"],
        0,
        draw_dust,
    )?;
    // The names as a regex group: `<(...)>` keeps the particle name the script wrote.
    let named = format!(
        "[1¦force ]draw %number% <({})> particle[s] [with extra %-number%] at %locations%",
        NAMED.join("|")
    );
    reg.add_effect("draw named", &[named.as_str()], 0, draw_named)?;
    Ok(())
}

/// `draw N <colour> dust particle [of size S] at LOCATIONS`. Slots: 0 count, 1 colour, 2 size,
/// 3 locations.
fn draw_dust(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let colour = colour_of(&slot(cx, m, 1)?)
        .ok_or_else(|| RunError::new("a dust particle needs a colour: rgb(red, green, blue)"))?;
    let spec = ParticleSpec {
        name: "dust".to_owned(),
        count: count_of(cx, m)?,
        colour: Some(colour),
        size: number_in(cx, m, 2)?.unwrap_or(1.0),
        extra: 0.0,
        force: m.has_mark(1),
    };
    draw_at(cx, m, 3, &spec)
}

/// `draw N <name> particle [with extra X] at LOCATIONS`. Slots: 0 count, 1 extra, 2 locations.
/// The name is regex group 0.
fn draw_named(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let spec = ParticleSpec {
        name: m.regex(0).unwrap_or_default().to_owned(),
        count: count_of(cx, m)?,
        colour: None,
        size: 1.0,
        extra: number_in(cx, m, 1)?.unwrap_or(0.0),
        force: m.has_mark(1),
    };
    draw_at(cx, m, 2, &spec)
}

fn count_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<i64, RunError> {
    Ok(number_in(cx, m, 0)?.unwrap_or(1.0).trunc() as i64)
}

/// Draws the particle at each location of slot `n`.
fn draw_at(cx: &mut Ctx<'_>, m: &Matched, n: usize, spec: &ParticleSpec) -> Result<Flow, RunError> {
    for value in items_of(slot(cx, m, n)?) {
        match value {
            Value::Location(at) => {
                cx.host.actions().draw_particles(&at, spec);
            }
            other => {
                return Err(RunError::new(format!(
                    "draw needs a location, got {}",
                    other.class()
                )));
            }
        }
    }
    Ok(Flow::Next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Recorder;
    use crate::syntax::support::testing::{acted, run};

    #[test]
    fn long_names_come_before_their_prefixes() {
        let at = |n: &str| NAMED.iter().position(|x| *x == n).unwrap();
        assert!(at("soul fire flame") < at("soul"));
        assert!(!NAMED.contains(&"dust"));
    }

    #[test]
    fn force_dust_draws_with_colour_and_size() {
        let host = Recorder::default();
        let logs = run(
            &host,
            "\tforce draw 12 rgb(255, 200 + 45 * 0.5, 90.9) dust particle of size 1.5 at location(1, 64, 2, world \"stage\")\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        let lines = acted(&host);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "particle force dust x12 [255, 222, 90] size 1.5 at stage at 1, 64, 2"
        );
    }

    #[test]
    fn named_particles_are_normal_unless_forced() {
        let host = Recorder::default();
        let logs = run(
            &host,
            "\tdraw 1 end rod particle with extra 0.5 at location(0, 0, 0, world \"stage\")\n\tforce draw 3 portal particles at location(0, 1, 0, world \"stage\")\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        let lines = acted(&host);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], "particle normal end rod x1 size 1 at stage at 0, 0, 0");
        assert_eq!(lines[1], "particle force portal x3 size 1 at stage at 0, 1, 0");
    }

    #[test]
    fn a_dust_particle_without_a_colour_is_an_error() {
        let host = Recorder::default();
        let logs = run(&host, "\tdraw 1 \"red\" dust particle at location(0, 0, 0, world \"stage\")\n");
        assert!(logs.iter().any(|l| l.contains("needs a colour")), "{logs:?}");
    }
}
