//! The console: `execute console command`, `make console execute`, `send ... to console` and
//! `set weather`. Patterns follow Skript's `EffExecuteCommand` (console form), `EffSend` (to the
//! console) and `EffWeather`. Commands run as the console, as vanilla commands without the `/`;
//! `send to console` writes to the server log.
//!
//! They are registered before the messages and builtins (priority -2 and -1) because `to console`
//! and `set weather to` would otherwise read as a player named `console` and a variable.

use super::{console, next, slot, texts_in};
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;

pub(super) fn register(reg: &mut Registry) -> Result<(), PatternError> {
    reg.add_effect(
        "execute console command",
        &[
            "execute console command[s] %texts%",
            "make console execute[s] %texts%",
        ],
        -2,
        execute_console,
    )?;
    reg.add_effect(
        "send to console",
        &["(send|log|print) %texts% to console"],
        -2,
        send_to_console,
    )?;
    reg.add_effect("set weather", &["set weather to %text%"], -1, set_weather)?;
    Ok(())
}

/// Each text is one vanilla command for the console.
fn execute_console(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for line in texts_in(cx, m, 0)? {
        console(cx, &line);
    }
    next()
}

fn send_to_console(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    for line in texts_in(cx, m, 0)? {
        cx.host.log(&line);
    }
    next()
}

/// `set weather to clear` or `rain` or `thunder`: the vanilla `weather` command, for the default
/// world (the command takes no world). Other words are an error.
fn set_weather(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let word = slot(cx, m, 0)?.to_text().trim().to_ascii_lowercase();
    let weather = weather_word(&word)?;
    console(cx, &format!("weather {weather}"));
    next()
}

/// The vanilla weather names: `clear`, `rain` and `thunder`.
pub(crate) fn weather_word(word: &str) -> Result<&'static str, RunError> {
    match word {
        "clear" | "sunny" => Ok("clear"),
        "rain" | "rainy" | "raining" => Ok("rain"),
        "thunder" | "storm" | "thundering" => Ok("thunder"),
        other => Err(RunError::new(format!("unknown weather {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Recorder;
    use crate::syntax::support::testing::{acted, run};

    #[test]
    fn weather_words_map_to_vanilla_names() {
        assert_eq!(weather_word("clear").unwrap(), "clear");
        assert_eq!(weather_word("rain").unwrap(), "rain");
        assert!(weather_word("snow").is_err());
    }

    #[test]
    fn console_commands_run_as_the_console() {
        let host = Recorder::default();
        let logs = run(
            &host,
            "\texecute console command \"weather clear 1000000\"\n\tmake console execute \"say hi\"\n",
        );
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&host), vec!["console weather clear 1000000", "console say hi"]);
    }

    #[test]
    fn send_to_console_writes_the_log() {
        let host = Recorder::default();
        let logs = run(&host, "\tsend \"[stage] built\" to console\n");
        // The log is the host's log: the one line the script wrote is all there is.
        assert_eq!(logs, vec!["[stage] built".to_owned()]);
        assert!(host.sent.borrow().is_empty());
    }

    #[test]
    fn set_weather_runs_the_weather_command() {
        let host = Recorder::default();
        let logs = run(&host, "\tset weather to \"rain\"\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(acted(&host), vec!["console weather rain"]);
    }
}
