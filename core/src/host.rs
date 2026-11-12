//! What the language core needs from the server. The Horizon glue implements it; tests use
//! [`Recorder`].

use crate::value::Location;
use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// One stack of items in a player's inventory: a namespaced id and an amount.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStack {
    pub id: String,
    pub amount: i64,
    /// The plain text of the item's custom name, if it has one.
    pub name: Option<String>,
}

/// What the engine says about an online player. The glue fills it from the player entity; every
/// field is read by the syntax library (conditions and expressions of `core/src/syntax/`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub name: String,
    pub display_name: String,
    pub location: Location,
    pub health: f64,
    pub max_health: f64,
    pub food: i64,
    pub level: i64,
    /// Lower-case game mode name: `survival`, `creative`, `adventure`, `spectator`.
    pub gamemode: String,
    pub flying: bool,
    pub can_fly: bool,
    pub sneaking: bool,
    pub sprinting: bool,
    pub swimming: bool,
    pub op: bool,
    /// Namespaced id of the item in the main hand (`minecraft:air` when empty).
    pub held: String,
    /// The filled slots of the inventory, in slot order.
    pub inventory: Vec<ItemStack>,
}

/// One entity of a world, as the scripts see it. Player-type entities (`kind` `minecraft:player`)
/// include the NPCs that are absent from `online_players`: that is how Citizens NPCs look to a
/// script (`loop all entities` with `is a player`, and `all players does not contain ...`).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityInfo {
    pub uuid: u128,
    /// Namespaced entity id, `minecraft:player` for a player-type entity.
    pub kind: String,
    pub name: String,
    pub location: Location,
}

/// A particle as a script draws it (`force draw 12 rgb(255, 0, 0) dust particle of size 1 at ...`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleSpec {
    /// Particle name as the scripts write it: `dust`, `portal`, `end rod`, ...
    pub name: String,
    /// How many particles to draw (`draw 12 ...`).
    pub count: i64,
    /// Colour of a `dust` particle, 0 to 255 per channel.
    pub colour: Option<[u8; 3]>,
    /// Size of a `dust` particle.
    pub size: f64,
    /// `extra` of the particle (speed for most kinds).
    pub extra: f64,
    /// `force`: sent whatever the distance and the viewer's particle setting.
    pub force: bool,
}

pub trait Host {
    /// The engine actions the effects of the syntax library use (commands, titles, teleports,
    /// blocks, health). A host without them returns the default, whose methods do nothing.
    fn actions(&self) -> &dyn Actions {
        &NoActions
    }
    /// Sends a line of text to one player (by UUID).
    fn send(&self, uuid: u128, text: &str);
    /// Sends a line of text to every online player.
    fn broadcast(&self, text: &str);
    /// Writes a line to the server log (script errors, reloads).
    fn log(&self, line: &str);
    /// The online player with that name (case-insensitive), as (UUID, exact name).
    fn player_by_name(&self, name: &str) -> Option<(u128, String)>;
    /// Whether the namespaced item id exists. Checked when a script names an item type.
    fn is_item_type(&self, _id: &str) -> bool {
        true
    }
    /// Whether the namespaced entity id exists.
    fn is_entity_type(&self, _id: &str) -> bool {
        true
    }
    /// Milliseconds since the Unix epoch, for `now` style expressions.
    fn now_millis(&self) -> i64 {
        0
    }
    /// The state of an online player (by UUID); `None` when the player is not online.
    fn player_state(&self, _uuid: u128) -> Option<PlayerState> {
        None
    }
    /// Every online player (UUID and exact name).
    fn online_players(&self) -> Vec<(u128, String)> {
        Vec::new()
    }
    /// Whether a player has a permission node. Operators are the glue's concern.
    fn has_permission(&self, _uuid: u128, _node: &str) -> bool {
        false
    }
    /// Whether `viewer` can see `target` (not vanished, same world, and so on).
    fn can_see(&self, _viewer: u128, _target: u128) -> bool {
        false
    }
    /// The namespaced id of the block at a location, `None` when the chunk is not loaded.
    fn block_type_at(&self, _loc: &Location) -> Option<String> {
        None
    }
    /// The block a player looks at, within `range` blocks: its location.
    fn target_block(&self, _uuid: u128, _range: f64) -> Option<Location> {
        None
    }
    /// Every entity in a world, player-type ones (NPCs included) and the others. Empty when the
    /// engine has no entity list (the glue says so in GAPS.md).
    fn entities_in_world(&self, _world: &str) -> Vec<EntityInfo> {
        Vec::new()
    }
    /// The state of an entity that is not an online player (an NPC): position, yaw and pitch,
    /// health, inventory. `None` when the engine does not know the uuid.
    fn entity_state(&self, _uuid: u128) -> Option<PlayerState> {
        None
    }
    /// The item in an equipment slot (`helmet`, `chestplate`, `leggings`, `boots`) of a player or
    /// an entity, `None` when the slot is empty or unknown.
    fn equipment(&self, _uuid: u128, _slot: &str) -> Option<ItemStack> {
        None
    }
    /// A pseudo-random number in `[0, 1)`. The default seeds from the standard library's random
    /// state; the glue may use the engine's generator instead.
    fn random_unit(&self) -> f64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(self.now_millis() as u64);
        (h.finish() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// What the effects of the syntax library ask the engine to do. Every method has a default that
/// does nothing and returns false, so a host without one of them only makes that effect a no-op.
/// Commands go through `run_console` and `run_as` (the vanilla commands, without the `/`).
pub trait Actions {
    /// Sets a player's game mode without a chat message, as Bukkit's `Player.setGameMode` (0
    /// survival, 1 creative, 2 adventure, 3 spectator). False when the host cannot: the effect then
    /// runs the vanilla `gamemode` command instead.
    fn set_game_mode(&self, _uuid: u128, _mode: i32) -> bool {
        false
    }
    /// Runs a command as the console. False when the engine refused it.
    fn run_console(&self, _line: &str) -> bool {
        false
    }
    /// Runs a command as a player, as if they typed it.
    fn run_as(&self, _uuid: u128, _line: &str) -> bool {
        false
    }
    /// Shows text on a player's action bar.
    fn action_bar(&self, _uuid: u128, _text: &str) -> bool {
        false
    }
    /// Shows a title and a subtitle; the three times are in ticks.
    fn title(
        &self,
        _uuid: u128,
        _title: &str,
        _subtitle: &str,
        _fade_in: u64,
        _stay: u64,
        _fade_out: u64,
    ) -> bool {
        false
    }
    /// Sends a chat line as the player (it goes through the chat, as if they typed it).
    fn chat_as(&self, _uuid: u128, _text: &str) -> bool {
        false
    }
    /// Teleports a player to a location (its world, position and direction).
    fn teleport(&self, _uuid: u128, _to: &Location) -> bool {
        false
    }
    /// Sets the block at a location to a namespaced block id.
    fn set_block(&self, _at: &Location, _block: &str) -> bool {
        false
    }
    /// Spawns an entity of a namespaced id at a location.
    fn spawn(&self, _at: &Location, _entity: &str) -> bool {
        false
    }
    /// Sets a player's health, in health points (20 is full).
    fn set_health(&self, _uuid: u128, _health: f64) -> bool {
        false
    }
    /// Heals a player by some health points, or fully when `None`.
    fn heal(&self, _uuid: u128, _points: Option<f64>) -> bool {
        false
    }
    /// Sets a player's food level.
    fn set_food(&self, _uuid: u128, _food: i64) -> bool {
        false
    }
    /// Adds a velocity (blocks per tick) to a player.
    fn push(&self, _uuid: u128, _velocity: [f64; 3]) -> bool {
        false
    }
    /// Draws a particle at a location, for everyone who can see it (`force` lifts the distance
    /// and the viewer's particle setting).
    fn draw_particles(&self, _at: &Location, _spec: &ParticleSpec) -> bool {
        false
    }
    /// Plays a sound at a location, to one player (`Some(uuid)`) or to everyone near it (`None`).
    /// Volume and pitch are as the script gives them (volume 1 and pitch 1 by default).
    fn play_sound(
        &self,
        _to: Option<u128>,
        _at: &Location,
        _sound: &str,
        _volume: f32,
        _pitch: f32,
    ) -> bool {
        false
    }
    /// Gives a potion effect (a namespaced id without `minecraft:`, such as `regeneration`) to a
    /// player or an entity. `amplifier` is the tier minus one; `duration_ticks` of `None` lasts
    /// until it is removed; `particles` says whether the swirl is shown.
    fn apply_effect(
        &self,
        _uuid: u128,
        _effect: &str,
        _amplifier: i32,
        _duration_ticks: Option<u64>,
        _particles: bool,
    ) -> bool {
        false
    }
    /// Hides the swirl of every potion effect a player or an entity has now.
    fn hide_effect_particles(&self, _uuid: u128) -> bool {
        false
    }
    /// Adds items to a player's or an entity's inventory.
    fn give_item(&self, _uuid: u128, _id: &str, _amount: i64) -> bool {
        false
    }
    /// Puts an item (`Some(id)`) or nothing (`None`) into an equipment slot.
    fn set_equipment(
        &self,
        _uuid: u128,
        _slot: &str,
        _id: Option<&str>,
        _name: Option<&str>,
    ) -> bool {
        false
    }
    /// Sets where a player respawns.
    fn set_respawn(&self, _uuid: u128, _at: &Location) -> bool {
        false
    }
}

/// The actions of a host that has none.
pub struct NoActions;

impl Actions for NoActions {}

/// A host for tests and tools: records what the scripts sent and logged.
#[derive(Default)]
pub struct Recorder {
    pub players: Vec<(u128, String)>,
    pub sent: RefCell<Vec<(u128, String)>>,
    pub broadcasts: RefCell<Vec<String>>,
    pub logs: RefCell<Vec<String>>,
    pub now: i64,
    /// Player states, by UUID (what `player_state` returns).
    pub states: Vec<(u128, PlayerState)>,
    /// Permission nodes granted, as (UUID, node).
    pub permissions: Vec<(u128, String)>,
    /// Pairs (viewer, target) that can see each other.
    pub sees: Vec<(u128, u128)>,
    /// Block ids by location (exact coordinates).
    pub blocks: Vec<(Location, String)>,
    /// What `target_block` returns for every player.
    pub target: Option<Location>,
    /// What `random_unit` returns.
    pub rand: f64,
    /// What the effects asked the engine to do, one line per action (`console kick Alice`).
    pub acted: RefCell<Vec<String>>,
    /// Entities of the world (what `entities_in_world` returns), NPCs included.
    pub entities: Vec<EntityInfo>,
    /// States of entities that are not online players (what `entity_state` returns).
    pub mobs: Vec<(u128, PlayerState)>,
    /// Equipment by (uuid, slot) (what `equipment` returns).
    pub equipment: Vec<(u128, String, ItemStack)>,
}

impl Recorder {
    pub fn with_players(players: &[(u128, &str)]) -> Self {
        Recorder {
            players: players.iter().map(|(u, n)| (*u, (*n).to_owned())).collect(),
            ..Recorder::default()
        }
    }
}

impl Host for Recorder {
    fn send(&self, uuid: u128, text: &str) {
        self.sent.borrow_mut().push((uuid, text.to_owned()));
    }
    fn broadcast(&self, text: &str) {
        self.broadcasts.borrow_mut().push(text.to_owned());
    }
    fn log(&self, line: &str) {
        self.logs.borrow_mut().push(line.to_owned());
    }
    fn player_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.players
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .cloned()
    }
    fn now_millis(&self) -> i64 {
        self.now
    }
    fn player_state(&self, uuid: u128) -> Option<PlayerState> {
        self.states
            .iter()
            .find(|(u, _)| *u == uuid)
            .map(|(_, s)| s.clone())
    }
    fn online_players(&self) -> Vec<(u128, String)> {
        self.players.clone()
    }
    fn has_permission(&self, uuid: u128, node: &str) -> bool {
        self.permissions
            .iter()
            .any(|(u, n)| *u == uuid && n.eq_ignore_ascii_case(node))
    }
    fn can_see(&self, viewer: u128, target: u128) -> bool {
        self.sees.contains(&(viewer, target))
    }
    fn block_type_at(&self, loc: &Location) -> Option<String> {
        self.blocks
            .iter()
            .find(|(l, _)| l == loc)
            .map(|(_, id)| id.clone())
    }
    fn target_block(&self, _uuid: u128, _range: f64) -> Option<Location> {
        self.target.clone()
    }
    fn random_unit(&self) -> f64 {
        self.rand
    }
    fn actions(&self) -> &dyn Actions {
        self
    }
    fn entities_in_world(&self, world: &str) -> Vec<EntityInfo> {
        self.entities
            .iter()
            .filter(|e| e.location.world.eq_ignore_ascii_case(world))
            .cloned()
            .collect()
    }
    fn entity_state(&self, uuid: u128) -> Option<PlayerState> {
        self.mobs
            .iter()
            .find(|(u, _)| *u == uuid)
            .map(|(_, s)| s.clone())
    }
    fn equipment(&self, uuid: u128, slot: &str) -> Option<ItemStack> {
        self.equipment
            .iter()
            .find(|(u, s, _)| *u == uuid && s == slot)
            .map(|(_, _, item)| item.clone())
    }
}

impl Actions for Recorder {
    fn run_console(&self, line: &str) -> bool {
        self.acted.borrow_mut().push(format!("console {line}"));
        true
    }
    fn run_as(&self, uuid: u128, line: &str) -> bool {
        self.acted.borrow_mut().push(format!("as {uuid} {line}"));
        true
    }
    fn action_bar(&self, uuid: u128, text: &str) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("action bar {uuid} {text}"));
        true
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
        self.acted.borrow_mut().push(format!(
            "title {uuid} {title}|{subtitle} {fade_in}/{stay}/{fade_out}"
        ));
        true
    }
    fn chat_as(&self, uuid: u128, text: &str) -> bool {
        self.acted.borrow_mut().push(format!("chat {uuid} {text}"));
        true
    }
    fn teleport(&self, uuid: u128, to: &Location) -> bool {
        self.acted.borrow_mut().push(format!(
            "teleport {uuid} {}",
            crate::value::Value::Location(to.clone()).to_text()
        ));
        true
    }
    fn set_block(&self, at: &Location, block: &str) -> bool {
        self.acted.borrow_mut().push(format!(
            "block {} {block}",
            crate::value::Value::Location(at.clone()).to_text()
        ));
        true
    }
    fn spawn(&self, at: &Location, entity: &str) -> bool {
        self.acted.borrow_mut().push(format!(
            "spawn {} {entity}",
            crate::value::Value::Location(at.clone()).to_text()
        ));
        true
    }
    fn set_health(&self, uuid: u128, health: f64) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("health {uuid} {health}"));
        true
    }
    fn heal(&self, uuid: u128, points: Option<f64>) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("heal {uuid} {points:?}"));
        true
    }
    fn set_food(&self, uuid: u128, food: i64) -> bool {
        self.acted.borrow_mut().push(format!("food {uuid} {food}"));
        true
    }
    fn push(&self, uuid: u128, velocity: [f64; 3]) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("push {uuid} {velocity:?}"));
        true
    }
    fn draw_particles(&self, at: &Location, spec: &ParticleSpec) -> bool {
        let colour = spec.colour.map_or(String::new(), |c| format!(" {c:?}"));
        self.acted.borrow_mut().push(format!(
            "particle {} {} x{}{colour} size {} at {}",
            if spec.force { "force" } else { "normal" },
            spec.name,
            spec.count,
            spec.size,
            crate::value::Value::Location(at.clone()).to_text()
        ));
        true
    }
    fn play_sound(
        &self,
        to: Option<u128>,
        at: &Location,
        sound: &str,
        volume: f32,
        pitch: f32,
    ) -> bool {
        let to = to.map_or("all".to_owned(), |u| u.to_string());
        self.acted.borrow_mut().push(format!(
            "sound {to} {sound} {volume}/{pitch} at {}",
            crate::value::Value::Location(at.clone()).to_text()
        ));
        true
    }
    fn apply_effect(
        &self,
        uuid: u128,
        effect: &str,
        amplifier: i32,
        duration_ticks: Option<u64>,
        particles: bool,
    ) -> bool {
        self.acted.borrow_mut().push(format!(
            "effect {uuid} {effect} {amplifier} {duration_ticks:?} particles={particles}"
        ));
        true
    }
    fn hide_effect_particles(&self, uuid: u128) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("hide effect particles {uuid}"));
        true
    }
    fn give_item(&self, uuid: u128, id: &str, amount: i64) -> bool {
        self.acted
            .borrow_mut()
            .push(format!("give {uuid} {id} {amount}"));
        true
    }
    fn set_equipment(&self, uuid: u128, slot: &str, id: Option<&str>, name: Option<&str>) -> bool {
        let named = name.map(|n| format!(" named {n:?}")).unwrap_or_default();
        self.acted
            .borrow_mut()
            .push(format!("equip {uuid} {slot} {id:?}{named}"));
        true
    }
    fn set_respawn(&self, uuid: u128, at: &Location) -> bool {
        self.acted.borrow_mut().push(format!(
            "respawn {uuid} {}",
            crate::value::Value::Location(at.clone()).to_text()
        ));
        true
    }
}
