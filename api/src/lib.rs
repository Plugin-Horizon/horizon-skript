//! horizon-skript: the Skript language as a Horizon trait mod. The language is in `core/`
//! (tested without the engine); this file maps the engine's hooks onto it: the scripts folder
//! and its config, the `/skript` command, the commands scripts add, join, quit and chat events,
//! messages to players, and the tick that runs waits and repeats.
//!
//! Built against Horizon 1453e0e61. Lines marked `UNVERIFIED`
//! guess a field or method name the generated event types may spell differently; they are the
//! first thing to check when this compiles (STATUS.md).

use horizon_client::mod_api::{
    BlockPlaceEvent, ConfigLoadingEvent, ConfigReloadingEvent, DestroyBlockEvent, Flow, Hooks,
    Host as EngineHost, LeftClickBlockEvent, LivingIncomingDamageEvent, NativeCommand,
    NativeConfig, NativeConfigKind, PerformCommandEvent, PerformConsoleCommandEvent,
    PlayerChatEvent, PlayerCommandEvent, PlayerDiedEvent, PlayerLoggedInEvent,
    PlayerLoggedOutEvent, PlayerMoveEvent, PlayerRespawnedEvent, RegisterCommandsEvent,
    RegisterConfigsEvent, RightClickBlockEvent, ServerAboutToStartEvent, ServerStoppingEvent, Text,
    TickEvent,
};
use horizon_skript_core::Registry;
use horizon_skript_core::host::{Actions, Host as SkHost};
use horizon_skript_core::manager::Manager;
use horizon_skript_core::run::Event;
use horizon_skript_core::value::{Location, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MOD_ID: &str = "skript";
/// The folder scripts are read from, relative to the server's folder (Skript's own default).
const DEFAULT_FOLDER: &str = "plugins/Skript/scripts";
const DEFAULT_TOML: &str = "# Folder of the .sk scripts, relative to the server folder.\nscripts-folder = \"plugins/Skript/scripts\"\n";

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Skript {
        st: Mutex::new(State::default()),
    })
}

struct Skript {
    st: Mutex<State>,
}

#[derive(Default)]
struct State {
    folder: Option<String>,
    manager: Option<Manager>,
    /// Chat lines heard in `player_chat`, fired as `chat` events on the next tick: the hook
    /// has no host, so the message cannot be answered inside it.
    chats: Vec<(u128, String)>,
    /// Events heard in hooks that have no host (damage, move): fired on the next tick. They
    /// cannot be cancelled.
    queued: Vec<(&'static str, Event)>,
    names: HashMap<u128, String>,
}

fn lock(m: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The language core's host, over the engine's host for one hook call.
struct Bridge<'a> {
    host: RefCell<&'a mut dyn EngineHost>,
}

impl SkHost for Bridge<'_> {
    fn send(&self, uuid: u128, text: &str) {
        self.host
            .borrow_mut()
            .tell(uuid, Text::literal(text.to_owned()));
    }

    fn broadcast(&self, text: &str) {
        let players = self.host.borrow_mut().server_players();
        for p in players {
            self.host
                .borrow_mut()
                .tell(p.uuid, Text::literal(text.to_owned()));
        }
    }

    fn log(&self, line: &str) {
        eprintln!("{line}");
    }

    fn player_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.host
            .borrow_mut()
            .server_players()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| (p.uuid, p.name))
    }

    // UNVERIFIED: the same engine call as `player_by_name` (compiles; behaviour not run yet). The other player
    // queries (`player_state`, `has_permission`, `can_see`, `block_type_at`, `target_block`) keep
    // the core's defaults until the engine's player API is read (GAPS.md).
    fn online_players(&self) -> Vec<(u128, String)> {
        self.host
            .borrow_mut()
            .server_players()
            .into_iter()
            .map(|p| (p.uuid, p.name))
            .collect()
    }

    fn actions(&self) -> &dyn Actions {
        self
    }

    fn now_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

// UNVERIFIED throughout (STATUS.md, GAPS.md): the helper names are the ones the engine's
// fragments in `tools/native-hook-fragments` give (`run_command`, `run_command_as`, `action_bar`,
// `title`, `chat_as`, `teleport_player`, `set_player_status`, `dimension_of_world`); the argument
// types are guesses until the engine compiles here. Stubs return false and are listed in GAPS.md.
impl Actions for Bridge<'_> {
    fn run_console(&self, line: &str) -> bool {
        self.host.borrow_mut().run_command(line)
    }

    fn run_as(&self, uuid: u128, line: &str) -> bool {
        // The answer (a NativeCommandResult) goes to the player; the effect does not read it.
        let _ = self.host.borrow_mut().run_command_as(uuid, line);
        true
    }

    fn action_bar(&self, uuid: u128, text: &str) -> bool {
        self.host
            .borrow_mut()
            .action_bar(uuid, Text::literal(text.to_owned()))
    }

    fn title(
        &self,
        uuid: u128,
        title: &str,
        subtitle: &str,
        fade_in: u64,
        stay: u64,
        fade_out: u64,
    ) -> bool {
        // UNVERIFIED: the engine may take the times as another integer type.
        self.host.borrow_mut().title(
            uuid,
            &Text::literal(title.to_owned()),
            &Text::literal(subtitle.to_owned()),
            fade_in as i32,
            stay as i32,
            fade_out as i32,
        )
    }

    fn chat_as(&self, uuid: u128, text: &str) -> bool {
        // False inside a player's hook (the engine says so): `make ... say` from a join or chat
        // trigger does nothing until this runs from the tick (GAPS.md).
        self.host
            .borrow_mut()
            .chat_as(uuid, Text::literal(text.to_owned()))
    }

    fn teleport(&self, uuid: u128, to: &Location) -> bool {
        let mut host = self.host.borrow_mut();
        let Some(dimension) = host.dimension_of_world(&to.world) else {
            return false;
        };
        host.teleport_player(uuid, &dimension, [to.x, to.y, to.z], to.yaw, to.pitch)
    }

    // STUB: a block needs its state id (`default_state`) and the flags of `set_block`; GAPS.md.
    fn set_block(&self, _at: &Location, _block: &str) -> bool {
        false
    }

    // STUB: `summon` needs the entity's NBT; GAPS.md.
    fn spawn(&self, _at: &Location, _entity: &str) -> bool {
        false
    }

    fn set_health(&self, uuid: u128, health: f64) -> bool {
        // UNVERIFIED: the helper takes an optional health, food and experience each.
        self.host
            .borrow_mut()
            .set_player_status(uuid, Some(health as f32), None, None)
    }

    fn heal(&self, uuid: u128, points: Option<f64>) -> bool {
        // Full health is the maximum (20, UNVERIFIED for modified attributes). A number of
        // points needs the current health, which the glue does not read yet (GAPS.md).
        match points {
            None => self.set_health(uuid, 20.0),
            Some(_) => false,
        }
    }

    fn set_food(&self, uuid: u128, food: i64) -> bool {
        // The engine takes food with saturation and exhaustion; keep the player's own values.
        let mut host = self.host.borrow_mut();
        let (_, saturation, exhaustion) = host.player_food(uuid).unwrap_or((0, 0.0, 0.0));
        host.set_player_status(uuid, None, Some((food as i32, saturation, exhaustion)), None)
    }

    // STUB: the engine's entity velocity helper is not identified yet; GAPS.md.
    fn push(&self, _uuid: u128, _velocity: [f64; 3]) -> bool {
        false
    }
}

impl Skript {
    /// Runs `f` with the manager, loading the scripts the first time (and after the folder changed).
    fn with_manager<R>(
        &self,
        host: &mut dyn EngineHost,
        f: impl FnOnce(&mut State, &mut Manager, &Bridge<'_>) -> R,
    ) -> R {
        let mut st = lock(&self.st);
        let bridge = Bridge {
            host: RefCell::new(host),
        };
        if st.manager.is_none() {
            let folder = st
                .folder
                .clone()
                .unwrap_or_else(|| DEFAULT_FOLDER.to_owned());
            let mut m = Manager::new(Registry::new(), folder);
            m.load_dir(&bridge);
            st.manager = Some(m);
        }
        // The manager is taken out while `f` runs, so `f` may also look at the state.
        let mut m = st.manager.take().expect("set above");
        let out = f(&mut *st, &mut m, &bridge);
        st.manager = Some(m);
        out
    }

    fn load_config(&self, text: &str) {
        let folder = text.parse::<toml::Table>().ok().and_then(|t| {
            t.get("scripts-folder")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        });
        match folder {
            Some(f) => {
                let mut st = lock(&self.st);
                if st.folder.as_deref() != Some(f.as_str()) {
                    st.folder = Some(f);
                    // A new folder: the next use loads its scripts.
                    st.manager = None;
                }
            }
            None => eprintln!("[skript] config has no scripts-folder, keeping the old one"),
        }
    }
}

fn player_value(uuid: u128, name: &str) -> Value {
    Value::Player {
        uuid,
        name: name.to_owned(),
    }
}

/// An event with the player who it is about (name from the join hook).
fn player_event(sk: &Skript, uuid: u128) -> Event {
    let name = lock(&sk.st).names.get(&uuid).cloned().unwrap_or_default();
    Event {
        player: Some(player_value(uuid, &name)),
        ..Event::default()
    }
}

/// Fires each of `names` with `data` and says whether a trigger cancelled the event. The
/// triggers run before this returns, so the cancel flag is set by then.
fn fire_cancellable(sk: &Skript, host: &mut dyn EngineHost, names: &[&str], data: Event) -> Flow {
    let probe = data.clone();
    sk.with_manager(host, |_, m, b| {
        for name in names {
            m.fire(b, name, data.clone());
        }
    });
    if probe.is_cancelled() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// A block position as a location in the world of `dimension` (UNVERIFIED: the position fields
/// and `world_of_dimension`).
fn block_location(host: &mut dyn EngineHost, dimension: &str, pos: [i32; 3]) -> Value {
    let world = host
        .world_of_dimension(dimension)
        .unwrap_or_else(|| dimension.to_owned());
    Value::Location(Location {
        world,
        x: f64::from(pos[0]),
        y: f64::from(pos[1]),
        z: f64::from(pos[2]),
        yaw: 0.0,
        pitch: 0.0,
    })
}

/// The dimension a player is in, from the server's player list (UNVERIFIED field `dimension`).
fn player_dimension(host: &mut dyn EngineHost, uuid: u128) -> Option<String> {
    host.server_players()
        .into_iter()
        .find(|p| p.uuid == uuid)
        .map(|p| p.dimension)
}

impl Hooks for Skript {
    fn register_configs(&self, _host: &mut dyn EngineHost, event: &mut RegisterConfigsEvent<'_>) {
        event.configs.push(NativeConfig::new(
            MOD_ID,
            NativeConfigKind::Common,
            DEFAULT_TOML,
        ));
    }

    fn config_loading(&self, _host: &mut dyn EngineHost, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == MOD_ID {
            self.load_config(&event.config.text);
        }
    }

    fn config_reloading(&self, _host: &mut dyn EngineHost, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == MOD_ID {
            self.load_config(&event.config.text);
        }
    }

    fn register_commands(&self, host: &mut dyn EngineHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::op("skript", 2));
        event
            .commands
            .push(NativeCommand::op("skript <args:text>", 2));
        // The names of the scripts' `command /name:` triggers (loads the scripts on the first call).
        let names = self.with_manager(host, |_, m, _| m.command_names());
        for name in names {
            event.commands.push(NativeCommand::new(&name));
            event
                .commands
                .push(NativeCommand::new(&format!("{name} <args:text>")));
        }
    }

    fn perform_command(
        &self,
        host: &mut dyn EngineHost,
        event: &mut PerformCommandEvent<'_>,
    ) -> Flow {
        let (root, args) = event.command.split_once(' ').unwrap_or((event.command, ""));
        let sender = event.player.clone();
        if root == "skript" {
            self.with_manager(host, |_, m, b| {
                for line in m.skript_command(b, args) {
                    b.send(sender.uuid, &line);
                }
            });
            return Flow::Cancel;
        }
        self.with_manager(host, |_, m, b| {
            let data = Event {
                player: Some(player_value(sender.uuid, &sender.name)),
                message: None,
                args: args.split_whitespace().map(str::to_owned).collect(),
                ..Event::default()
            };
            if m.dispatch_command(b, root, data) {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        })
    }

    fn perform_console_command(
        &self,
        host: &mut dyn EngineHost,
        event: &mut PerformConsoleCommandEvent<'_>,
    ) -> Flow {
        let cmd = event.command.trim_start_matches('/');
        let (root, args) = cmd.split_once(' ').unwrap_or((cmd, ""));
        if root != "skript" {
            return Flow::Continue;
        }
        self.with_manager(host, |_, m, b| {
            for line in m.skript_command(b, args) {
                b.log(&line);
            }
        });
        Flow::Cancel
    }

    fn tick(&self, host: &mut dyn EngineHost, _event: &mut TickEvent) {
        self.with_manager(host, |st, m, b| {
            for (uuid, text) in std::mem::take(&mut st.chats) {
                let name = st.names.get(&uuid).cloned().unwrap_or_default();
                let data = Event {
                    player: Some(player_value(uuid, &name)),
                    message: Some(text),
                    args: Vec::new(),
                    ..Event::default()
                };
                m.fire(b, "chat", data);
            }
            for (name, data) in std::mem::take(&mut st.queued) {
                m.fire(b, name, data);
            }
            m.tick(b);
        });
    }

    fn player_logged_in(&self, host: &mut dyn EngineHost, event: &mut PlayerLoggedInEvent<'_>) {
        // UNVERIFIED: `player` and `name` as the generated event type spells them (the
        // worldguard glue uses both).
        lock(&self.st)
            .names
            .insert(event.player, event.name.to_string());
        let data = Event {
            player: Some(player_value(event.player, &event.name)),
            ..Event::default()
        };
        // UNVERIFIED timing: `has_played_before` may already be true at this hook, in which case
        // `first join` never fires (GAPS.md).
        let first = !host.has_played_before(event.player);
        self.with_manager(host, |_, m, b| {
            if first {
                m.fire(b, "first join", data.clone());
            }
            m.fire(b, "join", data);
        });
    }

    fn player_logged_out(&self, host: &mut dyn EngineHost, event: &mut PlayerLoggedOutEvent<'_>) {
        let name = lock(&self.st)
            .names
            .remove(&event.player)
            .unwrap_or_default();
        let data = Event {
            player: Some(player_value(event.player, &name)),
            ..Event::default()
        };
        self.with_manager(host, |_, m, b| m.fire(b, "quit", data));
    }

    // -- events of the syntax library. UNVERIFIED throughout: the event type names follow the
    // generator's convention (as the worldguard glue uses them), and the field names are the ones
    // that glue reads. The hooks with a host can cancel; the others queue the event (GAPS.md).

    fn server_about_to_start(
        &self,
        host: &mut dyn EngineHost,
        _event: &mut ServerAboutToStartEvent,
    ) {
        self.with_manager(host, |_, m, b| m.fire(b, "server start", Event::default()));
    }

    fn server_stopping(&self, host: &mut dyn EngineHost, _event: &mut ServerStoppingEvent) {
        self.with_manager(host, |_, m, b| m.fire(b, "server stop", Event::default()));
    }

    fn player_respawned(&self, host: &mut dyn EngineHost, event: &mut PlayerRespawnedEvent<'_>) {
        let data = player_event(self, event.player);
        fire_cancellable(self, host, &["respawn"], data);
    }

    fn player_died(&self, host: &mut dyn EngineHost, event: &mut PlayerDiedEvent<'_>) {
        let data = player_event(self, event.player);
        fire_cancellable(self, host, &["death"], data);
    }

    fn living_incoming_damage(&self, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        // No host here, so the damage event is queued for the tick and cannot be cancelled.
        // UNVERIFIED: `kind` (the entity id), `amount`. The victim is only its entity type: a
        // player's UUID needs the entity-to-player map the worldguard glue keeps (GAPS.md).
        let data = Event::default()
            .with("victim", Value::EntityType(event.kind.to_owned()))
            .with("damage", Value::Num(f64::from(event.amount)));
        lock(&self.st).queued.push(("damage", data));
        Flow::Continue
    }

    fn destroy_block(&self, host: &mut dyn EngineHost, event: &mut DestroyBlockEvent) -> Flow {
        let Some(np) = host.acting_player() else {
            return Flow::Continue;
        };
        let at = block_location(host, &np.dimension, event.pos);
        let data = player_event(self, np.uuid).with("location", at);
        fire_cancellable(self, host, &["break"], data)
    }

    fn block_place(&self, host: &mut dyn EngineHost, event: &mut BlockPlaceEvent<'_>) -> Flow {
        let Some(dimension) = player_dimension(host, event.player) else {
            return Flow::Continue;
        };
        let at = block_location(host, &dimension, event.pos);
        let data = player_event(self, event.player).with("location", at);
        fire_cancellable(self, host, &["place"], data)
    }

    fn right_click_block(
        &self,
        host: &mut dyn EngineHost,
        event: &mut RightClickBlockEvent,
    ) -> Flow {
        let Some(np) = host.acting_player() else {
            return Flow::Continue;
        };
        let at = block_location(host, &np.dimension, event.pos);
        let data = player_event(self, np.uuid).with("location", at);
        fire_cancellable(self, host, &["right click", "click"], data)
    }

    fn left_click_block(&self, host: &mut dyn EngineHost, event: &mut LeftClickBlockEvent) -> Flow {
        let Some(np) = host.acting_player() else {
            return Flow::Continue;
        };
        let at = block_location(host, &np.dimension, event.pos);
        let data = player_event(self, np.uuid).with("location", at);
        fire_cancellable(self, host, &["left click", "click"], data)
    }

    fn player_move(&self, event: &mut PlayerMoveEvent<'_>) -> Flow {
        // No host: queued for the tick, and `from` and `to` are not filled (GAPS.md).
        let data = player_event(self, event.player);
        lock(&self.st).queued.push(("move", data));
        Flow::Continue
    }

    fn player_command(
        &self,
        host: &mut dyn EngineHost,
        event: &mut PlayerCommandEvent<'_>,
    ) -> Flow {
        let mut data = player_event(self, event.player);
        data.message = Some(event.command.to_owned());
        fire_cancellable(self, host, &["command"], data)
    }

    fn player_chat(&self, event: &mut PlayerChatEvent<'_>) -> Flow {
        // UNVERIFIED: `message` on the generated chat event type (the hook passes it as `&str`).
        lock(&self.st)
            .chats
            .push((event.player, event.message.to_owned()));
        Flow::Continue
    }
}
