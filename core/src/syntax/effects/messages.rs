//! Messages: `send`/`message` (several texts, to players or to the event's player), `broadcast`
//! of several texts, action bars, titles and subtitles, and `make ... say`. Patterns follow
//! Skript's `EffMessage`, `EffBroadcast`, `EffActionBar`, `EffSendTitle` and `EffMakeSay`.
//! Titles default to Bukkit's times: fade in 10 ticks, stay 70, fade out 20.

use super::{next, players_in, slot, texts_in, ticks_in};
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;

const FADE_IN: u64 = 10;
const STAY: u64 = 70;
const FADE_OUT: u64 = 20;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "send action bar",
        &["send [the] action[ ]bar [with text] %text% [to %-players%]"],
        -2,
        action_bar,
    )?;
    reg.add_effect(
        "send title",
        // The spaces sit inside each optional part: adjacent optional parts with a space between
        // them do not match when the first is given and the second is not (pattern.rs).
        &["send title %text%[ with subtitle %-text%][ to %-players%][ for %-timespan%][ with fade[ ]in %-timespan%][ and[ with] fade[ ]out %-timespan%]"],
        -2,
        send_title,
    )?;
    reg.add_effect(
        "send subtitle",
        &["send subtitle %text%[ to %-players%][ for %-timespan%][ with fade[ ]in %-timespan%][ and[ with] fade[ ]out %-timespan%]"],
        -2,
        send_subtitle,
    )?;
    reg.add_effect(
        "send",
        &["(send|message) %texts% [to %-players%]"],
        -1,
        send,
    )?;
    reg.add_effect("broadcast", &["broadcast %texts%"], -1, broadcast)?;
    reg.add_effect(
        "make say",
        &[
            "make %players% (say|send [the] message[s]) %texts%",
            "force %players% to (say|send [the] message[s]) %texts%",
        ],
        0,
        make_say,
    )?;
    Ok(())
}

/// `send "text" [to players]`: every text to every player, or to the event's player.
fn send(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let texts = texts_in(cx, m, 0)?;
    for (uuid, _) in players_in(cx, m, 1)? {
        for text in &texts {
            cx.host.send(uuid, text);
        }
    }
    next()
}

/// `broadcast "a" and "b"`: one broadcast per text.
fn broadcast(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for text in texts_in(cx, m, 0)? {
        cx.host.broadcast(&text);
    }
    next()
}

/// `send action bar "text" [to players]`. Slots: 0 text, 1 players.
fn action_bar(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let text = slot(cx, m, 0)?.to_text();
    for (uuid, _) in players_in(cx, m, 1)? {
        cx.host.actions().action_bar(uuid, &text);
    }
    next()
}

/// `send title "a" [with subtitle "b"] [to players] [for T] [with fade in F] [and fade out O]`.
/// Slots: 0 title, 1 subtitle, 2 players, 3 stay, 4 fade in, 5 fade out.
fn send_title(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let title = slot(cx, m, 0)?.to_text();
    let subtitle = optional_text(cx, m, 1)?;
    timed_title(cx, m, &title, &subtitle, [2, 3, 4, 5])
}

/// `send subtitle "b" [to players] [for T] ...`. Slots: 0 subtitle, 1 players, 2 stay, 3 fade in,
/// 4 fade out.
fn send_subtitle(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let subtitle = slot(cx, m, 0)?.to_text();
    timed_title(cx, m, "", &subtitle, [1, 2, 3, 4])
}

/// Shows the title on each player. `slots` are the players, stay, fade in and fade out slots.
fn timed_title(
    cx: &mut Ctx<'_>,
    m: &Matched,
    title: &str,
    subtitle: &str,
    [players, stay, fade_in, fade_out]: [usize; 4],
) -> Result<Flow, RunError> {
    let targets = players_in(cx, m, players)?;
    let stay = ticks_in(cx, m, stay, STAY)?;
    let fade_in = ticks_in(cx, m, fade_in, FADE_IN)?;
    let fade_out = ticks_in(cx, m, fade_out, FADE_OUT)?;
    for (uuid, _) in targets {
        cx.host
            .actions()
            .title(uuid, title, subtitle, fade_in, stay, fade_out);
    }
    next()
}

/// `make players say "text"`, `force players to send the message "text"`: each text goes into
/// the chat as the player. Slots: 0 players, 1 texts.
fn make_say(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let players = players_in(cx, m, 0)?;
    let texts = texts_in(cx, m, 1)?;
    for (uuid, _) in players {
        for text in &texts {
            cx.host.actions().chat_as(uuid, text);
        }
    }
    next()
}

/// A text slot that may be empty: the empty text when it is.
fn optional_text(cx: &mut Ctx<'_>, m: &Matched, n: usize) -> Result<String, RunError> {
    Ok(match slot(cx, m, n)? {
        Value::Null => String::new(),
        v => v.to_text(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_title_times_are_bukkits() {
        assert_eq!((FADE_IN, STAY, FADE_OUT), (10, 70, 20));
    }
}
