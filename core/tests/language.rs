//! Script-level tests: small `.sk` sources are loaded through the manager, fired or ticked, and
//! checked against what the host received and the variables they left.

use horizon_skript_core::host::Recorder;
use horizon_skript_core::run::Event;
use horizon_skript_core::value::Value;
use horizon_skript_core::{Manager, Registry};
use std::fs;
use std::path::PathBuf;

const ALICE: u128 = 1;

struct Harness {
    m: Manager,
    host: Recorder,
}

impl Harness {
    /// Loads one enabled script named `test.sk`; panics on parse errors.
    fn new(src: &str) -> Harness {
        let mut h = Harness::empty();
        h.load("test.sk", src).expect("the script parses");
        h
    }

    fn empty() -> Harness {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused");
        Harness {
            m: Manager::new(Registry::new(), dir),
            host: Recorder::with_players(&[(ALICE, "Alice"), (2, "Bob")]),
        }
    }

    fn load(&mut self, key: &str, src: &str) -> Result<(), Vec<String>> {
        self.m
            .load_source(&self.host, key, src, true)
            .map_err(|errs| errs.iter().map(|e| e.to_string()).collect())
    }

    fn fire(&mut self, event: &str, data: Event) {
        self.m.fire(&self.host, event, data);
    }

    fn tick(&mut self, n: usize) {
        for _ in 0..n {
            self.m.tick(&self.host);
        }
    }

    fn broadcasts(&self) -> Vec<String> {
        self.host.broadcasts.borrow().clone()
    }

    fn sent(&self) -> Vec<(u128, String)> {
        self.host.sent.borrow().clone()
    }

    fn logs(&self) -> Vec<String> {
        self.host.logs.borrow().clone()
    }

    fn var(&self, name: &str) -> Option<Value> {
        self.m.variables().get(name).cloned()
    }
}

fn player_event() -> Event {
    Event {
        player: Some(Value::Player {
            uuid: ALICE,
            name: "Alice".to_owned(),
        }),
        ..Event::default()
    }
}

#[test]
fn arithmetic_and_interpolation() {
    let h = Harness::new(
        "on load:\n\
         \tset {a} to 2 + 3 * 4\n\
         \tset {b} to (2 + 3) * 4\n\
         \tbroadcast \"a=%{a}% b=%{b}% pow=%2 ^ 3% half=%7 / 2%\"\n",
    );
    assert_eq!(h.var("a"), Some(Value::Int(14)));
    assert_eq!(h.var("b"), Some(Value::Int(20)));
    assert_eq!(h.broadcasts(), vec!["a=14 b=20 pow=8 half=3.5"]);
}

#[test]
fn percent_and_text_escapes() {
    let h = Harness::new("on load:\n\tbroadcast \"100%% done\"\n");
    assert_eq!(h.broadcasts(), vec!["100% done"]);
}

#[test]
fn comments_and_hashes_inside_strings() {
    let h = Harness::new(
        "# a file comment\non load: # trailing\n\t# inside\n\tbroadcast \"a # b\" # gone\n",
    );
    assert_eq!(h.broadcasts(), vec!["a # b"]);
}

#[test]
fn options_substitute_into_lines() {
    let h =
        Harness::new("options:\n\tgreeting: Hello\non load:\n\tbroadcast \"{@greeting}, world\"\n");
    assert_eq!(h.broadcasts(), vec!["Hello, world"]);
}

#[test]
fn variables_lists_and_indices() {
    let h = Harness::new(
        "on load:\n\
         \tset {list::1} to \"a\"\n\
         \tadd \"b\" to {list::*}\n\
         \tadd \"c\" to {list::*}\n\
         \tloop {list::*}:\n\
         \t\tbroadcast \"%loop-index%=%loop-value%\"\n\
         \tremove \"b\" from {list::*}\n\
         \tset {copy} to {list::*}\n\
         \tset {n} to 5\n\
         \tadd 2 to {n}\n\
         \tremove 1 from {n}\n\
         \tdelete {list::*}\n\
         \tset {x::y} to \"deep\"\n\
         \tdelete {x}\n",
    );
    assert_eq!(h.broadcasts(), vec!["1=a", "2=b", "3=c"],);
    assert_eq!(h.var("n"), Some(Value::Int(6)));
    assert_eq!(h.var("list::1"), None);
    assert_eq!(h.var("copy::1"), Some(Value::Text("a".into())));
    assert_eq!(h.var("copy::2"), Some(Value::Text("c".into())));
    assert_eq!(h.var("x::y"), None);
}

#[test]
fn set_copies_a_list_into_indices_of_another() {
    let h = Harness::new(
        "on load:\n\tset {a::1} to 1\n\tset {a::2} to 2\n\tset {b} to {a::*}\n\tset {c} to 0\n",
    );
    assert_eq!(h.var("b::1"), Some(Value::Int(1)));
    assert_eq!(h.var("b::2"), Some(Value::Int(2)));
    assert_eq!(h.var("b"), None);
}

#[test]
fn local_variables_belong_to_one_run() {
    let mut h = Harness::new(
        "on load:\n\tset {_secret} to 5\n\tset {shared} to 1\non chat:\n\tbroadcast \"%{_secret}%\"\n",
    );
    h.fire(
        "chat",
        Event {
            message: Some("hi".into()),
            ..player_event()
        },
    );
    assert_eq!(h.broadcasts(), vec!["<none>"]);
    assert_eq!(h.var("shared"), Some(Value::Int(1)));
}

#[test]
fn if_else_if_else_chooses_one_branch() {
    let src = "on load:\n\
               \tif {a} is set:\n\
               \t\tbroadcast \"a\"\n\
               \telse if {b}:\n\
               \t\tbroadcast \"b\"\n\
               \telse:\n\
               \t\tbroadcast \"none\"\n";
    let h = Harness::new(src);
    assert_eq!(h.broadcasts(), vec!["none"]);
    let mut h = Harness::new(&src.replace("on load:", "options:\n\ta: 1\non load:"));
    h.m.variables_mut().set("b", Value::Bool(true));
    h.fire("nothing", Event::default());
    assert_eq!(h.broadcasts(), vec!["none"]);
}

#[test]
fn if_with_a_variable_set_earlier() {
    let h = Harness::new(
        "on load:\n\tset {flag} to true\n\tif {flag}:\n\t\tbroadcast \"yes\"\n\telse:\n\t\tbroadcast \"no\"\n",
    );
    assert_eq!(h.broadcasts(), vec!["yes"]);
}

#[test]
fn loops_exit_and_nesting() {
    let h = Harness::new(
        "on load:\n\
         \tloop 1, 2, 3:\n\
         \t\tbroadcast \"%loop-value%\"\n\
         \t\tif loop-value is set:\n\
         \t\t\texit loop\n\
         \tbroadcast \"after\"\n",
    );
    assert_eq!(h.broadcasts(), vec!["1", "after"]);
}

#[test]
fn exit_two_loops_leaves_both() {
    let h = Harness::new(
        "on load:\n\
         \tloop 1, 2:\n\
         \t\tloop 3, 4:\n\
         \t\t\tbroadcast \"%loop-value%\"\n\
         \t\t\texit 2 loops\n\
         \t\tbroadcast \"never\"\n\
         \tbroadcast \"end\"\n",
    );
    assert_eq!(h.broadcasts(), vec!["3", "end"]);
}

#[test]
fn while_runs_until_its_condition_is_gone() {
    let h = Harness::new(
        "on load:\n\
         \tset {w} to true\n\
         \twhile {w}:\n\
         \t\tbroadcast \"once\"\n\
         \t\tset {w} to false\n\
         \tbroadcast \"done\"\n",
    );
    assert_eq!(h.broadcasts(), vec!["once", "done"]);
}

#[test]
fn stop_ends_the_trigger_and_return_ends_a_function() {
    let h = Harness::new("on load:\n\tbroadcast \"a\"\n\tstop\n\tbroadcast \"b\"\n");
    assert_eq!(h.broadcasts(), vec!["a"]);
    let h = Harness::new(
        "function pick(flag: boolean) :: text:\n\tif {_flag}:\n\t\treturn \"yes\"\n\treturn \"no\"\n\
         on load:\n\tbroadcast pick(false)\n\tbroadcast pick(true)\n",
    );
    assert_eq!(
        h.broadcasts(),
        vec!["no"]
            .into_iter()
            .chain(std::iter::empty())
            .collect::<Vec<_>>()
            .iter()
            .map(|_| "no")
            .take(0)
            .chain(["no", "yes"])
            .map(String::from)
            .collect::<Vec<_>>()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn functions_take_and_return_values() {
    let h = Harness::new(
        "function double(n: integer) :: integer:\n\treturn {_n} * 2\n\
         function greet(name: text):\n\tbroadcast \"hi %{_name}%\"\n\
         on load:\n\tbroadcast \"%double(21)%\"\n\tgreet(\"bob\")\n",
    );
    assert_eq!(h.broadcasts(), vec!["42", "hi bob"]);
}

#[test]
fn a_function_calling_itself_forever_is_an_error_with_a_line() {
    let h = Harness::new("function again():\n\tagain()\non load:\n\tagain()\n");
    let logs = h.logs();
    assert!(
        logs.iter()
            .any(|l| l.contains("test.sk, line 4") && l.contains("too deeply")),
        "{logs:?}"
    );
}

#[test]
fn a_function_cannot_wait() {
    let h = Harness::new(
        "function slow():\n\twait 1 second\non load:\n\tslow()\n\tbroadcast \"not reached\"\n",
    );
    assert!(h.logs().iter().any(|l| l.contains("functions cannot wait")));
    assert!(h.broadcasts().is_empty());
}

#[test]
fn wait_resumes_on_the_right_tick() {
    let mut h = Harness::new("on load:\n\tbroadcast \"a\"\n\twait 1 second\n\tbroadcast \"b\"\n");
    assert_eq!(h.broadcasts(), vec!["a"]);
    h.tick(19);
    assert_eq!(h.broadcasts(), vec!["a"]);
    h.tick(1);
    assert_eq!(h.broadcasts(), vec!["a", "b"]);
}

#[test]
fn wait_takes_ticks_and_minutes() {
    let mut h = Harness::new("on load:\n\twait 2 ticks\n\tbroadcast \"x\"\n");
    h.tick(1);
    assert!(h.broadcasts().is_empty());
    h.tick(1);
    assert_eq!(h.broadcasts(), vec!["x"]);
}

#[test]
fn a_long_loop_yields_to_the_next_tick() {
    let mut h =
        Harness::new("on load:\n\tset {c} to 0\n\twhile {c} is set:\n\t\tset {c} to {c} + 1\n");
    let first = h.var("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert!(first > 0, "the loop ran during load");
    h.tick(3);
    let later = h.var("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert!(later > first, "the loop went on after the tick");
}

#[test]
fn join_and_chat_events_carry_their_data() {
    let mut h = Harness::new(
        "on join:\n\tsend \"Welcome %player%!\" to player\n\
         on chat:\n\tbroadcast \"<%player%> %message%\"\n",
    );
    h.fire("join", player_event());
    h.fire(
        "chat",
        Event {
            message: Some("hello".into()),
            ..player_event()
        },
    );
    assert_eq!(h.sent(), vec![(ALICE, "Welcome Alice!".to_owned())]);
    assert_eq!(h.broadcasts(), vec!["<Alice> hello"]);
    h.tick(1);
}

#[test]
fn quit_and_leave_are_the_same_event() {
    let h = Harness::new("on leave:\n\tbroadcast \"bye\"\n");
    let mut h = h;
    h.fire("quit", player_event());
    assert_eq!(h.broadcasts(), vec!["bye"]);
}

#[test]
fn commands_take_their_arguments() {
    let mut h = Harness::new("command /greet <text>:\n\tsend \"Hi %arg-1%\" to player\n");
    assert_eq!(h.m.command_names(), vec!["greet".to_owned()]);
    let ran = h.m.dispatch_command(
        &h.host,
        "greet",
        Event {
            args: vec!["Bob".into()],
            ..player_event()
        },
    );
    assert!(ran);
    assert_eq!(h.sent(), vec![(ALICE, "Hi Bob".to_owned())]);
    assert!(!h.m.dispatch_command(&h.host, "nothing", Event::default()));
    h.tick(0);
}

#[test]
fn every_repeats_on_its_interval() {
    let mut h = Harness::new("every 2 ticks:\n\tbroadcast \"t\"\n");
    h.tick(6);
    assert_eq!(h.broadcasts().len(), 3);
}

#[test]
fn a_bad_line_is_reported_with_file_and_line() {
    let h = Harness::empty();
    let mut h = h;
    let errors = h
        .load(
            "bad.sk",
            "on load:\n\tbroadcast \"ok\"\n\tfrobnicate the thing\n",
        )
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].starts_with("bad.sk, line 3: Can't understand this effect"),
        "{}",
        errors[0]
    );
}

#[test]
fn indentation_mistakes_are_parse_errors() {
    let mut h = Harness::empty();
    let errors = h
        .load(
            "ind.sk",
            "on load:\n  broadcast \"a\"\n     broadcast \"b\"\n",
        )
        .unwrap_err();
    assert!(
        errors.iter().any(|e| e.starts_with("ind.sk, line 3")),
        "{errors:?}"
    );
}

#[test]
fn a_runtime_error_names_its_line() {
    let h = Harness::new("on load:\n\tbroadcast \"x\"\n\tset {a} to 1 / 0\n\tbroadcast \"y\"\n");
    assert_eq!(h.broadcasts(), vec!["x"]);
    assert!(
        h.logs()
            .iter()
            .any(|l| l == "[Skript] test.sk, line 3: division by zero"),
        "{:?}",
        h.logs()
    );
}

#[test]
fn timespans_read_and_print() {
    let h = Harness::new("on load:\n\tset {t} to 1 minute + 30 seconds\n\tbroadcast \"%{t}%\"\n");
    assert_eq!(h.var("t"), Some(Value::Timespan(90_000)));
    assert_eq!(h.broadcasts(), vec!["1 minute, 30 seconds"]);
}

#[test]
fn players_are_looked_up_by_name() {
    let mut h = Harness::new("on load:\n\tsend \"psst\" to Bob\n");
    h.tick(0);
    assert_eq!(h.sent(), vec![(2, "psst".to_owned())]);
    let errors = {
        let mut other = Harness::empty();
        other
            .load("p.sk", "on load:\n\tsend \"x\" to Nobody\n")
            .unwrap();
        other.logs()
    };
    assert!(
        errors
            .iter()
            .any(|l| l.contains("there is no player named Nobody")),
        "{errors:?}"
    );
}

#[test]
fn options_and_functions_are_shared_between_files() {
    let mut h = Harness::empty();
    h.load(
        "lib.sk",
        "function shout(text: text) :: text:\n\treturn \"%upper%\"\n",
    )
    .unwrap_err();
    h.load("lib.sk", "function shout(text: text) :: text:\n\treturn \"!!\"\non load:\n\tbroadcast shout(\"x\")\n")
        .unwrap();
    assert_eq!(h.broadcasts(), vec!["!!"]);
}

#[test]
fn skript_command_lists_enables_and_disables() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("skript-cmd");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("a.sk"), "on load:\n\tbroadcast \"a loaded\"\n").unwrap();
    fs::write(dir.join("sub/b.sk"), "every 1 tick:\n\tbroadcast \"b\"\n").unwrap();
    fs::write(dir.join("-off.sk"), "on load:\n\tbroadcast \"never\"\n").unwrap();
    let mut m = Manager::new(Registry::new(), &dir);
    let host = Recorder::default();
    m.load_dir(&host);
    assert_eq!(host.broadcasts.borrow().clone(), vec!["a loaded"]);
    assert_eq!(m.script_names(), vec!["-off.sk", "a.sk", "sub/b.sk"]);
    let list = m.skript_command(&host, "list");
    assert!(list.contains(&"-off.sk: disabled".to_owned()), "{list:?}");
    assert!(list.contains(&"sub/b.sk: enabled".to_owned()), "{list:?}");

    m.skript_command(&host, "enable -off");
    assert_eq!(
        host.broadcasts.borrow().last().map(String::as_str),
        Some("never")
    );
    m.skript_command(&host, "disable a");
    m.tick(&host);
    assert_eq!(
        host.broadcasts
            .borrow()
            .iter()
            .filter(|b| *b == "b")
            .count(),
        1
    );
    assert!(m.skript_command(&host, "enable nope")[0].contains("No script named nope"));

    fs::write(dir.join("a.sk"), "on load:\n\tbroadcast \"a again\"\n").unwrap();
    m.skript_command(&host, "reload a");
    m.skript_command(&host, "enable a");
    assert_eq!(
        host.broadcasts.borrow().last().map(String::as_str),
        Some("a again")
    );
    let info = m.skript_command(&host, "info");
    assert!(info[0].starts_with("Scripts: 3 loaded"), "{info:?}");
}

#[test]
fn clear_empties_a_list_but_keeps_the_name() {
    let h = Harness::new(
        "on load:\n\tset {keep} to 1\n\tadd 1 to {l::*}\n\tadd 2 to {l::*}\n\tclear {l::*}\n\tset {l::9} to 9\n",
    );
    assert_eq!(h.var("l::1"), None);
    assert_eq!(h.var("l::9"), Some(Value::Int(9)));
    assert_eq!(h.var("keep"), Some(Value::Int(1)));
}
