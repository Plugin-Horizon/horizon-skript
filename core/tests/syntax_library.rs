//! Script-level tests of the syntax library (`src/syntax/`): each condition and expression is
//! written in a small `.sk` file the way a server admin would, run on `load`, and checked
//! against what the host received. Players and worlds come from a fixed host.

use horizon_skript_core::host::{ItemStack, PlayerState, Recorder};
use horizon_skript_core::run::Event;
use horizon_skript_core::value::{Location, Value};
use horizon_skript_core::{Manager, Registry};
use std::path::PathBuf;

const ALICE: u128 = 0x1234_5678_9abc_def0_1122_3344_5566_7788;
const BOB: u128 = 2;

fn alice() -> PlayerState {
    PlayerState {
        name: "Alice".into(),
        display_name: "Alice the Brave".into(),
        location: Location {
            world: "world".into(),
            x: 10.5,
            y: 64.0,
            z: -3.25,
            yaw: 90.0,
            pitch: 0.0,
        },
        health: 18.0,
        max_health: 20.0,
        food: 15,
        level: 7,
        gamemode: "survival".into(),
        flying: false,
        can_fly: false,
        sneaking: true,
        sprinting: false,
        swimming: false,
        op: true,
        held: "minecraft:diamond_sword".into(),
        inventory: vec![
            ItemStack {
                id: "minecraft:stone".into(),
                amount: 64,
            },
            ItemStack {
                id: "minecraft:flint".into(),
                amount: 3,
            },
        ],
    }
}

fn bob() -> PlayerState {
    PlayerState {
        name: "Bob".into(),
        display_name: "Bob".into(),
        location: Location {
            world: "world_nether".into(),
            x: 0.0,
            y: 40.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
        },
        health: 0.0,
        max_health: 20.0,
        food: 20,
        level: 0,
        gamemode: "creative".into(),
        flying: true,
        can_fly: true,
        sneaking: false,
        sprinting: false,
        swimming: false,
        op: false,
        held: "minecraft:air".into(),
        inventory: vec![],
    }
}

/// Runs `body` (script lines, one tab in) in an `on load` trigger and returns what it
/// broadcast, the log lines and the host, panicking on parse errors.
fn run_with(host: Recorder, body: &str) -> (Vec<String>, Vec<String>) {
    let src = format!("on load:\n{body}");
    let mut m = Manager::new(
        Registry::new(),
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused"),
    );
    if let Err(errs) = m.load_source(&host, "test.sk", &src, true) {
        panic!("the script does not parse: {errs:?}");
    }
    let broadcasts = host.broadcasts.borrow().clone();
    let logs = host.logs.borrow().clone();
    (broadcasts, logs)
}

fn host() -> Recorder {
    let mut r = Recorder::with_players(&[(ALICE, "Alice"), (BOB, "Bob")]);
    r.states = vec![(ALICE, alice()), (BOB, bob())];
    r.rand = 0.5;
    r
}

/// The value of an expression, as its text: the script stores it in `{r}` and broadcasts it.
fn expr(e: &str) -> String {
    expr_in(host(), e)
}

fn expr_in(h: Recorder, e: &str) -> String {
    let (out, logs) = run_with(h, &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r}}%\"\n"));
    assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
    assert_eq!(out.len(), 1, "no broadcast for `{e}`");
    out.into_iter().next().unwrap()
}

/// The items of a list expression, joined as the text of the list (`a, b`).
fn list(e: &str) -> String {
    let (out, logs) = run_with(
        host(),
        &format!("\tset {{r}} to {e}\n\tbroadcast \"%{{r::*}}%\"\n"),
    );
    assert!(logs.is_empty(), "run errors for `{e}`: {logs:?}");
    out.into_iter().next().unwrap_or_default()
}

/// Whether the condition holds, from `if`.
fn check(c: &str) -> bool {
    check_in(host(), c)
}

fn check_in(h: Recorder, c: &str) -> bool {
    let body = format!("\tif {c}:\n\t\tbroadcast \"yes\"\n\telse:\n\t\tbroadcast \"no\"\n");
    let (out, logs) = run_with(h, &body);
    assert!(logs.is_empty(), "run errors for `{c}`: {logs:?}");
    match out.as_slice() {
        [one] if one == "yes" => true,
        [one] if one == "no" => false,
        other => panic!("unexpected output for `{c}`: {other:?}"),
    }
}

// ---------------------------------------------------------------- comparisons

#[test]
fn is_and_isnt_compare_by_value() {
    assert!(check("5 is 5.0"));
    assert!(check("5 is equal to 5"));
    assert!(check("\"Hello\" is \"hello\""));
    assert!(check("\"a\" isn't \"b\""));
    assert!(check("\"a\" is not \"b\""));
    assert!(check("1 = 1"));
    assert!(check("1 != 2"));
    assert!(
        !check("\"5\" is 5"),
        "text and numbers are different classes"
    );
    assert!(!check("2 is 3"));
}

#[test]
fn greater_less_and_between() {
    assert!(check("5 > 3"));
    assert!(check("5 is greater than 3"));
    assert!(check("5 is more than or equal to 5"));
    assert!(check("3 < 5"));
    assert!(check("3 is smaller than 5"));
    assert!(check("5 is less than or equal to 5"));
    assert!(check("5 is between 1 and 10"));
    assert!(check("5 isn't between 6 and 10"));
    assert!(!check("3 is greater than 3"));
    assert!(check("3 >= 3"));
    assert!(check("(1 + 1) is 2"));
}

#[test]
fn comparing_unordered_values_is_an_error() {
    let (out, logs) = run_with(host(), "\tif \"a\" > \"b\":\n\t\tbroadcast \"x\"\n");
    assert!(out.is_empty());
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("can't compare text with text"), "{logs:?}");
}

#[test]
fn lists_compare_item_by_item() {
    // Every left item must equal one of the right items; every item must be greater than each.
    assert!(check("2 is 1, 2 and 3"));
    assert!(!check("4 is 1, 2 and 3"));
    assert!(check("1, 2 and 3 are greater than 0"));
    assert!(!check("1, 2 and 3 are greater than 1"));
}

#[test]
fn contains_text_and_lists() {
    assert!(check("\"hello world\" contains \"o w\""));
    assert!(check("\"hello\" does not contain \"xyz\""));
    assert!(check("1, 2 and 3 contains 2"));
    assert!(check("1, 2 and 3 doesn't contain 4"));
}

#[test]
fn starts_and_ends_with() {
    assert!(check("\"Hello\" starts with \"he\""));
    assert!(check("\"Hello\" ends with \"LO\""));
    assert!(check("\"Hello\" doesn't start with \"lo\""));
}

#[test]
fn is_empty_and_evenly_divisible() {
    assert!(check("\"\" is empty"));
    assert!(!check("\"a\" is empty"));
    assert!(check("\"a\" is not empty"));
    assert!(check("10 is evenly divisible by 5"));
    assert!(!check("10 is evenly divisible by 4"));
}

// ---------------------------------------------------------------- player conditions

#[test]
fn player_state_conditions() {
    assert!(check("Alice is online"));
    assert!(check("Alice and Bob are online"));
    assert!(check("Alice is an operator"));
    assert!(!check("Bob is op"));
    assert!(check("Bob is dead"));
    assert!(check("Alice is alive"));
    assert!(check("Bob is not alive"));
    assert!(check("Bob is flying"));
    assert!(check("Alice isn't flying"));
    assert!(check("Bob can fly"));
    assert!(check("Alice can't fly"));
    assert!(check("Alice is sneaking"));
    assert!(check("Alice is not sprinting"));
    assert!(!check("Alice is swimming"));
}

#[test]
fn holding_and_inventory() {
    assert!(check("Alice is holding diamond sword"));
    assert!(!check("Alice is holding stick"));
    assert!(check("Alice has 3 flint"));
    assert!(check("Alice has 64 stone and 1 flint"));
    assert!(!check("Alice has 4 flint"));
    assert!(check("Alice does not have 4 flint"));
}

#[test]
fn permissions_and_can_see() {
    let mut h = host();
    h.permissions = vec![(ALICE, "example.fly".into())];
    h.sees = vec![(ALICE, BOB)];
    let mut h2 = host();
    h2.permissions = h.permissions.clone();
    h2.sees = h.sees.clone();
    assert!(check_in(h, "Alice has permission \"example.fly\""));
    assert!(!check_in(
        clone_host(&h2),
        "Alice has permission \"other.node\""
    ));
    assert!(check_in(clone_host(&h2), "Alice can see Bob"));
    assert!(!check_in(clone_host(&h2), "Bob can see Alice"));
    assert!(check_in(h2, "Bob can't see Alice"));
}

#[test]
fn is_in_world_for_players_and_locations() {
    assert!(check("Alice is in world world"));
    assert!(check("Bob is in world world_nether"));
    assert!(!check("Bob is in world world"));
    assert!(check("location(1, 2, 3, \"world\") is in world world"));
}

#[test]
fn chance_uses_the_host_random_number() {
    // The fixed host draws 0.5: that is a 50 in 100 chance.
    assert!(check("chance of 60"));
    assert!(!check("chance of 40"));
    assert!(check("chance of 60%"));
}

// ---------------------------------------------------------------- player expressions

#[test]
fn player_properties() {
    assert_eq!(expr("name of Alice"), "Alice");
    assert_eq!(expr("Alice's name"), "Alice");
    assert_eq!(expr("display name of Alice"), "Alice the Brave");
    assert_eq!(
        expr("uuid of Alice"),
        "12345678-9abc-def0-1122-334455667788"
    );
    assert_eq!(expr("health of Alice"), "18");
    assert_eq!(expr("max health of Alice"), "20");
    assert_eq!(expr("food level of Alice"), "15");
    assert_eq!(expr("level of Alice"), "7");
    assert_eq!(expr("gamemode of Alice"), "survival");
    assert_eq!(expr("tool of Alice"), "diamond sword");
    assert_eq!(expr("held item of Alice"), "diamond sword");
    assert_eq!(expr("world of Alice"), "world");
    assert_eq!(expr("Bob's world"), "world_nether");
    assert_eq!(expr("location of Alice"), "world at 10.5, 64, -3.25");
}

#[test]
fn inventory_and_online_players() {
    assert_eq!(expr("amount of inventory of Alice"), "2");
    assert_eq!(list("inventory of Alice"), "64 stone, 3 flint");
    assert_eq!(expr("amount of online players"), "2");
    assert_eq!(list("players"), "Alice, Bob");
}

#[test]
fn target_block_and_block_at() {
    let spot = Location {
        world: "world".into(),
        x: 1.0,
        y: 70.0,
        z: 1.0,
        yaw: 0.0,
        pitch: 0.0,
    };
    let mut h = host();
    h.target = Some(spot.clone());
    h.blocks = vec![(spot, "minecraft:stone".into())];
    assert_eq!(expr_in(h, "target block of Alice"), "stone");
    let mut h = host();
    h.blocks = vec![(
        Location {
            world: "world".into(),
            x: 4.0,
            y: 64.0,
            z: 4.0,
            yaw: 0.0,
            pitch: 0.0,
        },
        "minecraft:oak_planks".into(),
    )];
    assert_eq!(
        expr_in(h, "block at location(4, 64, 4, \"world\")"),
        "oak planks"
    );
}

// ---------------------------------------------------------------- locations

#[test]
fn coordinates_worlds_and_distance() {
    assert_eq!(expr("x of Alice"), "10.5");
    assert_eq!(expr("y of location(1, 2, 3)"), "2");
    assert_eq!(expr("z of location(1, 2, 3, \"world\")"), "3");
    assert_eq!(expr("world of location(1, 2, 3, \"nether\")"), "nether");
    assert_eq!(
        expr("distance between location(0, 0, 0) and location(3, 4, 0)"),
        "5"
    );
}

// ---------------------------------------------------------------- text

#[test]
fn text_expressions() {
    assert_eq!(expr("length of \"h\u{e9}llo\""), "5");
    assert_eq!(expr("uppercase \"abc\""), "ABC");
    assert_eq!(expr("lowercase \"ABC\""), "abc");
    assert_eq!(expr("substring of \"hello\" from 2 to 4"), "ell");
    assert_eq!(expr("substring of \"hello\" from 4 to 99"), "lo");
    assert_eq!(expr("character at index 2 in \"hello\""), "e");
    assert_eq!(expr("join \"a\", \"b\" and \"c\""), "abc");
    assert_eq!(expr("concat \"x\" and 1"), "x1");
    assert_eq!(expr("join \"a\", \"b\" with \"-\""), "a-b");
    assert_eq!(list("split \"a,b,c\" at \",\""), "a, b, c");
    assert_eq!(
        expr("replace all \"o\" with \"0\" in \"foo boo\""),
        "f00 b00"
    );
}

// ---------------------------------------------------------------- collections, random, time

#[test]
fn amount_random_and_now() {
    assert_eq!(expr("amount of 1, 2 and 3"), "3");
    assert_eq!(expr("size of 1, 2"), "2");
    // The fixed host draws 0.5: the middle element of three, and the middle integer of 1 to 4.
    assert_eq!(expr("random element out of 1, 2 and 3"), "2");
    assert_eq!(expr("random integer between 1 and 4"), "3");
    assert_eq!(expr("random number between 0 and 10"), "5");
    let mut h = host();
    h.now = 86_400_000;
    assert_eq!(expr_in(h, "now"), "1970-01-02 00:00:00");
}

// ---------------------------------------------------------------- functions

#[test]
fn math_functions() {
    assert_eq!(expr("abs(-3)"), "3");
    assert_eq!(expr("round(2.5)"), "3");
    assert_eq!(expr("round(2.36, 1)"), "2.4");
    assert_eq!(expr("floor(2.99)"), "2");
    assert_eq!(expr("ceil(2.01)"), "3");
    assert_eq!(expr("ceiling(2.01)"), "3");
    assert_eq!(expr("min(1, 2, 3, 4)"), "1");
    assert_eq!(expr("max(1, 2, 3, 4)"), "4");
    assert_eq!(expr("sqrt(16)"), "4");
    assert_eq!(expr("sin(90)"), "1");
    assert_eq!(expr("mod(-1, 10)"), "9");
    assert_eq!(expr("sum(2, 3, 4)"), "9");
    assert_eq!(expr("product(2, 3, 4)"), "24");
    assert_eq!(expr("mean(1, 2, 3)"), "2");
    assert_eq!(expr("clamp(5.5, 0, 5)"), "5");
    assert_eq!(expr("round(log(16, 2))"), "4");
}

#[test]
fn functions_build_values() {
    assert_eq!(expr("date(1970, 1, 2)"), "1970-01-02 00:00:00");
    assert_eq!(expr("date(2014, 10, 1, 14, 30)"), "2014-10-01 14:30:00");
    assert_eq!(expr("world(\"world_nether\")"), "world_nether");
    assert_eq!(expr("concat(\"foo \", 100, \" bar\")"), "foo 100 bar");
    assert_eq!(expr("location(1, 2, 3, \"world\")"), "world at 1, 2, 3");
    let mut h = host();
    h.now = 0;
    assert_eq!(expr_in(h, "now()"), "1970-01-01 00:00:00");
}

// ---------------------------------------------------------------- event values

/// Loads `body` as an `on <event>` trigger, fires it with `event` and returns what it broadcast.
fn fire_in(h: &Recorder, event: &str, body: &str, data: Event) -> Vec<String> {
    let src = format!("on {event}:\n{body}");
    let mut m = Manager::new(
        Registry::new(),
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unused"),
    );
    m.load_source(h, "test.sk", &src, true)
        .expect("the script parses");
    let before = h.broadcasts.borrow().len();
    m.fire(h, event, data);
    h.broadcasts.borrow()[before..].to_vec()
}

#[test]
fn attacker_victim_and_event_values() {
    let h = host();
    let data = Event::default()
        .with(
            "victim",
            Value::Player {
                uuid: BOB,
                name: "Bob".into(),
            },
        )
        .with(
            "attacker",
            Value::Player {
                uuid: ALICE,
                name: "Alice".into(),
            },
        )
        .with("damage", Value::Num(3.5));
    let out = fire_in(
        &h,
        "death",
        "\tbroadcast \"%name of victim% killed by %name of attacker%\"\n\tbroadcast \"%event-damage%\"\n",
        data,
    );
    assert_eq!(out, vec!["Bob killed by Alice", "3.5"]);
}

#[test]
fn event_values_that_are_missing_are_errors() {
    let h = host();
    let (out, logs) = (
        fire_in(
            &h,
            "death",
            "\tbroadcast \"%attacker%\"\n",
            Event::default(),
        ),
        h.logs.borrow().clone(),
    );
    assert!(out.is_empty());
    assert_eq!(logs.len(), 1);
    assert!(
        logs[0].contains("there is no attacker in this event"),
        "{logs:?}"
    );
}

#[test]
fn wrong_arity_is_not_a_builtin() {
    let (_, logs) = run_with(host(), "\tbroadcast \"%abs(1, 2)%\"\n");
    assert_eq!(logs.len(), 1);
    assert!(
        logs[0].contains("there is no function named abs"),
        "{logs:?}"
    );
}

/// A copy of the parts of the host the tests set up (`Recorder` is not `Clone`).
fn clone_host(h: &Recorder) -> Recorder {
    Recorder {
        players: h.players.clone(),
        states: h.states.clone(),
        permissions: h.permissions.clone(),
        sees: h.sees.clone(),
        blocks: h.blocks.clone(),
        target: h.target.clone(),
        rand: h.rand,
        now: h.now,
        ..Recorder::default()
    }
}
