//! The console: `execute console command`, `make console execute`, `send ... to console` and
//! `set weather`. Patterns follow Skript's `EffExecuteCommand` (console form), `EffSend` (to the
//! console) and `EffWeather`. Commands run as the console, as vanilla commands without the `/`;
//! `send to console` writes to the server log.
//!
//! They are registered before the messages and builtins (priority -2 and -1) because `to console`
//! and `set weather to` would otherwise read as a player named `console` and a variable.

use super::{console, next, slot, texts_in};
use crate::horizon_skript_core::error::RunError;
use crate::horizon_skript_core::pattern::PatternError;
use crate::horizon_skript_core::registry::{Flow, Matched, Registry};
use crate::horizon_skript_core::run::Ctx;

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
    reg.add_effect(
        "log",
        &["log %texts% [(to|in) [file[s]] %-texts%] [with [the|a] severity [of] (1¦warning|2¦severe)]"],
        -1,
        log,
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

/// `log "x" [to file "name"]`: a line in the server log, or one `[date] message` line in each
/// named file (`.log` appended, folders refused). `server.log` is the server log.
fn log(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let files = texts_in(cx, m, 1)?;
    let severity = if m.has_mark(1) {
        " WARNING"
    } else if m.has_mark(2) {
        " SEVERE"
    } else {
        ""
    };
    for line in texts_in(cx, m, 0)? {
        if files.is_empty() {
            cx.host.log(&line);
            continue;
        }
        for file in &files {
            let mut name = file.to_lowercase();
            if !name.ends_with(".log") {
                name.push_str(".log");
            }
            if name == "server.log" {
                cx.host.log(&line);
                continue;
            }
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                return Err(RunError::new(format!("the log file '{file}' must not name a folder")));
            }
            let stamp = format_stamp(cx.host.now_millis());
            if !cx.host.append_log(&name, &format!("[{stamp}{severity}] {line}")) {
                return Err(RunError::new(format!("cannot write to the log file '{name}'")));
            }
        }
    }
    next()
}

/// `YYYY-MM-DD HH:MM:SS` (UTC) of a time in milliseconds since the epoch.
fn format_stamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from the day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
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
    use crate::horizon_skript_core::host::Recorder;
    use crate::horizon_skript_core::syntax::support::testing::{acted, run};

    #[test]
    fn weather_words_map_to_vanilla_names() {
        assert_eq!(weather_word("clear").unwrap(), "clear");
        assert_eq!(weather_word("rain").unwrap(), "rain");
        assert!(weather_word("snow").is_err());
    }

    #[test]
    fn log_to_a_file_adds_the_extension_and_a_stamp() {
        let mut host = Recorder::default();
        host.now = 86_400_000 + 3_661_000;
        let logs = run(&host, "\tlog \"hello\" to file \"Mine\" with severity warning\n");
        assert!(logs.is_empty(), "{logs:?}");
        assert_eq!(
            acted(&host),
            vec!["logfile mine.log [1970-01-02 01:01:01 WARNING] hello"]
        );
    }

    #[test]
    fn log_without_a_file_goes_to_the_server_log() {
        let host = Recorder::default();
        let logs = run(&host, "\tlog \"plain\"\n");
        assert_eq!(logs, vec!["plain".to_owned()]);
    }

    #[test]
    fn a_log_file_with_a_folder_is_refused() {
        let host = Recorder::default();
        let logs = run(&host, "\tlog \"x\" to file \"../evil\"\n");
        assert!(logs.iter().any(|l| l.contains("folder")), "{logs:?}");
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
