//! Language-core tests for what the Skript 2.17.1 lobby scripts use: `loop N times` and its
//! counters, typed loops, `subtract`, command sections, the load events, `is one of`, the default
//! operator, nested `%...%`, colour tags, number accuracy, timespans and dates, case-insensitive
//! names, memory-only variables, `variables.csv` and the `config.sk` keys.

use horizon_skript_core::host::Recorder;
use horizon_skript_core::manager::{Database, Settings, settings_from_config};
use horizon_skript_core::run::Event;
use horizon_skript_core::value::{Value, format_number};
use horizon_skript_core::{Manager, Registry, config};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

const ALICE: u128 = 1;

struct H {
    m: Manager,
    host: Recorder,
}

impl H {
    fn new() -> H {
        H {
            m: Manager::new(Registry::new(), PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused")),
            host: Recorder::with_players(&[(ALICE, "Alice")]),
        }
    }

    fn load(&mut self, src: &str) {
        self.m
            .load_source(&self.host, "test.sk", src, true)
            .unwrap_or_else(|errs| panic!("the script parses: {errs:?}"));
    }

    fn join(&mut self) {
        let data = Event {
            player: Some(Value::Player {
                uuid: ALICE,
                name: "Alice".into(),
            }),
            ..Event::default()
        };
        self.m.fire(&self.host, "join", data);
    }

    fn var(&self, name: &str) -> Option<Value> {
        self.m.variables().get(name).cloned()
    }

    fn sent_texts(&self) -> Vec<String> {
        self.host.sent.borrow().iter().map(|(_, t)| t.clone()).collect()
    }
}

/// A script with one `on join:` trigger holding `body` (indented by the caller).
fn join_script(body: &str) -> String {
    format!("on join:\n{body}")
}

#[test]
fn loop_times_runs_one_to_n_with_the_counters() {
    let mut h = H::new();
    h.load(&join_script(
        "\tloop 3 times:\n\t\tadd loop-number to {n}\n\t\tadd loop-iteration * 10 to {d}\n\t\tadd loop-counter to {c}\n",
    ));
    h.join();
    assert_eq!(h.var("n"), Some(Value::Int(6)));
    assert_eq!(h.var("d"), Some(Value::Int(60)));
    assert_eq!(h.var("c"), Some(Value::Int(6)));
}

#[test]
fn loop_times_with_a_variable_count_and_zero() {
    let mut h = H::new();
    h.load(&join_script(
        "\tset {k} to 4\n\tloop {k} times:\n\t\tadd 1 to {n}\n\tloop 0 times:\n\t\tadd 1 to {n}\n",
    ));
    h.join();
    assert_eq!(h.var("n"), Some(Value::Int(4)));
}

#[test]
fn loop_counter_resets_in_an_inner_loop() {
    let mut h = H::new();
    h.load(&join_script(
        "\tloop 2 times:\n\t\tloop 2 times:\n\t\t\tadd loop-iteration to {inner}\n\t\tadd loop-iteration to {outer}\n",
    ));
    h.join();
    // Inner loops count 1 and 2 twice each; the outer loop counts 1 and 2 once each.
    assert_eq!(h.var("inner"), Some(Value::Int(6)));
    assert_eq!(h.var("outer"), Some(Value::Int(3)));
}

#[test]
fn typed_list_loop_names_its_value() {
    let mut h = H::new();
    h.load(&join_script(
        "\tadd player to {p::*}\n\tadd player to {p::*}\n\tloop {p::*}:\n\t\tsend \"hi\" to loop-player\n",
    ));
    h.join();
    assert_eq!(h.sent_texts(), vec!["hi".to_owned(), "hi".to_owned()]);
}

#[test]
fn subtract_takes_from_a_variable() {
    let mut h = H::new();
    h.load(&join_script("\tset {x} to 10\n\tsubtract 3 from {x}\n"));
    h.join();
    assert_eq!(h.var("x"), Some(Value::Int(7)));
}

#[test]
fn exit_two_loops_leaves_both() {
    let mut h = H::new();
    h.load(&join_script(
        "\tloop 5 times:\n\t\tloop 5 times:\n\t\t\tadd 1 to {c}\n\t\t\texit 2 loops\n\tadd 10 to {after}\n",
    ));
    h.join();
    assert_eq!(h.var("c"), Some(Value::Int(1)));
    assert_eq!(h.var("after"), Some(Value::Int(10)));
}

#[test]
fn command_permission_denies_then_allows() {
    let mut h = H::new();
    h.load(
        "command /npcaura:\n\
         \tpermission: example.test\n\
         \tpermission message: <red>No!</red>\n\
         \ttrigger:\n\
         \t\tsend \"ran\" to player\n",
    );
    let run = |h: &mut H| {
        let data = Event {
            player: Some(Value::Player {
                uuid: ALICE,
                name: "Alice".into(),
            }),
            ..Event::default()
        };
        assert!(h.m.dispatch_command(&h.host, "npcaura", data));
    };
    run(&mut h);
    assert_eq!(h.sent_texts(), vec!["§cNo!§r".to_owned()]);
    h.host.permissions.push((ALICE, "example.test".into()));
    run(&mut h);
    assert_eq!(h.sent_texts(), vec!["§cNo!§r".to_owned(), "ran".to_owned()]);
}

#[test]
fn command_without_a_message_uses_skripts_default() {
    let mut h = H::new();
    h.load("command /secret:\n\tpermission: a.b\n\ttrigger:\n\t\tsend \"x\" to player\n");
    let data = Event {
        player: Some(Value::Player {
            uuid: ALICE,
            name: "Alice".into(),
        }),
        ..Event::default()
    };
    h.m.dispatch_command(&h.host, "secret", data);
    assert_eq!(
        h.sent_texts(),
        vec!["You don't have the required permission to use this command".to_owned()]
    );
}

#[test]
fn command_metadata_is_read_and_the_body_runs_without_trigger_section() {
    let mut h = H::new();
    h.load(
        "command /npc [<text>]:\n\
         \tusage: /npc <name>\n\
         \taliases: /npcs, np\n\
         \tdescription: Manage NPCs.\n\
         \tsend \"plain\" to player\n",
    );
    let infos = h.m.command_infos();
    assert_eq!(infos.len(), 1);
    let (name, info) = &infos[0];
    assert_eq!(name, "npc");
    assert_eq!(info.usage.as_deref(), Some("/npc <name>"));
    assert_eq!(info.aliases, vec!["npcs".to_owned(), "np".to_owned()]);
    assert_eq!(info.description.as_deref(), Some("Manage NPCs."));
    assert_eq!(info.permission, None);
    let data = Event {
        player: Some(Value::Player {
            uuid: ALICE,
            name: "Alice".into(),
        }),
        ..Event::default()
    };
    h.m.dispatch_command(&h.host, "npc", data);
    assert_eq!(h.sent_texts(), vec!["plain".to_owned()]);
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir");
    dir
}

#[test]
fn script_load_and_skript_load_events_fire_in_order() {
    let dir = scratch_dir("lobby_core_load_events");
    fs::write(
        dir.join("a.sk"),
        "on script load:\n\tbroadcast \"loaded\"\non skript load:\n\tbroadcast \"skript loaded\"\n",
    )
    .unwrap();
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    m.load_dir(&host);
    // The glue fires the server-start event once the server runs (`on skript load` is that event).
    m.fire(&host, "server start", horizon_skript_core::run::Event::default());
    assert_eq!(
        *host.broadcasts.borrow(),
        vec!["loaded".to_owned(), "skript loaded".to_owned()]
    );
}

#[test]
fn is_one_of_and_is_not_one_of() {
    let mut h = H::new();
    h.load(&join_script(
        "\tif \"b\" is one of \"a\", \"b\" or \"c\":\n\t\tset {one} to true\n\tif \"z\" is not one of \"a\" or \"c\":\n\t\tset {none} to true\n",
    ));
    h.join();
    assert_eq!(h.var("one"), Some(Value::Bool(true)));
    assert_eq!(h.var("none"), Some(Value::Bool(true)));
}

#[test]
fn default_operator_takes_the_fallback_only_when_unset() {
    let mut h = H::new();
    h.load(&join_script(
        "\tset {a} to {missing} ? \"fallback\"\n\tset {b} to 5 ? 9\n\tset {c} to {missing} otherwise 7\n",
    ));
    h.join();
    assert_eq!(h.var("a"), Some(Value::Text("fallback".into())));
    assert_eq!(h.var("b"), Some(Value::Int(5)));
    assert_eq!(h.var("c"), Some(Value::Int(7)));
}

#[test]
fn nested_percent_inside_a_variable_name_in_text() {
    let mut h = H::new();
    h.load(&join_script(
        "\tset {p::b} to \"B\"\n\tset {a::b} to \"ok\"\n\tset {r} to \"%{a::%{p::b}%}%\"\n",
    ));
    h.join();
    assert_eq!(h.var("r"), Some(Value::Text("ok".into())));
}

#[test]
fn colour_tags_and_ampersand_codes_become_section_codes() {
    let mut h = H::new();
    h.load(&join_script(
        "\tsend \"<gold>Hi<reset>&6x§6\" to player\n\tsend \"<#ff0000>x\" to player\n\tsend \"<##00ff00>y<bold>z\" to player\n\tsend \"<unknown>\" to player\n",
    ));
    h.join();
    assert_eq!(
        h.sent_texts(),
        vec![
            "§6Hi§r§6x§6".to_owned(),
            "§x§f§f§0§0§0§0x".to_owned(),
            "§x§0§0§f§f§0§0y§lz".to_owned(),
            "<unknown>".to_owned(),
        ]
    );
}

#[test]
fn numbers_print_with_two_decimals_at_most() {
    assert_eq!(format_number(1.0 / 3.0), "0.33");
    assert_eq!(format_number(2.5), "2.5");
    assert_eq!(format_number(2.0), "2");
    assert_eq!(format_number(1.999), "2");
    assert_eq!(format_number(-0.001), "0");
}

#[test]
fn timespan_and_date_arithmetic() {
    let mut h = H::new();
    h.host.now = 86_400_000;
    h.load(&join_script(
        "\tset {t} to 2 * 1 minute\n\tset {u} to 1 hour / 2\n\tset {v} to (random number between 1 and 1) * 1 second\n\tset {d} to now + 1 day\n\tset {diff} to difference between 5 and 9\n\tset {dd} to difference between (now + 1 day) and now\n",
    ));
    h.join();
    assert_eq!(h.var("t"), Some(Value::Timespan(120_000)));
    assert_eq!(h.var("u"), Some(Value::Timespan(1_800_000)));
    assert_eq!(h.var("v"), Some(Value::Timespan(1_000)));
    assert_eq!(h.var("d"), Some(Value::Date(172_800_000)));
    assert_eq!(h.var("diff"), Some(Value::Int(4)));
    assert_eq!(h.var("dd"), Some(Value::Timespan(86_400_000)));
}

#[test]
fn variable_names_are_case_insensitive() {
    let mut h = H::new();
    h.load(&join_script("\tset {Mixed::Name} to 1\n\tset {m} to {mixed::name}\n"));
    h.join();
    assert_eq!(h.var("m"), Some(Value::Int(1)));
}

fn db_settings(file: &Path) -> Settings {
    Settings {
        database: Some(Database {
            file: file.to_path_buf(),
            pattern: Regex::new("^(?:.*)$").unwrap(),
        }),
        ..Settings::default()
    }
}

#[test]
fn variables_are_saved_and_loaded_with_memory_only_names_left_out() {
    let dir = scratch_dir("lobby_core_vars");
    let file = dir.join("variables.csv");
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    m.configure(db_settings(&file));
    m.load_variables(&host);
    m.variables_mut().set("kept", Value::Int(42));
    m.variables_mut().set("-cache", Value::Int(1));
    m.save_variables(&host).expect("saved");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("# === Skript's variable storage ===\n"));
    assert!(text.contains("kept, long, 000000000000002A\n"));
    assert!(!text.contains("cache"));

    let mut again = Manager::new(Registry::new(), dir.clone());
    again.configure(db_settings(&file));
    again.load_variables(&host);
    assert_eq!(again.variables().get("kept"), Some(&Value::Int(42)));
}

#[test]
fn rows_of_unknown_types_survive_a_save() {
    let dir = scratch_dir("lobby_core_raw_rows");
    let file = dir.join("variables.csv");
    fs::write(
        &file,
        "# === Skript's variable storage ===\n# version: 2.17.1\nwhen, date, 00ABCDEF\nn, long, 0000000000000005\n",
    )
    .unwrap();
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    m.configure(db_settings(&file));
    m.load_variables(&host);
    m.variables_mut().set("n", Value::Int(6));
    m.save_variables(&host).expect("saved");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("when, date, 00ABCDEF\n"));
    assert!(text.contains("n, long, 0000000000000006\n"));
}

#[test]
fn saving_happens_after_the_configured_number_of_changes() {
    let dir = scratch_dir("lobby_core_threshold");
    let file = dir.join("variables.csv");
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    let mut settings = db_settings(&file);
    settings.changes_until_save = 2;
    m.configure(settings);
    m.load_variables(&host);
    m.variables_mut().set("a", Value::Int(1));
    m.tick(&host);
    assert!(!file.exists(), "one change is below the threshold");
    m.variables_mut().set("b", Value::Int(2));
    m.tick(&host);
    let text = fs::read_to_string(&file).expect("written on the tick");
    assert!(text.contains("a, long, 0000000000000001\n"));
    assert!(text.contains("b, long, 0000000000000002\n"));
}

#[test]
fn config_keys_are_read_with_skripts_format() {
    let root = config::parse(
        "number accuracy: 3\n\
         case-insensitive variables: false\n\
         variable changes until save: 250\n\
         databases:\n\
         \tdatabase 1:\n\
         \t\ttype: disabled\n\
         \tdefault:\n\
         \t\ttype: CSV\n\
         \t\tpattern: .*\n\
         \t\tfile: ./plugins/Skript/variables.csv\n",
    );
    let s = settings_from_config(&root, Path::new("/srv/server"));
    assert_eq!(s.number_accuracy, 3);
    assert!(!s.fold_case);
    assert_eq!(s.changes_until_save, 250);
    let db = s.database.expect("the CSV database");
    assert_eq!(db.file, Path::new("/srv/server/./plugins/Skript/variables.csv"));
    assert!(db.pattern.is_match("anything"));
}

#[test]
fn bare_item_words_compare_and_is_set_keeps_its_meaning() {
    let dir = scratch_dir("lobby_core_bare_items");
    fs::write(
        dir.join("a.sk"),
        "on script load:\n\tset {x} to 1\n\tif {x} is set:\n\t\tbroadcast \"set\"\n\tif {y} is not air:\n\t\tbroadcast \"not air\"\n\tif \"b\" is one of \"a\", \"b\", or \"c\":\n\t\tbroadcast \"one of\"\n",
    )
    .unwrap();
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    m.load_dir(&host);
    assert_eq!(
        *host.broadcasts.borrow(),
        vec!["set".to_owned(), "not air".to_owned(), "one of".to_owned()]
    );
}

#[test]
fn disabled_files_and_folders_are_not_parsed() {
    let dir = scratch_dir("lobby_core_disabled");
    fs::create_dir_all(dir.join("-examples")).unwrap();
    fs::write(dir.join("-examples/broken.sk"), "this is not skript at all\n").unwrap();
    fs::write(dir.join("-off.sk"), "neither is this\n").unwrap();
    fs::write(dir.join("on.sk"), "on script load:\n\tbroadcast \"on\"\n").unwrap();
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    let report = m.load_dir(&host);
    assert!(report.iter().all(|l| !l.contains("broken") && !l.contains("-off")), "{report:?}");
    assert_eq!(*host.broadcasts.borrow(), vec!["on".to_owned()]);
}

#[test]
fn a_list_variable_is_set_as_a_whole() {
    let dir = scratch_dir("lobby_core_set_list");
    fs::write(
        dir.join("a.sk"),
        "on script load:\n\tset {_r::*} to \"a/b/c\" split at \"/\"\n\tset {_r::*} to \"x/y\" split at \"/\"\n\tbroadcast \"%size of {_r::*}% %{_r::1}% %{_r::2}%\"\n",
    )
    .unwrap();
    let host = Recorder::default();
    let mut m = Manager::new(Registry::new(), dir.clone());
    m.load_dir(&host);
    assert_eq!(*host.broadcasts.borrow(), vec!["2 x y".to_owned()]);
}
