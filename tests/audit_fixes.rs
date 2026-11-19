//! Tests for the fixes of the parity audit (see the commit messages): permission nodes of scripts,
//! command headers and arguments, partial loading, `variables:` and `aliases:`.

use mod_skript::horizon_skript_core::manager::permission_nodes_in;

#[test]
fn permission_nodes_of_a_script_are_found() {
    let src = "command /a:\n\
               \tpermission: coresmp.a.admin\n\
               \tpermission message: <red>No</red>\n\
               \ttrigger:\n\
               \t\tif player has permission \"x.y\":\n\
               \t\t\tstop\n\
               \t\tif player has permission \"%{_n}%\":\n\
               \t\t\tstop\n\
               # permission: no.comment\n";
    assert_eq!(
        permission_nodes_in(src),
        vec!["coresmp.a.admin".to_owned(), "x.y".to_owned()]
    );
}

use mod_skript::horizon_skript_core::host::Recorder;
use mod_skript::horizon_skript_core::manager::{CommandSender, Manager};
use mod_skript::horizon_skript_core::parse::{ArgSpec, ExecutableBy, HeaderPart, parse_command_header};
use mod_skript::horizon_skript_core::run::Event;
use mod_skript::horizon_skript_core::value::Value;
use mod_skript::horizon_skript_core::Registry;
use std::path::PathBuf;

const ALICE: u128 = 1;

struct H {
    m: Manager,
    host: Recorder,
}

impl H {
    fn new(src: &str) -> H {
        let mut h = H {
            m: Manager::new(Registry::new(), PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused")),
            host: Recorder::with_players(&[(ALICE, "Alice")]),
        };
        h.m.load_source(&h.host, "t.sk", src, true)
            .unwrap_or_else(|e| panic!("the script parses: {e:?}"));
        h
    }

    fn console(&mut self, name: &str, rest: &str) -> bool {
        let sender = CommandSender { player: None, op: true };
        self.m.run_command(&self.host, name, &sender, rest)
    }

    fn player(&mut self, name: &str, rest: &str) -> bool {
        let sender = CommandSender {
            player: Some((ALICE, "Alice".to_owned())),
            op: false,
        };
        self.m.run_command(&self.host, name, &sender, rest)
    }

    fn broadcasts(&self) -> Vec<String> {
        self.host.broadcasts.borrow().clone()
    }

    fn logs(&self) -> Vec<String> {
        self.host.logs.borrow().clone()
    }

    fn sent(&self) -> Vec<String> {
        self.host.sent.borrow().iter().map(|(_, t)| t.clone()).collect()
    }

    fn var(&self, name: &str) -> Option<Value> {
        self.m.variables().get(name).cloned()
    }
}

fn arg(class: &'static str, optional: bool, default: Option<&str>) -> HeaderPart {
    HeaderPart::Arg(ArgSpec {
        name: None,
        class,
        plural: false,
        optional,
        default: default.map(str::to_owned),
    })
}

#[test]
fn a_command_header_gives_its_words_and_arguments() {
    let (name, parts, tail) = parse_command_header("Greet <text> [<number=1>] extra").unwrap();
    assert_eq!(name, "greet");
    assert_eq!(
        parts,
        vec![
            arg("text", false, None),
            arg("number", true, Some("1")),
            HeaderPart::Word { text: "extra".to_owned(), optional: false },
        ]
    );
    assert_eq!(tail, "<text> [<number=1>] extra");
    let (_, named, _) = parse_command_header("x <who: player>").unwrap();
    assert_eq!(
        named,
        vec![HeaderPart::Arg(ArgSpec {
            name: Some("who".to_owned()),
            class: "player",
            plural: false,
            optional: false,
            default: None,
        })]
    );
    assert!(parse_command_header("x<text>").unwrap_err().contains("Literals cannot"));
    assert!(parse_command_header("x <thing>").unwrap_err().contains("not a type"));
}

#[test]
fn the_console_runs_a_script_command() {
    let mut h = H::new("command /hello <text>:\n\ttrigger:\n\t\tbroadcast \"hi %arg-1%\"\n");
    assert!(h.console("hello", "world"));
    assert_eq!(h.broadcasts(), vec!["hi world".to_owned()]);
    assert!(!h.console("nothing", ""));
}

#[test]
fn a_wrong_argument_list_prints_the_usage_and_runs_nothing() {
    let mut h = H::new("command /hello <text>:\n\ttrigger:\n\t\tbroadcast \"ran\"\n");
    assert!(h.console("hello", ""));
    assert_eq!(h.logs(), vec!["/hello <text>".to_owned()]);
    assert!(h.broadcasts().is_empty());
    let mut custom = H::new("command /hello <text>:\n\tusage: Say hello to somebody\n\ttrigger:\n\t\tbroadcast \"ran\"\n");
    assert!(custom.player("hello", ""));
    assert_eq!(custom.sent(), vec!["Say hello to somebody".to_owned()]);
}

#[test]
fn number_arguments_are_numbers_and_bad_ones_are_refused() {
    let mut h = H::new("command /pay <number>:\n\ttrigger:\n\t\tset {paid} to arg-1 * 2\n");
    assert!(h.console("pay", "21"));
    assert_eq!(h.var("paid"), Some(Value::Int(42)));
    assert!(h.console("pay", "abc"));
    assert_eq!(
        h.logs(),
        vec!["\u{a7}4'abc' is not a valid number.".to_owned(), "/pay <number>".to_owned()]
    );
    assert_eq!(h.var("paid"), Some(Value::Int(42)));
}

#[test]
fn optional_arguments_take_their_default_or_stay_unset() {
    let mut h = H::new(
        "command /n [<number=3>] [<text>]:\n\ttrigger:\n\t\tset {n} to arg-1\n\t\tif arg-2 is set:\n\t\t\tbroadcast \"second\"\n",
    );
    assert!(h.console("n", ""));
    assert_eq!(h.var("n"), Some(Value::Int(3)));
    assert!(h.broadcasts().is_empty());
    assert!(h.console("n", "7 some words"));
    assert_eq!(h.var("n"), Some(Value::Int(7)));
    assert_eq!(h.broadcasts(), vec!["second".to_owned()]);
}

#[test]
fn only_the_last_text_argument_takes_the_rest_of_the_line() {
    let mut h = H::new(
        "command /npcaura <text> [<text>]:\n\ttrigger:\n\t\tif arg-2 is set:\n\t\t\tbroadcast \"%arg-1%|%arg-2%\"\n\t\telse:\n\t\t\tbroadcast \"only %arg-1%\"\n",
    );
    assert!(h.player("npcaura", "set coral reef"));
    assert!(h.player("npcaura", "list"));
    assert_eq!(
        h.broadcasts(),
        vec!["set|coral reef".to_owned(), "only list".to_owned()]
    );
}

#[test]
fn a_named_argument_is_a_local_variable() {
    let mut h = H::new("command /w <who: text>:\n\ttrigger:\n\t\tset {got} to {_who}\n");
    assert!(h.console("w", "Bob"));
    assert_eq!(h.var("got"), Some(Value::Text("Bob".to_owned())));
}

#[test]
fn executable_by_limits_the_sender() {
    let mut h = H::new(
        "command /p:\n\texecutable by: players\n\ttrigger:\n\t\tbroadcast \"ran\"\n",
    );
    assert!(h.console("p", ""));
    assert_eq!(h.logs(), vec!["This command can only be used by players".to_owned()]);
    assert!(h.broadcasts().is_empty());
    assert!(h.player("p", ""));
    assert_eq!(h.broadcasts(), vec!["ran".to_owned()]);
}

#[test]
fn a_cooldown_blocks_a_second_run_until_it_is_over() {
    let mut h = H::new(
        "command /c:\n\tcooldown: 10 seconds\n\tcooldown message: <red>Wait!</red>\n\ttrigger:\n\t\tbroadcast \"ran\"\n",
    );
    h.host.now = 1_000;
    assert!(h.player("c", ""));
    h.host.now = 6_000;
    assert!(h.player("c", ""));
    assert_eq!(h.broadcasts().len(), 1);
    assert_eq!(h.sent(), vec!["\u{a7}cWait!\u{a7}r".to_owned()]);
    h.host.now = 11_000;
    assert!(h.player("c", ""));
    assert_eq!(h.broadcasts().len(), 2);
}

#[test]
fn aliases_run_the_command_and_are_registered() {
    let mut h = H::new("command /greet:\n\taliases: /hi, Hey\n\ttrigger:\n\t\tbroadcast \"hello\"\n");
    assert_eq!(h.m.command_registrations(), vec!["greet", "hey", "hi"]);
    assert!(h.console("hey", ""));
    assert!(h.console("hi", ""));
    assert_eq!(h.broadcasts().len(), 2);
}

#[test]
fn a_trigger_with_an_error_is_left_out_and_the_rest_loads() {
    let mut m = Manager::new(Registry::new(), PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused"));
    let host = Recorder::with_players(&[(ALICE, "Alice")]);
    let errors = m
        .load_source(
            &host,
            "p.sk",
            "on join:\n\tbroadcast \"ok\"\non quit:\n\tfrobnicate\n",
            true,
        )
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 4);
    m.fire(&host, "join", Event::default());
    m.fire(&host, "quit", Event::default());
    assert_eq!(host.broadcasts.borrow().clone(), vec!["ok".to_owned()]);
}

#[test]
fn variables_set_defaults_only_for_unset_variables() {
    let mut m = Manager::new(Registry::new(), PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused"));
    let host = Recorder::default();
    m.variables_mut().set("title", Value::Text("kept".to_owned()));
    m.load_source(
        &host,
        "v.sk",
        "variables:\n\t{score} = 5\n\t{title} = \"none\"\non join:\n\tadd 1 to {score}\n",
        true,
    )
    .unwrap();
    assert_eq!(m.variables().get("score"), Some(&Value::Int(5)));
    assert_eq!(m.variables().get("title"), Some(&Value::Text("kept".to_owned())));
    m.fire(&host, "join", Event::default());
    assert_eq!(m.variables().get("score"), Some(&Value::Int(6)));
}

#[test]
fn aliases_name_item_types_in_their_file() {
    let mut h = H::new(
        "aliases:\n\tblood = red dye\non join:\n\tgive 3 blood to Alice\n",
    );
    h.m.fire(&h.host, "join", Event::default());
    assert_eq!(
        h.host.acted.borrow().clone(),
        vec!["console give Alice minecraft:red_dye 3".to_owned()]
    );
}

#[test]
fn using_local_functions_and_priorities_are_accepted() {
    let mut h = H::new(
        "using script reflection\non join with priority high:\n\tbroadcast \"p%f()%\"\nlocal function f() :: number:\n\treturn 1\n",
    );
    h.m.fire(&h.host, "join", Event::default());
    assert_eq!(h.broadcasts(), vec!["p1".to_owned()]);
}

#[test]
fn a_chat_trigger_can_change_the_message_and_cancel() {
    let mut h = H::new("on chat:\n\tset message to \"changed\"\n\tcancel event\n");
    let data = Event {
        message: Some("hello".to_owned()),
        ..Event::default()
    };
    let probe = data.clone();
    h.m.fire(&h.host, "chat", data);
    assert_eq!(probe.changed_message(), Some("changed".to_owned()));
    assert!(probe.is_cancelled());
}

#[test]
fn the_skript_command_has_help_reload_variants_and_completions() {
    let mut h = H::new("on join:\n\tbroadcast \"x\"\n");
    let help = h.m.skript_command(&h.host, "");
    assert!(help[0].contains("Skript's main command"), "{help:?}");
    assert!(h.m.skript_command(&h.host, "bogus")[0].contains("Skript's main command"));
    let usage = h.m.skript_command(&h.host, "reload");
    assert!(usage[0].starts_with("Usage:"), "{usage:?}");
    let reloaded = h.m.skript_command(&h.host, "reload nothing.sk");
    assert!(reloaded[0].contains("Can't find the script"), "{reloaded:?}");
    assert_eq!(h.m.completions("rel"), vec!["reload".to_owned()]);
    let all = h.m.completions("reload ");
    assert!(all.contains(&"all".to_owned()) && all.contains(&"t.sk".to_owned()), "{all:?}");
    assert_eq!(h.m.completions("enable t"), vec!["t.sk".to_owned()]);
}

#[test]
fn executable_by_reads_its_list() {
    let h = H::new("command /e:\n\texecutable by: players and the console\n\ttrigger:\n\t\tbroadcast \"x\"\n");
    let infos = h.m.command_infos();
    assert_eq!(
        infos[0].1.executable_by,
        Some(vec![ExecutableBy::Players, ExecutableBy::Console])
    );
}
