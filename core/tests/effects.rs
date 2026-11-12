//! Effects and events of the syntax library, run from small scripts: each test loads a script,
//! fires its event or runs its command, and checks what the host was asked to do (the
//! `Recorder` keeps the commands and actions as text lines).

use horizon_skript_core::host::Recorder;
use horizon_skript_core::run::Event;
use horizon_skript_core::value::{Location, Value};
use horizon_skript_core::{Manager, Registry};
use std::path::PathBuf;

const ALICE: u128 = 1;

struct Harness {
    m: Manager,
    host: Recorder,
}

impl Harness {
    fn new(src: &str) -> Harness {
        let mut m = Manager::new(Registry::new(), PathBuf::new());
        let host = Recorder::with_players(&[(ALICE, "Alice"), (2, "Bob")]);
        m.load_source(&host, "test.sk", src, true)
            .expect("the script parses");
        Harness { m, host }
    }

    fn fire(&mut self, event: &str, data: Event) {
        self.m.fire(&self.host, event, data);
    }

    fn alice() -> Event {
        Event {
            player: Some(Value::Player {
                uuid: ALICE,
                name: "Alice".to_owned(),
            }),
            ..Event::default()
        }
    }

    /// Fires `join` for Alice, where the script's effects run.
    fn run(&mut self) {
        self.fire("join", Harness::alice());
    }

    fn acted(&self) -> Vec<String> {
        self.host.acted.borrow().clone()
    }

    fn sent(&self) -> Vec<(u128, String)> {
        self.host.sent.borrow().clone()
    }

    fn broadcasts(&self) -> Vec<String> {
        self.host.broadcasts.borrow().clone()
    }

    fn var(&self, name: &str) -> Option<Value> {
        self.m.variables().get(name).cloned()
    }
}

/// Runs one effect line in a `join` trigger, with the location `{loc}` set to world at 1, 64, 2.
fn effect_at(line: &str) -> Vec<String> {
    let mut h = Harness::new(&format!("on join:\n\t{line}\n"));
    h.m.variables_mut().set(
        "loc",
        Value::Location(Location {
            world: "world".into(),
            x: 1.0,
            y: 64.0,
            z: 2.0,
            yaw: 0.0,
            pitch: 0.0,
        }),
    );
    h.run();
    h.acted()
}

/// Runs one effect line in a `join` trigger and returns what the host was asked.
fn effect(line: &str) -> Vec<String> {
    let mut h = Harness::new(&format!("on join:\n\t{line}\n"));
    h.run();
    h.acted()
}

#[test]
fn send_takes_several_texts_and_the_event_player() {
    let mut h = Harness::new("on join:\n\tsend \"a\" and \"b\"\n\tsend \"c\" to Bob\n");
    h.run();
    assert_eq!(
        h.sent(),
        vec![
            (ALICE, "a".to_owned()),
            (ALICE, "b".to_owned()),
            (2, "c".to_owned())
        ]
    );
}

#[test]
fn broadcast_says_each_text() {
    let mut h = Harness::new("on join:\n\tbroadcast \"one\" and \"two\"\n");
    h.run();
    assert_eq!(h.broadcasts(), vec!["one", "two"]);
}

#[test]
fn action_bar_and_titles_use_bukkits_default_times() {
    assert_eq!(
        effect("send action bar \"ready\" to Alice"),
        vec!["action bar 1 ready"]
    );
    // Default: fade in 10, stay 70, fade out 20 ticks.
    assert_eq!(
        effect("send title \"Hi\" to Alice"),
        vec!["title 1 Hi| 10/70/20"]
    );
    // Seconds become ticks: 1 s = 20 ticks, 2 s = 40.
    assert_eq!(
        effect(
            "send title \"T\" with subtitle \"S\" to Alice for 2 seconds with fade in 1 second and fade out 1 second"
        ),
        vec!["title 1 T|S 20/40/20"]
    );
    assert_eq!(
        effect("send subtitle \"only\" to Alice"),
        vec!["title 1 |only 10/70/20"]
    );
}

#[test]
fn make_say_sends_the_text_as_the_player() {
    assert_eq!(effect("make Alice say \"hi\""), vec!["chat 1 hi"]);
    assert_eq!(
        effect("force Alice to send the message \"yo\""),
        vec!["chat 1 yo"]
    );
}

#[test]
fn kick_ban_and_operators_run_the_vanilla_commands() {
    assert_eq!(
        effect("kick Alice because \"bye\""),
        vec!["console kick Alice bye"]
    );
    assert_eq!(effect("kick Alice"), vec!["console kick Alice"]);
    assert_eq!(
        effect("ban Alice by reason of \"spam\""),
        vec!["console ban Alice spam"]
    );
    assert_eq!(effect("unban Alice"), vec!["console pardon Alice"]);
    assert_eq!(effect("op Alice"), vec!["console op Alice"]);
    assert_eq!(effect("deop Alice"), vec!["console deop Alice"]);
    assert_eq!(effect("de-op Alice"), vec!["console deop Alice"]);
    assert_eq!(effect("kill Alice"), vec!["console kill Alice"]);
}

#[test]
fn damage_is_in_hearts_and_vanilla_counts_points() {
    assert_eq!(
        effect("damage Alice by 3 hearts"),
        vec!["console damage Alice 6"]
    );
    assert_eq!(
        effect("damage Alice by 1.5 hearts"),
        vec!["console damage Alice 3"]
    );
}

#[test]
fn heal_health_and_food_go_through_the_actions() {
    assert_eq!(effect("heal Alice"), vec!["heal 1 None"]);
    assert_eq!(effect("heal Alice by 2 hearts"), vec!["heal 1 Some(4.0)"]);
    assert_eq!(effect("set health of Alice to 12"), vec!["health 1 12"]);
    assert_eq!(effect("set food of Alice to 15"), vec!["food 1 15"]);
}

#[test]
fn game_mode_must_be_a_mode_name() {
    // Text literals are quoted, as in Skript.
    assert_eq!(
        effect("set game mode of Alice to \"creative\""),
        vec!["console gamemode creative Alice"]
    );
    // An unknown mode is a run-time error, and nothing is done.
    assert!(effect("set gamemode of Alice to \"turbo\"").is_empty());
}

#[test]
fn teleport_and_set_block_take_locations() {
    // The core has no location literal yet, so the location comes in as the variable `{loc}`.
    assert_eq!(
        effect_at("teleport Alice to {loc}"),
        vec!["teleport 1 world at 1, 64, 2"]
    );
    assert_eq!(
        effect_at("set block at {loc} to stone"),
        vec!["block world at 1, 64, 2 minecraft:stone"]
    );
}

#[test]
fn push_takes_a_direction_and_a_speed() {
    assert_eq!(
        effect("push Alice up at speed 2"),
        vec!["push 1 [0.0, 2.0, 0.0]"]
    );
    assert_eq!(effect("push Alice down"), vec!["push 1 [0.0, -1.0, 0.0]"]);
    assert_eq!(effect("push Alice west"), vec!["push 1 [-1.0, 0.0, 0.0]"]);
}

#[test]
fn potions_are_given_by_vanilla_effect_give() {
    // Level 2 is amplifier 1; 90 seconds.
    assert_eq!(
        effect("apply speed 2 to Alice for 90 seconds"),
        vec!["console effect give Alice minecraft:speed 90 1"]
    );
    // No time: as long as the vanilla maximum.
    assert_eq!(
        effect("affect Alice with haste"),
        vec!["console effect give Alice minecraft:haste 1000000 0"]
    );
}

#[test]
fn commands_run_as_the_console_or_as_a_player() {
    assert_eq!(effect("execute command \"say hi\""), vec!["console say hi"]);
    assert_eq!(
        effect("execute command \"say hi\" by Alice"),
        vec!["as 1 say hi"]
    );
    assert_eq!(
        effect("make Alice execute command \"/say x\""),
        vec!["as 1 say x"]
    );
}

#[test]
fn sounds_play_at_the_player_with_volume_and_pitch() {
    assert_eq!(
        effect("play sound \"entity.cow.ambient\" with volume 0.5 to Alice"),
        vec![
            "console execute at Alice run playsound minecraft:entity.cow.ambient master Alice ~ ~ ~ 0.5 1"
        ]
    );
    // A name with a space in it is not a sound id: nothing plays.
    assert!(effect("play sound \"bad; say hi\" to Alice").is_empty());
}

#[test]
fn spawn_names_the_entity_and_the_place() {
    assert_eq!(
        effect_at("spawn zombie at {loc}"),
        vec!["spawn world at 1, 64, 2 minecraft:zombie"]
    );
}

#[test]
fn give_and_remove_items_use_give_and_clear() {
    assert_eq!(
        effect("give 3 stone to Alice"),
        vec!["console give Alice minecraft:stone 3"]
    );
    assert_eq!(
        effect("remove 2 stone from Alice"),
        vec!["console clear Alice minecraft:stone 2"]
    );
    assert_eq!(
        effect("clear the inventory of Alice"),
        vec!["console clear Alice"]
    );
}

#[test]
fn remove_on_a_variable_is_still_the_builtin() {
    let mut h = Harness::new("on join:\n\tset {x} to 5\n\tremove 2 from {x}\n");
    h.run();
    assert_eq!(h.var("x"), Some(Value::Int(3)));
    assert!(h.acted().is_empty());
}

#[test]
fn cancel_event_sets_the_flag_the_glue_reads() {
    let mut h = Harness::new("on death:\n\tcancel event\n");
    let event = Harness::alice();
    let probe = event.clone();
    h.fire("death", event);
    assert!(probe.is_cancelled());
}

#[test]
fn uncancel_event_lets_it_go_on() {
    let mut h = Harness::new("on death:\n\tcancel event\n\tuncancel event\n");
    let event = Harness::alice();
    let probe = event.clone();
    h.fire("death", event);
    assert!(!probe.is_cancelled());
}

#[test]
fn event_values_are_read_by_name() {
    let e = Event::default().with("victim", Value::Text("minecraft:zombie".into()));
    assert_eq!(
        e.value("victim"),
        Some(&Value::Text("minecraft:zombie".into()))
    );
    assert_eq!(e.value("attacker"), None);
    // Setting a name again replaces the value.
    let e = e.with("victim", Value::Int(1));
    assert_eq!(e.value("victim"), Some(&Value::Int(1)));
}

#[test]
fn the_event_triggers_parse() {
    for header in [
        "first join",
        "respawn",
        "death",
        "damage",
        "break",
        "place",
        "click",
        "left click",
        "right click",
        "move",
        "command",
        "server start",
        "server stop",
    ] {
        let mut m = Manager::new(Registry::new(), PathBuf::new());
        let host = Recorder::default();
        let src = format!("on {header}:\n\tbroadcast \"x\"\n");
        assert!(
            m.load_source(&host, "t.sk", &src, true).is_ok(),
            "on {header}: does not parse"
        );
    }
}
