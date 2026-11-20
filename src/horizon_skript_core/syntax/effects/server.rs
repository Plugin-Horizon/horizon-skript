//! The server: `stop the server`, `save world`, `enforce the whitelist`, `set weather to rain`,
//! `send message` and `wait for`. Patterns follow Skript's `EffStopServer`, `EffWorldSave`,
//! `EffEnforceWhitelist`, `EffWeather`, `EffMessage` and `Delay`.
//!
//! `restart the server` and `unenforce the whitelist` have no vanilla command and are not here;
//! `save` flushes every world, as the command takes no world.

use super::{console, next, players_in, slot, texts_in};
use crate::horizon_skript_core::builtins::ticks_of;
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;
use crate::horizon_skript_core::value::Value;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "stop server",
        &["(stop|shut[ ]down) [the] server"],
        0,
        stop_server,
    )?;
    reg.add_effect(
        "save world",
        &["save [[the] world[s] ]%worlds%"],
        0,
        save_world,
    )?;
    reg.add_effect(
        "enforce whitelist",
        &["enforce[ the][ server] white[ ]list"],
        0,
        enforce_whitelist,
    )?;
    // Before `set weather to %text%`, which would read the bare word as a variable.
    reg.add_effect(
        "set weather word",
        &["(set|change) [the] weather to <[a-zA-Z]+>"],
        -2,
        set_weather_word,
    )?;
    reg.add_effect(
        "send message",
        &["send message[s] %texts% [to %-players%]"],
        -2,
        send_message,
    )?;
    reg.add_effect(
        "wait for",
        &["wait for %timespan%", "halt [for] %timespan%"],
        0,
        wait_for,
    )?;
    reg.add_effect(
        "script file",
        &[
            "(1¦(enable|load)|2¦reload|3¦disable|4¦unload) script [file|named] %text% [print:with errors]",
            "(1¦(enable|load)|2¦reload|3¦disable|4¦unload) skript file %text% [print:with errors]",
        ],
        -1,
        script_file,
    )?;
    Ok(())
}

/// `reload script "name"` and the others: the glue does it when the running trigger is done.
/// Skript's `unload` and `disable` both switch the script off here.
fn script_file(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let action = if m.has_mark(1) {
        "enable"
    } else if m.has_mark(2) {
        "reload"
    } else if m.has_mark(3) {
        "disable"
    } else {
        "unload"
    };
    for name in texts_in(cx, m, 0)? {
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err(RunError::new(format!("the script '{name}' must not name a folder")));
        }
        if !cx.host.script_action(action, &name) {
            return Err(RunError::new("this server cannot change scripts from a script"));
        }
    }
    next()
}

fn stop_server(cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    console(cx, "stop");
    next()
}

/// `save world "x"`: the vanilla `save-all flush`, which saves every world.
fn save_world(cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    console(cx, "save-all flush");
    next()
}

/// `enforce the whitelist`: reloads the list, which kicks every player who is not on it.
fn enforce_whitelist(cx: &mut Ctx<'_>, _: &Matched) -> Result<Flow, RunError> {
    console(cx, "whitelist reload");
    next()
}

fn set_weather_word(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let word = m.regex(0).unwrap_or("").to_ascii_lowercase();
    let weather = super::console::weather_word(&word)?;
    console(cx, &format!("weather {weather}"));
    next()
}

/// `send message "x" to players`: the same as `send`.
fn send_message(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let texts = texts_in(cx, m, 0)?;
    for (uuid, _) in players_in(cx, m, 1)? {
        for text in &texts {
            cx.host.send(uuid, text);
        }
    }
    next()
}

fn wait_for(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    match slot(cx, m, 0)? {
        Value::Timespan(ms) => Ok(Flow::Wait(ticks_of(ms))),
        other => Err(RunError::new(format!(
            "wait needs a timespan, got {}",
            other.class()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    #[test]
    fn script_effects_ask_the_host() {
        let host = Recorder::default();
        let logs = run(
            &host,
            "\treload script \"lobby\"\n\tdisable script \"old\"\n\tenable script file \"new\"\n\tunload skript file \"x.sk\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "script reload lobby",
                "script disable old",
                "script enable new",
                "script unload x.sk"
            ]
        );
    }

    #[test]
    fn a_script_path_with_a_folder_is_refused() {
        let host = Recorder::default();
        let logs = run(&host, "\treload script \"../x\"\n");
        assert!(logs.iter().any(|l| l.contains("folder")), "{logs:?}");
    }

    #[test]
    fn server_commands() {
        let host = Recorder::default();
        let logs = run(
            &host,
            "\tsave world \"world\"\n\tenforce the whitelist\n\tset weather to rain\n\tstop the server\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec![
                "console save-all flush",
                "console whitelist reload",
                "console weather rain",
                "console stop",
            ]
        );
    }

    #[test]
    fn send_message_goes_to_the_named_players() {
        let host = Recorder::with_players(&[(1, "Alice")]);
        let logs = run(&host, "\tsend message \"hi\" to Alice\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(*host.sent.borrow(), vec![(1, "hi".to_owned())]);
    }
}
