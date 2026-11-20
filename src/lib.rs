//! horizon-skript as an arismoko native trait mod (`mod_api`). The language is the unchanged
//! `horizon-skript-core` crate in `core/` (tested without the engine); this file maps the engine's
//! hooks and host methods onto it: the scripts folder and its config, the `/skript` command, the
//! commands scripts add, join, quit, respawn, chat, click and damage events, messages to players,
//! and the tick that runs waits and repeats.
//!
//! Every `TODO(arismoko-api: ...)` marks a call that arismoko lacks; the fallback next to it is
//! described in PORT_NOTES.md, which also lists the names this file had to guess (`UNVERIFIED`).
//!
//! Ported from the horizon-rust glue (`api/src/lib.rs` of horizon-skript at 1453e0e61).

pub mod horizon_skript_core;
use crate::horizon_skript_core::Registry;
use crate::horizon_skript_core::host::{
    Actions, EntityInfo, Host as SkHost, ItemStack as SkItemStack, ParticleSpec, PlayerState,
};
use crate::horizon_skript_core::manager::{self, Manager};
use crate::horizon_skript_core::run::Event;
use crate::horizon_skript_core::value::{Location, Value};
use mod_api::{
    Command, CommandArgumentValue, CommandSource, Config, ConfigKind, ConfigLoadingEvent,
    ConfigReloadingEvent, EntityRef, EquipmentSlot, Flow, GameMode, Hand, Hooks, HostSide,
    Identifier, InteractionResult, ItemKey, ItemStack, LevelRandomHost, LivingDamagePreEvent, ParticleOptions,
    PlayerCommandsSentEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerRef, RegisterCommandsEvent,
    RunModCommandEvent, ServerAboutToStartEvent, ServerChatEvent, ServerHost, ServerStoppingEvent,
    ServerTickEvent, ServerTickPhase, StatType, Suggestion, TabCompleteEvent, Text, ParticleKey, Registrar, RegistrationContext,
    NodeDefault, PermissionError, PermissionGatherNodesEvent, PermissionLevel, PermissionNode,
    PermissionValueType,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MOD_ID: &str = "skript";
/// plugin.yml `permission:` of /skript (and /sk), default op.
const SKRIPT_NODE: &str = "skript.admin";

/// The typed node of `/skript` and `/sk` (`skript.admin`), declared at gathering with operator level 2.
fn skript_node() -> PermissionNode {
    PermissionNode::new(SKRIPT_NODE, PermissionValueType::Boolean)
}
/// The other permissions of Skript's plugin.yml (`skript.admin` is `SKRIPT_NODE`), all default op.
/// Declared so they can be granted; `skript.reloadnotify` is read when a script is reloaded.
const PLUGIN_NODES: [&str; 4] = [
    SKRIPT_NODE,
    "skript.effectcommands",
    "skript.reloadnotify",
    "skript.see_runtime_errors",
];
/// Bukkit's default line for a refused command (red, two sentences).
const NO_PERMISSION: &str = "\u{a7}cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error.";
/// The folder scripts are read from, relative to the server's folder (Skript's own default).
const DEFAULT_FOLDER: &str = "plugins/Skript/scripts";
const DEFAULT_TOML: &str = "# Folder of the .sk scripts, relative to the server folder.\nscripts-folder = \"plugins/Skript/scripts\"\n";
/// The tag horizon-citizens puts on every NPC it spawns (`citizens_npc`, `api/core/src/spawn.rs`).
const NPC_TAG: &str = "citizens_npc";
/// The vanilla dimension of the open world's Bukkit folder names.
const NETHER_FOLDER: &str = "world_nether";
const END_FOLDER: &str = "world_the_end";

#[derive(Default)]
struct Skript {
    st: Mutex<State>,
}

#[derive(Default)]
struct State {
    folder: Option<String>,
    /// `level-name` of server.properties: the folder of the open world (the overworld).
    level_name: String,
    manager: Option<Manager>,
    /// Events heard in hooks that have no server host (damage): fired on the next tick.
    /// HMOD-GAP(damage-cancel): the hook only has a level host, which cannot run the scripts, so
    /// `cancel event` on `on damage` cannot stop the hit.
    queued: Vec<(&'static str, Event)>,
    names: HashMap<u128, String>,
}

fn lock(m: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn log(line: &str) {
    eprintln!("[skript] {line}");
}

/// The language core's host, over the engine's host for one hook call.
struct Bridge<'a> {
    host: RefCell<&'a mut dyn ServerHost>,
    level_name: String,
    /// Script enable/reload/disable/unload requests of the running triggers, applied once the
    /// manager is free again (`with_manager`).
    script_requests: RefCell<Vec<(String, String)>>,
}

impl<'a> Bridge<'a> {
    /// Runs `f` with the engine's host. Every host call of the bridge goes through here.
    fn with<R>(&self, f: impl FnOnce(&mut dyn ServerHost) -> R) -> R {
        let mut guard = self.host.borrow_mut();
        f(&mut **guard)
    }

    /// The dimension a world name stands for: the open world's folder is the overworld, whatever
    /// its name (`stage` on a lobby); the Bukkit folders of nether and end. `None` otherwise.
    fn dimension_of(&self, world: &str) -> Option<&'static str> {
        if world == self.level_name {
            Some("minecraft:overworld")
        } else if world == NETHER_FOLDER {
            Some("minecraft:the_nether")
        } else if world == END_FOLDER {
            Some("minecraft:the_end")
        } else {
            None
        }
    }

    /// The world name a dimension is in (the inverse of `dimension_of`).
    fn world_of(&self, dimension: &str) -> String {
        match dimension {
            "minecraft:overworld" => self.level_name.clone(),
            "minecraft:the_nether" => NETHER_FOLDER.to_owned(),
            "minecraft:the_end" => END_FOLDER.to_owned(),
            other => other.to_owned(),
        }
    }

    /// The name of an online player, `None` for anyone else (NPCs are not online players).
    fn online_name(&self, uuid: u128) -> Option<String> {
        self.with(|h| {
            let p = h.player(uuid)?;
            p.name(&*h)
        })
    }
}

impl SkHost for Bridge<'_> {
    fn send(&self, uuid: u128, text: &str) {
        self.with(|h| {
            if let Some(p) = h.player(uuid) {
                let _ = p.tell(h, Text::literal(text.to_owned()));
            }
        });
    }

    fn broadcast(&self, text: &str) {
        // The messages service: a system line in every player's chat and the server's log.
        self.with(|h| {
            let _ = h.messages().broadcast(Text::literal(text.to_owned()));
        });
    }

    fn log(&self, line: &str) {
        eprintln!("{line}");
    }

    // `log "x" to file "name"`: one line appended to plugins/Skript/logs/<name>. The core already
    // refused names with folders; this checks again before touching the disk.
    fn script_action(&self, action: &str, name: &str) -> bool {
        self.script_requests
            .borrow_mut()
            .push((action.to_owned(), name.to_owned()));
        true
    }

    fn append_log(&self, file: &str, line: &str) -> bool {
        use std::io::Write;
        if file.is_empty() || file.contains(['/', '\\']) || file.contains("..") {
            return false;
        }
        let dir = std::path::Path::new("plugins/Skript/logs");
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(file))
            .and_then(|mut f| writeln!(f, "{line}"))
            .is_ok()
    }

    fn player_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.with(|h| {
            let p = h.players().player_by_name(name)?;
            Some((p.uuid(), p.name(&*h)?))
        })
    }

    fn online_players(&self) -> Vec<(u128, String)> {
        self.with(|h| {
            h.players()
                .server_players()
                .into_iter()
                .filter_map(|p| Some((p.uuid(), p.name(&*h)?)))
                .collect()
        })
    }

    // The scripts' `permission:` nodes and `player has permission`. The nodes the script files name
    // are declared at gathering (operator level 2, as Bukkit's default `op`). A node nobody declared
    // (a dynamic one such as `has permission "%{_x}%"`) is answered as Bukkit answers an unknown node:
    // for operators only. Any other error is a denial.
    fn has_permission(&self, uuid: u128, node: &str) -> bool {
        self.with(|h| {
            let Some(p) = h.player(uuid) else {
                return false;
            };
            match p.permission_bool(h, &PermissionNode::new(node, PermissionValueType::Boolean)) {
                Ok(answer) => answer,
                Err(PermissionError::Unregistered) => {
                    p.permission_level(&*h).is_some_and(|l| l >= 2)
                }
                Err(_) => false,
            }
        })
    }

    fn actions(&self) -> &dyn Actions {
        self
    }

    fn now_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    // Every entity of the world's level, as `all_entities` lists them (players and mobs, NPCs
    // included). A Citizens NPC of type PLAYER is a mannequin with the NPC tag: it is reported as
    // `minecraft:player`, so `is a player` holds for it and `all players` does not contain it.
    // Entities of a level that is not the one in place are listed too (levels() service).
    fn entities_in_world(&self, world: &str) -> Vec<EntityInfo> {
        let Some(dimension) = self.dimension_of(world) else {
            return Vec::new();
        };
        self.with(|h| {
            let Some(level) = h.level(dimension) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for e in level.all_entities(&*h).unwrap_or_default() {
                let (Some(kind), Some(at)) = (e.kind(&*h).map(|k| k.id(&*h)), e.position(&*h)) else {
                    continue;
                };
                let Some(uuid) = e.uuid(h) else {
                    continue;
                };
                let kind = if kind == "minecraft:mannequin" && is_npc(&*h, e) {
                    "minecraft:player".to_owned()
                } else {
                    kind
                };
                let [yaw, pitch] = e.rotation(&*h).unwrap_or([0.0, 0.0]);
                out.push(EntityInfo {
                    uuid,
                    kind,
                    name: e.name(&*h).unwrap_or_default(),
                    location: Location {
                        world: world.to_owned(),
                        x: at[0],
                        y: at[1],
                        z: at[2],
                        yaw,
                        pitch,
                    },
                });
            }
            out
        })
    }

    // The state of an NPC (an entity that is not an online player), read from the entity: position,
    // rotation, name, health, sneaking and sprinting, swimming, the main hand. Food, level, game mode
    // and the inventory are fixed values (as before the port; GAPS.md).
    fn entity_state(&self, uuid: u128) -> Option<PlayerState> {
        self.with(|h| {
            if h.players()
                .server_players()
                .iter()
                .any(|p| p.uuid() == uuid)
            {
                return None;
            }
            let (dimension, e) = find_entity(h, uuid)?;
            let world = self.world_of(dimension);
            let [x, y, z] = e.position(&*h)?;
            let [yaw, pitch] = e.rotation(&*h).unwrap_or([0.0, 0.0]);
            let name = e.name(&*h).unwrap_or_default();
            let living = h.living(e);
            let health = living.and_then(|l| l.health(&*h)).map_or(20.0, f64::from);
            let held = held_item(h, living);
            Some(PlayerState {
                display_name: name.clone(),
                name,
                location: Location {
                    world,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                },
                health,
                max_health: 20.0,
                food: 20,
                level: 0,
                gamemode: "survival".to_owned(),
                flying: false,
                can_fly: false,
                sneaking: e.is_shift_key_down(&*h).unwrap_or(false),
                sprinting: e.is_sprinting(&*h).unwrap_or(false),
                swimming: e.pose(&*h) == Some("swimming"),
                op: false,
                held,
                inventory: Vec::new(),
            })
        })
    }

    // An online player's state from the player's handle: position, world, rotation, health, food,
    // experience, game mode, flight, sneaking, sprinting, op (permission level 1+). Inventory is
    // not read here.
    fn player_state(&self, uuid: u128) -> Option<PlayerState> {
        self.with(|h| {
            let p = h.player(uuid)?;
            let name = p.name(&*h)?;
            let dimension = p.dimension(&*h)?.to_string();
            let world = self.world_of(&dimension);
            let e = p.entity();
            let [x, y, z] = e.position(&*h)?;
            let [yaw, pitch] = e.rotation(&*h).unwrap_or([0.0, 0.0]);
            let living = h.living(e);
            let health = living.and_then(|l| l.health(&*h)).map_or(20.0, f64::from);
            let food = p.food(&*h).map_or(20, |f| i64::from(f.level));
            let level = p.experience(&*h).map_or(0, |x| i64::from(x.level));
            let gamemode = match p.game_mode(&*h) {
                Some(GameMode::Creative) => "creative",
                Some(GameMode::Adventure) => "adventure",
                Some(GameMode::Spectator) => "spectator",
                _ => "survival",
            };
            let abilities = p.abilities(&*h);
            let held = held_item(h, living);
            Some(PlayerState {
                display_name: name.clone(),
                name,
                location: Location {
                    world,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                },
                health,
                max_health: 20.0,
                food,
                level,
                gamemode: gamemode.to_owned(),
                flying: abilities.is_some_and(|a| a.flying),
                can_fly: abilities.is_some_and(|a| a.may_fly),
                sneaking: e.is_shift_key_down(&*h).unwrap_or(false),
                sprinting: e.is_sprinting(&*h).unwrap_or(false),
                swimming: e.pose(&*h) == Some("swimming"),
                op: p.permission_level(&*h).is_some_and(|l| l >= 1),
                held,
                inventory: Vec::new(),
            })
        })
    }

    // The item in a helmet, chestplate, leggings or boots slot of an online player or an NPC.
    // Only the item id is kept (the core reads the id); the count is the stack's.
    // TODO(arismoko-api: item_custom_name): the stack's custom name is not read, so `name` is None.
    fn equipment(&self, uuid: u128, slot: &str) -> Option<SkItemStack> {
        let slot = equipment_of(slot)?;
        self.with(|h| {
            let (_, e) = entity_with_dimension(h, uuid)?;
            let stack = h.living(e)?.equipment(&*h, slot)?;
            Some(SkItemStack {
                id: item_id(&*h, stack.item()),
                amount: i64::from(stack.count()),
                name: None,
            })
        })
    }
}

impl Actions for Bridge<'_> {
    fn run_console(&self, line: &str) -> bool {
        self.with(|h| run_console_line(h, line))
    }

    fn set_game_mode(&self, uuid: u128, mode: i32) -> bool {
        let Some(gm) = GameMode::from_id(mode) else {
            return false;
        };
        self.with(|h| {
            let Some(p) = h.player(uuid) else {
                return false;
            };
            p.set_game_mode(h, gm)
        })
    }

    // TODO(arismoko-api: run_command_as): arismoko has no run-as-player call. Running the
    // line from the console would give it the console's permission level, so the line is refused.
    fn run_as(&self, _uuid: u128, _line: &str) -> bool {
        false
    }

    fn action_bar(&self, uuid: u128, text: &str) -> bool {
        self.with(|h| {
            let Some(p) = h.player(uuid) else {
                return false;
            };
            matches!(p.action_bar(h, Text::literal(text.to_owned())), Ok(true))
        })
    }

    // TODO(arismoko-api: player_show_title): the title is sent with the vanilla `title` command
    // (times, then title and subtitle as JSON text) run by the console.
    fn title(
        &self,
        uuid: u128,
        title: &str,
        subtitle: &str,
        fade_in: u64,
        stay: u64,
        fade_out: u64,
    ) -> bool {
        self.with(|h| {
            let Some(p) = h.player(uuid) else {
                return false;
            };
            let Some(name) = p.name(&*h) else {
                return false;
            };
            let times = format!("title {name} times {fade_in} {stay} {fade_out}");
            let main = format!("title {name} title {}", json_text(title));
            let sub = format!("title {name} subtitle {}", json_text(subtitle));
            [times, main, sub]
                .iter()
                .all(|line| run_console_line(h, line))
        })
    }

    // TODO(arismoko-api: chat_as): no server-side way to make a player chat; refused.
    fn chat_as(&self, _uuid: u128, _text: &str) -> bool {
        false
    }

    fn teleport(&self, uuid: u128, to: &Location) -> bool {
        let Some(dimension) = self.dimension_of(&to.world) else {
            return false;
        };
        self.with(|h| {
            let Some((current, e)) = entity_with_dimension(h, uuid) else {
                return false;
            };
            let pos = [to.x, to.y, to.z];
            let rot = Some([to.yaw, to.pitch]);
            if current == dimension {
                e.teleport_to(h, pos, rot)
            } else {
                e.teleport_to_level(h, dimension, pos, rot)
            }
        })
    }

    // TODO(arismoko-api: level_set_block_by_name): a block needs its default state
    // (`block_key` then `block_state_default`) and the flags of `set_block`; not wired yet.
    fn set_block(&self, _at: &Location, _block: &str) -> bool {
        false
    }

    // TODO(arismoko-api: summon_with_empty_nbt): `summon` needs the entity's NBT; not wired yet.
    fn spawn(&self, _at: &Location, _entity: &str) -> bool {
        false
    }

    fn set_health(&self, uuid: u128, health: f64) -> bool {
        self.with(|h| {
            let Some((_, e)) = entity_with_dimension(h, uuid) else {
                return false;
            };
            let Some(living) = h.living(e) else {
                return false;
            };
            living.set_health(h, health as f32)
        })
    }

    fn heal(&self, uuid: u128, points: Option<f64>) -> bool {
        match points {
            // Full health is the maximum (20; the attribute is not read here).
            None => self.set_health(uuid, 20.0),
            Some(points) => self.with(|h| {
                let Some((_, e)) = entity_with_dimension(h, uuid) else {
                    return false;
                };
                let Some(living) = h.living(e) else {
                    return false;
                };
                living.heal(h, points as f32)
            }),
        }
    }

    fn set_food(&self, uuid: u128, food: i64) -> bool {
        // Saturation and exhaustion stay as they are: only the food level is set.
        self.with(|h| {
            let Some(p) = h.player(uuid) else {
                return false;
            };
            p.set_food_level(h, food as i32)
        })
    }

    fn push(&self, uuid: u128, velocity: [f64; 3]) -> bool {
        self.with(|h| {
            let Some((_, e)) = entity_with_dimension(h, uuid) else {
                return false;
            };
            e.push(h, velocity)
        })
    }

    // The level's particles, as the level sends them: the players within 32 blocks.
    // TODO(arismoko-api: player_send_particles_forced): Skript's `force draw` reaches every player
    // of the world; without a per-player send the force flag is ignored here.
    fn draw_particles(&self, at: &Location, spec: &ParticleSpec) -> bool {
        let Some(key) = particle_by_name(&particle_id(&spec.name)) else {
            return false;
        };
        let Ok(count) = i32::try_from(spec.count) else {
            return false;
        };
        let options = match spec.colour {
            Some([r, g, b]) if spec.name == "dust" => ParticleOptions::dust(
                key,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                ],
                spec.size as f32,
            ),
            _ => ParticleOptions::simple(key),
        };
        let Some(dimension) = self.dimension_of(&at.world) else {
            return false;
        };
        let pos = [at.x, at.y, at.z];
        self.with(|h| {
            let Some(level) = h.level(dimension) else {
                return false;
            };
            level.send_particles(h, &options, None, pos, [0.0; 3], spec.extra, count)
        })
    }

    // The vanilla `playsound` command, run in the sound's dimension. Players outside the list of
    // online players (NPCs) cannot be named, so a sound for one of them is refused.
    fn play_sound(
        &self,
        to: Option<u128>,
        at: &Location,
        sound: &str,
        volume: f32,
        pitch: f32,
    ) -> bool {
        let Some(dimension) = self.dimension_of(&at.world) else {
            return false;
        };
        let target = match to {
            None => "@a".to_owned(),
            Some(uuid) => match self.online_name(uuid) {
                Some(name) => name,
                None => return false,
            },
        };
        let (x, y, z) = (at.x, at.y, at.z);
        self.with(|h| {
            run_console_line(
                h,
                &format!(
                    "execute in {dimension} run playsound {sound} master {target} {x} {y} {z} {volume} {pitch}"
                ),
            )
        })
    }

    // The vanilla `effect give` command. Only online players can be named (NPCs: not supported).
    // Without a duration the effect lasts a long time (1000000 seconds, as before the port).
    fn apply_effect(
        &self,
        uuid: u128,
        effect: &str,
        amplifier: i32,
        duration_ticks: Option<u64>,
        particles: bool,
    ) -> bool {
        let Some(name) = self.online_name(uuid) else {
            return false;
        };
        let seconds = duration_ticks.map_or(1_000_000, |t| t / 20);
        self.with(|h| {
            run_console_line(
                h,
                &format!(
                    "effect give {name} minecraft:{effect} {seconds} {amplifier} {}",
                    !particles
                ),
            )
        })
    }

    // TODO(arismoko-api: living_effect_hide_particles): no way to hide the swirls of effects that
    // are already on; refused.
    fn hide_effect_particles(&self, _uuid: u128) -> bool {
        false
    }

    // The vanilla `give` command, for online players (NPCs: not supported).
    fn give_item(&self, uuid: u128, id: &str, amount: i64) -> bool {
        let Some(name) = self.online_name(uuid) else {
            return false;
        };
        self.with(|h| run_console_line(h, &format!("give {name} {id} {amount}")))
    }

    // The vanilla `item replace entity` command, for online players. The item's custom name is a
    // plain-text `minecraft:custom_name` component, as Skript's `named "..."` writes it on Paper.
    // An NPC or any other living entity gets its slot from the engine (`living_set_equipment`), with
    // the same name set on the stack.
    fn set_equipment(&self, uuid: u128, slot: &str, id: Option<&str>, name: Option<&str>) -> bool {
        if let Some(player) = self.online_name(uuid) {
            let Some(target) = equipment_slot(slot) else {
                return false;
            };
            let item = match (id, name) {
                (Some(id), Some(n)) => format!("{id}[custom_name={}]", snbt_string(n)),
                (Some(id), None) => id.to_owned(),
                (None, _) => "minecraft:air".to_owned(),
            };
            return self.with(|h| {
                run_console_line(
                    h,
                    &format!("item replace entity {player} {target} with {item}"),
                )
            });
        }
        let Some(target) = equipment_of(slot) else {
            return false;
        };
        self.with(|h| {
            let Some((_, entity)) = find_entity(h, uuid) else {
                return false;
            };
            let Some(living) = h.living(entity) else {
                return false;
            };
            let stack = match id {
                None => None,
                Some(id) => {
                    let Ok(id) = Identifier::parse(id) else {
                        return false;
                    };
                    let Some(key) = h.item_key(&id) else {
                        return false;
                    };
                    let Some(stack) = ItemStack::new(key, 1) else {
                        return false;
                    };
                    // HMOD-GAP(item-display): main has no ItemStack setter for a custom name, so a living
                    // entity's equipment gets the bare item (the name is not set).
                    let _ = name;
                    Some(stack)
                }
            };
            living
                .set_equipment(h, target, stack)
                .is_ok_and(|done| done)
        })
    }

    // TODO(arismoko-api: player_set_respawn_point): the vanilla `spawnpoint` command, run in the
    // respawn location's dimension, for an online player (NPCs: not supported).
    fn set_respawn(&self, uuid: u128, at: &Location) -> bool {
        let Some(name) = self.online_name(uuid) else {
            return false;
        };
        let Some(dimension) = self.dimension_of(&at.world) else {
            return false;
        };
        let (x, y, z) = (at.x, at.y, at.z);
        self.with(|h| {
            run_console_line(
                h,
                &format!("execute in {dimension} run spawnpoint {name} {x} {y} {z}"),
            )
        })
    }
}

/// Runs `line` as the server's console does (`Commands::run_command`). Whether it ran.
fn run_console_line(h: &mut dyn ServerHost, line: &str) -> bool {
    h.commands().run_command(line)
}

/// The NPC tag check of a mannequin (`entity_tags`).
fn is_npc(h: &dyn ServerHost, e: EntityRef) -> bool {
    e.tags(h)
        .is_some_and(|tags| tags.iter().any(|tag| tag == NPC_TAG))
}

/// The engine's name of an item key (`item_key_id`).
// UNVERIFIED: `item_key_id` as a host method (keys area) on `dyn ServerHost`.
fn item_id(h: &dyn ServerHost, key: ItemKey) -> String {
    key.id(h)
}

/// The item id in an NPC's or a player's main hand, or `minecraft:air`.
fn held_item(h: &dyn ServerHost, living: Option<mod_api::LivingRef>) -> String {
    match living.and_then(|l| l.hand_item(h, Hand::Main)) {
        Some(stack) => item_id(h, stack.item()),
        None => "minecraft:air".to_owned(),
    }
}

/// The entity an NPC or an online player is by UUID, with the dimension it is in. Players come
/// from the player list; any other entity from the levels (`None` when no level has it).
fn entity_with_dimension(h: &mut dyn ServerHost, uuid: u128) -> Option<(&'static str, EntityRef)> {
    if let Some(p) = h.player(uuid) {
        let dimension = p.dimension(&*h)?;
        return Some((dimension_static(&dimension.to_string()), p.entity()));
    }
    find_entity(h, uuid)
}

/// An entity of any loaded level by its UUID, with its level's dimension.
fn find_entity(h: &mut dyn ServerHost, uuid: u128) -> Option<(&'static str, EntityRef)> {
    for level in h.levels().list_levels() {
        for e in level.all_entities(&*h).unwrap_or_default() {
            if e.uuid(h) == Some(uuid) {
                return Some((level.dimension_id(), e));
            }
        }
    }
    None
}

/// The dimension names of the vanilla levels as statics, so a player's dimension can be compared
/// with the bridge's. Any other dimension is `minecraft:overworld`-free text: it never matches.
fn dimension_static(name: &str) -> &'static str {
    match name {
        "minecraft:overworld" => "minecraft:overworld",
        "minecraft:the_nether" => "minecraft:the_nether",
        "minecraft:the_end" => "minecraft:the_end",
        _ => "",
    }
}

/// The particle id of a particle the scripts draw by name (`end rod`, `totem`).
/// The vanilla particles a script may name, retained in `register` (a mod cannot look a particle
/// up by name later: `particle_named_key` is engine-only now).
// TODO(arismoko-api: particle_key_by_name): a name lookup at run time would cover every particle.
const PARTICLE_NAMES: &[&str] = &[
    "dust", "portal", "reverse_portal", "splash", "end_rod", "flame", "soul_fire_flame", "heart",
    "happy_villager", "angry_villager", "witch", "enchant", "crit", "enchanted_hit", "smoke",
    "large_smoke", "cloud", "totem_of_undying", "electric_spark", "note", "glow", "firework",
    "dragon_breath", "snowflake", "white_ash", "ash", "effect", "instant_effect", "lava",
    "dripping_water", "falling_water", "bubble", "soul", "sculk_soul", "wax_on", "wax_off",
    "scrape", "poof", "explosion", "sweep_attack", "damage_indicator", "composter",
    "item_slime", "cherry_leaves", "campfire_cosy_smoke", "nautilus", "dolphin", "squid_ink",
    "glow_squid_ink", "mycelium", "spore_blossom_air", "falling_spore_blossom", "trial_spawner_detection",
];

static PARTICLE_KEYS: Mutex<Option<HashMap<String, ParticleKey>>> = Mutex::new(None);

fn particle_by_name(id: &str) -> Option<ParticleKey> {
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    PARTICLE_KEYS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(name).copied()
}

fn particle_id(name: &str) -> String {
    match name {
        "totem" => "totem_of_undying".to_owned(),
        other => other.replace(' ', "_"),
    }
}

/// The equipment slot of a script's slot name.
fn equipment_of(slot: &str) -> Option<EquipmentSlot> {
    match slot {
        "helmet" => Some(EquipmentSlot::Head),
        "chestplate" => Some(EquipmentSlot::Chest),
        "leggings" => Some(EquipmentSlot::Legs),
        "boots" => Some(EquipmentSlot::Feet),
        _ => None,
    }
}

/// The vanilla equipment slot of a script's slot name, for the `item replace` command.
fn equipment_slot(slot: &str) -> Option<&'static str> {
    match slot {
        "helmet" => Some("armor.head"),
        "chestplate" => Some("armor.chest"),
        "leggings" => Some("armor.legs"),
        "boots" => Some("armor.feet"),
        _ => None,
    }
}

/// A quoted SNBT string.
fn snbt_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A plain-text JSON component for a title command.
fn json_text(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("{{\"text\":\"{escaped}\"}}")
}

/// The level name from server.properties (`level-name`), `world` when missing.
/// Whether a script command's name can be registered: lower-case letters, digits, `_` and `-`.
fn is_command_label(label: &str) -> bool {
    !label.is_empty()
        && label != "skript"
        && label != "sk"
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn level_name_from_properties(text: &str) -> String {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("level-name="))
        .map(str::to_owned)
        .next()
        .unwrap_or_else(|| "world".to_owned())
}

impl Skript {
    /// Runs `f` with the manager, loading the scripts the first time (and after the folder changed).
    fn with_manager<R>(
        &self,
        host: &mut dyn ServerHost,
        f: impl FnOnce(&mut State, &mut Manager, &Bridge<'_>) -> R,
    ) -> R {
        let mut st = lock(&self.st);
        let bridge = Bridge {
            host: RefCell::new(host),
            level_name: st.level_name.clone(),
            script_requests: RefCell::new(Vec::new()),
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
        for (action, name) in bridge.script_requests.take() {
            match action.as_str() {
                "reload" => {
                    for line in m.reload(&bridge, &name) {
                        log(&line);
                    }
                }
                "enable" => {
                    if let Err(e) = m.enable(&bridge, &name) {
                        log(&e);
                    }
                }
                _ => {
                    if let Err(e) = m.disable(&bridge, &name) {
                        log(&e);
                    }
                }
            }
        }
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
            None => log("config has no scripts-folder, keeping the old one"),
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
fn fire_cancellable(sk: &Skript, host: &mut dyn ServerHost, names: &[&str], data: Event) -> Flow {
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

/// A block position as a location in the world of `dimension`.
fn block_location(level_name: &str, dimension: &str, pos: [i32; 3]) -> Value {
    let world = match dimension {
        "minecraft:overworld" => level_name.to_owned(),
        "minecraft:the_nether" => NETHER_FOLDER.to_owned(),
        "minecraft:the_end" => END_FOLDER.to_owned(),
        other => other.to_owned(),
    };
    Value::Location(Location {
        world,
        x: f64::from(pos[0]),
        y: f64::from(pos[1]),
        z: f64::from(pos[2]),
        yaw: 0.0,
        pitch: 0.0,
    })
}

/// The dimension a player is in, from the player's handle.
fn player_dimension(h: &dyn ServerHost, p: PlayerRef) -> Option<String> {
    p.dimension(h).map(|d| d.to_string())
}

/// The world a dimension id stands for, with the same names `block_location` gives.
fn dimension_world(level_name: &str, dimension: &str) -> String {
    match block_location(level_name, dimension, [0, 0, 0]) {
        Value::Location(l) => l.world,
        _ => dimension.to_owned(),
    }
}

/// A point of a dimension as a location value.
fn point_location(level_name: &str, dimension: &str, p: [f64; 3]) -> Value {
    Value::Location(Location {
        world: dimension_world(level_name, dimension),
        x: p[0],
        y: p[1],
        z: p[2],
        yaw: 0.0,
        pitch: 0.0,
    })
}

/// The lower-case game mode name a script reads.
fn game_mode_name(mode: &GameMode) -> &'static str {
    match mode {
        GameMode::Creative => "creative",
        GameMode::Adventure => "adventure",
        GameMode::Spectator => "spectator",
        _ => "survival",
    }
}

/// An item id as an item type value.
fn item_value(id: &str, amount: i64) -> Value {
    Value::ItemType {
        id: id.to_owned(),
        amount,
        name: None,
    }
}

/// The dimension the player with this UUID is in.
fn player_dim_of(host: &mut dyn ServerHost, uuid: u128) -> Option<String> {
    host.player(uuid).and_then(|p| player_dimension(&*host, p))
}

/// Fires the event `name` for `uuid` (when there is a player) with the extra values, and says
/// whether a trigger cancelled it. The player's name comes from the join hook.
fn fire_event(
    sk: &Skript,
    host: &mut dyn ServerHost,
    name: &str,
    uuid: Option<u128>,
    values: Vec<(&'static str, Value)>,
) -> bool {
    let mut data = match uuid {
        Some(u) => player_event(sk, u),
        None => Event::default(),
    };
    for (k, v) in values {
        data = data.with(k, v);
    }
    matches!(fire_cancellable(sk, host, &[name], data), Flow::Cancel)
}

/// The answer of a hook that may veto: cancel when a trigger cancelled the event.
fn verdict(cancelled: bool) -> Flow {
    if cancelled {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

#[mod_api::horizon_mod]
impl Hooks for Skript {
    fn register_configs(&self, _: HostSide, event: &mut mod_api::RegisterConfigsEvent<'_>) {
        event
            .configs
            .push(Config::new(MOD_ID, ConfigKind::Common, DEFAULT_TOML));
    }

    fn config_loading(&self, _: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == MOD_ID {
            self.load_config(&event.config.text);
        }
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == MOD_ID {
            self.load_config(&event.config.text);
        }
    }

    fn server_about_to_start(&self, host: &mut dyn ServerHost, _: &mut ServerAboutToStartEvent) {
        let level_name = std::fs::read_to_string("server.properties")
            .map(|p| level_name_from_properties(&p))
            .unwrap_or_else(|_| "world".to_owned());
        lock(&self.st).level_name = level_name;
        self.with_manager(host, |_, m, b| m.fire(b, "server start", Event::default()));
    }

    fn server_stopping(&self, host: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        self.with_manager(host, |_, m, b| m.fire(b, "server stop", Event::default()));
    }

    /// Declares the plugin.yml permissions (default op: operator level 2 without a permission provider's
    /// answer) and every node the script files name (`permission:` lines, `has permission "..."`).
    fn permission_gather_nodes(&self, event: &mut PermissionGatherNodesEvent<'_>) {
        let folder = lock(&self.st)
            .folder
            .clone()
            .unwrap_or_else(|| DEFAULT_FOLDER.to_owned());
        let mut names: Vec<String> = PLUGIN_NODES.iter().map(|n| (*n).to_owned()).collect();
        names.extend(manager::scan_permission_nodes(std::path::Path::new(&folder)));
        names.sort();
        names.dedup();
        for name in names {
            let _ = event.nodes.add(
                PermissionNode::new(name, PermissionValueType::Boolean),
                NodeDefault::Level(PermissionLevel::from_level(2)),
            );
        }
    }

    fn register_commands(&self, host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        // plugin.yml: `skript`, alias `sk`, permission skript.admin (default op). The node decides when a
        // permissions mod answers; the level 2 only without one. `player_command` gives the Bukkit line.
        for usage in ["skript", "skript <args:text>", "sk", "sk <args:text>"] {
            event.commands.push(Command::op(usage, 2).with_node(&skript_node()));
        }
        // The scripts' `command /name:` triggers and their aliases (loads the scripts on the first call).
        let infos = self.with_manager(host, |_, m, _| m.command_infos());
        let mut seen = std::collections::HashSet::new();
        for (name, info) in infos {
            for label in std::iter::once(name).chain(info.aliases) {
                if !is_command_label(&label) || !seen.insert(label.clone()) {
                    continue;
                }
                event.commands.push(Command::new(&label));
                event
                    .commands
                    .push(Command::new(&format!("{label} <args:text>")));
            }
        }
    }

    // Registered commands reach this hook after the permission check and the argument parse.
    // `/skript` runs for the console and for players; a script command runs for a player and for
    // the console, as in Skript (its `executable by:` and `permission:` decide the rest).
    // Anything else is not ours.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        let Some(root) = event.command.literals.first().cloned() else {
            return Flow::Continue;
        };
        let args = match event.command.argument("args") {
            Some(CommandArgumentValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let sender: Option<(u128, String)> = match &event.source {
            CommandSource::Player { player, .. } => {
                let name = player.name(&*host).unwrap_or_default();
                Some((player.uuid(), name))
            }
            _ => None,
        };
        if root == "skript" || root == "sk" {
            let refresh = self.with_manager(host, |_, m, b| {
                let before = m.command_registrations();
                for line in m.skript_command_as(b, &args, sender.as_ref().map(|(u, _)| *u)) {
                    match sender.as_ref() {
                        Some((uuid, _)) => b.send(*uuid, &line),
                        None => b.log(&line),
                    }
                }
                before != m.command_registrations()
            });
            if refresh {
                // The scripts' commands changed: every mod is asked again and the trees are resent.
                host.commands().refresh_commands();
            }
            return Flow::Cancel;
        }
        let op = match &event.source {
            CommandSource::Player { permission, .. } => permission.level() >= 2,
            CommandSource::Console { .. } => true,
            _ => return Flow::Continue,
        };
        let sender = manager::CommandSender { player: sender, op };
        self.with_manager(host, |_, m, b| {
            if m.run_command(b, &root, &sender, &args) {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        })
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(
            event.phase,
            ServerTickPhase::ServerTickStart {
                world_running: true,
                ..
            }
        ) {
            return;
        }
        self.with_manager(host, |st, m, b| {
            for (name, data) in std::mem::take(&mut st.queued) {
                m.fire(b, name, data);
            }
            m.tick(b);
        });
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        let uuid = event.player;
        let name = event.name.to_string();
        lock(&self.st).names.insert(uuid, name.clone());
        let data = Event {
            player: Some(player_value(uuid, &name)),
            ..Event::default()
        };
        // TODO(arismoko-api: player_has_played_before): the vanilla `minecraft:leave_game` stat
        // stands in: 0 (or no stat) means the player never left, so this is the first join. The
        // stat is read at this hook and may not be loaded yet, which would repeat `first join`.
        let first = host
            .player(uuid)
            .and_then(|p| p.stat(&*host, StatType::Custom, "minecraft:leave_game"))
            .is_some_and(|n| n == 0);
        self.with_manager(host, |_, m, b| {
            if first {
                m.fire(b, "first join", data.clone());
            }
            m.fire(b, "join", data);
        });
    }

    fn player_logged_out(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
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

    // -- events of the syntax library. The hooks with a host can cancel; the others queue the event.

    // UNVERIFIED: `player` as the respawned event's field (the hook doc names the player).
    fn player_respawned(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerRespawnedEvent,
    ) {
        let data = player_event(self, event.player);
        let _ = fire_cancellable(self, host, &["respawn"], data);
    }

    // UNVERIFIED: `player` as the died event's field (the hook doc names the player).
    fn player_died(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerDiedEvent) {
        let data = player_event(self, event.player);
        let _ = fire_cancellable(self, host, &["death"], data);
    }

    // No server host here, so the damage event is queued for the tick and cannot be cancelled.
    // UNVERIFIED: `kind` (the entity's type name) and `container.original()` (the damage before
    // the armor and the enchantments took their share).
    fn living_damage_pre(&self, _: &mut dyn LevelRandomHost, event: &mut LivingDamagePreEvent<'_>) {
        let data = Event::default()
            .with("victim", Value::EntityType(event.kind.to_owned()))
            .with("damage", Value::Num(f64::from(event.container.original())));
        lock(&self.st).queued.push(("damage", data));
    }

    // UNVERIFIED: `pos` (block position) and `canceled` (the out flag, set to keep the block).
    fn break_block(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::BreakBlockEvent<'_>,
    ) -> Flow {
        let Some(np) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dimension) = player_dimension(&*host, np) else {
            return Flow::Continue;
        };
        let level_name = lock(&self.st).level_name.clone();
        let at = block_location(&level_name, &dimension, event.pos);
        let data = player_event(self, np.uuid()).with("location", at);
        if matches!(fire_cancellable(self, host, &["break"], data), Flow::Cancel) {
            event.cancelled = true;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    // TODO(arismoko-api: player_place_block): Skript's `place` trigger has no arismoko hook yet;
    // no `place` event fires. The lobby scripts do not use it.

    // UNVERIFIED: `click.pos` and `click.player` (the BlockClick of the event), and the hand (the
    // hook fires once per hand; the main hand is the script's click).
    fn right_click_block(
        &self,
        host: HostSide<'_>,
        event: &mut mod_api::RightClickBlockEvent,
    ) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        if !matches!(event.click.hand, Hand::Main) {
            return Flow::Continue;
        }
        let uuid = event.click.player;
        let Some(dimension) = host.player(uuid).and_then(|p| player_dimension(&*host, p)) else {
            return Flow::Continue;
        };
        let level_name = lock(&self.st).level_name.clone();
        let at = block_location(&level_name, &dimension, event.click.pos);
        let data = player_event(self, uuid).with("location", at);
        if matches!(
            fire_cancellable(self, host, &["right click", "click"], data),
            Flow::Cancel
        ) {
            *event.cancellation_result = InteractionResult::Fail;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    // UNVERIFIED: `pos` and `canceled`, as the break hook.
    fn left_click_block(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::LeftClickBlockEvent,
    ) -> Flow {
        let Some(np) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dimension) = player_dimension(&*host, np) else {
            return Flow::Continue;
        };
        let level_name = lock(&self.st).level_name.clone();
        let at = block_location(&level_name, &dimension, event.pos);
        let data = player_event(self, np.uuid()).with("location", at);
        if matches!(
            fire_cancellable(self, host, &["left click", "click"], data),
            Flow::Cancel
        ) {
            event.cancelled = true;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    // The `move` trigger fires from the `player_move` hook below.

    /// Tab completion of `/skript` and `/sk`: the sub-commands, `all|scripts|config|aliases|last` and
    /// the script names, for a sender with skript.admin.
    fn tab_complete(&self, host: &mut dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
        let line = event.command.trim_start_matches('/');
        let (root, rest) = line.split_once(' ').unwrap_or((line, ""));
        let root = root.to_ascii_lowercase();
        if root != "skript" && root != "sk" {
            return;
        }
        let admin = match host.player(event.player) {
            Some(p) => p.permission_bool(host, &skript_node()).unwrap_or(false),
            None => false,
        };
        if !admin {
            return;
        }
        let words = self.with_manager(host, |_, m, _| m.completions(rest));
        for word in words {
            event
                .suggestions
                .push(Suggestion::new((event.range[0], event.range[1]), word));
        }
    }

    /// Tab: `/skript` and `/sk` only for a sender with skript.admin (Bukkit hides a command by its permission).
    fn player_commands_sent(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerCommandsSentEvent<'_>,
    ) {
        let admin = match host.player(event.player) {
            Some(p) => p.permission_bool(host, &skript_node()).unwrap_or(false),
            None => false,
        };
        if !admin {
            event.names.retain(|n| {
                let lower = n.to_ascii_lowercase();
                let bare = lower.rsplit(':').next().unwrap_or("");
                bare != "skript" && bare != "sk"
            });
        }
    }

    // UNVERIFIED: `player` (the UUID) and `command` (the typed line, without its slash).
    fn player_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerCommandEvent,
    ) -> Flow {
        // Bukkit's `permission:` check for /skript and its alias /sk: the default line, nothing runs.
        let root = event.command.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if root == "skript" || root == "sk" {
            let admin = match host.player(event.player) {
                Some(p) => p.permission_bool(host, &skript_node()).unwrap_or(false),
                None => false,
            };
            if !admin {
                event.replies.push(Text::literal(NO_PERMISSION));
                event.cancelled = true;
                return Flow::Cancel;
            }
        }
        let mut data = player_event(self, event.player);
        data.message = Some(event.command.to_owned());
        fire_cancellable(self, host, &["command"], data)
    }

    // The chat line runs the `on chat` triggers at once: `cancel event` stops the message and
    // `set message to ...` changes what players see (Skript's AsyncPlayerChatEvent).
    fn server_chat(&self, host: &mut dyn ServerHost, event: &mut ServerChatEvent<'_>) -> Flow {
        let data = Event {
            player: Some(player_value(event.player, event.username)),
            message: Some(event.raw_text.to_owned()),
            ..Event::default()
        };
        let probe = data.clone();
        self.with_manager(host, |_, m, b| m.fire(b, "chat", data));
        if let Some(text) = probe.changed_message() {
            *event.message = Text::literal(text);
        }
        if probe.is_cancelled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    // -- more events (`syntax/events/misc.rs` lists the values each one carries). Hooks that can
    // veto set the cancel flag of their event when a trigger cancelled it.

    // UNVERIFIED: the field names of the events below are the reference's.
    fn player_teleported(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerTeleportedEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("from", point_location(&level, event.dimension, event.from)),
            ("to", point_location(&level, event.dimension, event.to)),
        ];
        fire_event(self, host, "teleport", Some(event.player), values);
    }

    fn player_changed_dimension(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerChangedDimensionEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("from", Value::World(dimension_world(&level, event.from))),
            ("to", Value::World(dimension_world(&level, event.to))),
        ];
        fire_event(self, host, "world change", Some(event.player), values);
    }

    fn player_game_mode_changed(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerGameModeChangedEvent<'_>,
    ) -> Flow {
        let values = vec![
            ("from", Value::Text(game_mode_name(&event.from).to_owned())),
            ("gamemode", Value::Text(game_mode_name(&*event.to).to_owned())),
        ];
        let cancelled = fire_event(self, host, "game mode change", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_toggle_flight(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerToggleFlightEvent<'_>,
    ) -> Flow {
        let values = vec![("state", Value::Bool(event.flying))];
        let cancelled = fire_event(self, host, "flight toggle", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_toggle_sneak(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerToggleSneakEvent<'_>,
    ) {
        let values = vec![("state", Value::Bool(event.sneaking))];
        fire_event(self, host, "sneak toggle", Some(event.player), values);
    }

    fn player_sprint(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerSprintEvent<'_>) {
        let values = vec![("state", Value::Bool(event.sprinting))];
        fire_event(self, host, "sprint toggle", Some(event.player), values);
    }

    fn player_jump(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerJumpEvent<'_>) {
        fire_event(self, host, "jump", Some(event.player), Vec::new());
    }

    fn player_container_open(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerContainerOpenEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(pos) = event.pos {
            values.push(("location", block_location(&level, event.dimension, pos)));
        }
        fire_event(self, host, "inventory open", Some(event.player), values);
    }

    fn player_container_close(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerContainerCloseEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(pos) = event.pos {
            values.push(("location", block_location(&level, event.dimension, pos)));
        }
        fire_event(self, host, "inventory close", Some(event.player), values);
    }

    fn player_destroy_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerDestroyItemEvent<'_>,
    ) {
        let values = vec![("item", item_value(event.item, 1))];
        fire_event(self, host, "item break", Some(event.player), values);
    }

    fn food_level_changed(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::FoodLevelChangedEvent<'_>,
    ) -> Flow {
        let values = vec![
            ("from", Value::Int(i64::from(event.from))),
            ("to", Value::Int(i64::from(event.to))),
        ];
        let cancelled = fire_event(self, host, "hunger change", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_level_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerLevelChangeEvent<'_>,
    ) {
        let values = vec![
            ("from", Value::Int(i64::from(event.from))),
            ("to", Value::Int(i64::from(event.to))),
        ];
        fire_event(self, host, "level change", Some(event.player), values);
    }

    fn player_xp_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerXpChangeEvent<'_>,
    ) -> Flow {
        let values = vec![("number", Value::Int(i64::from(event.amount)))];
        let cancelled = fire_event(self, host, "experience change", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_kicked(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerKickedEvent<'_>) {
        let mut data = player_event(self, event.player);
        data.message = Some(event.reason.to_owned());
        let _ = fire_cancellable(self, host, &["kick"], data);
    }

    // Skript's `move` fires when the player enters another block, not on every packet.
    fn player_move(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerMoveEvent) -> Flow {
        let block = |p: [f64; 3]| [p[0].floor() as i64, p[1].floor() as i64, p[2].floor() as i64];
        if block(event.from) == block(event.to) {
            return Flow::Continue;
        }
        let Some(dimension) = player_dim_of(host, event.player) else {
            return Flow::Continue;
        };
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("from", point_location(&level, &dimension, event.from)),
            ("to", point_location(&level, &dimension, event.to)),
        ];
        let cancelled = fire_event(self, host, "move", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_swap_hands(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerSwapHandsEvent<'_>,
    ) -> Flow {
        let cancelled = fire_event(self, host, "item swap", Some(event.player), Vec::new());
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_bed_enter(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerBedEnterEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        let cancelled = fire_event(self, host, "bed enter", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_bed_leave(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerBedLeaveEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        fire_event(self, host, "bed leave", Some(event.player), values);
    }

    fn item_toss(&self, host: &mut dyn ServerHost, event: &mut mod_api::ItemTossEvent) -> Flow {
        let cancelled = fire_event(self, host, "drop", Some(event.player), Vec::new());
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_riptide(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerRiptideEvent<'_>) {
        fire_event(self, host, "riptide", Some(event.player), Vec::new());
    }

    fn player_arm_swing(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerArmSwingEvent<'_>,
    ) {
        fire_event(self, host, "arm swing", Some(event.player), Vec::new());
    }

    fn weather_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::WeatherChangeEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let weather = if event.thundering {
            "thunder"
        } else if event.raining {
            "rain"
        } else {
            "clear"
        };
        let values = vec![
            ("world", Value::World(dimension_world(&level, event.dimension))),
            ("weather", Value::Text(weather.to_owned())),
        ];
        let cancelled = fire_event(self, host, "weather change", None, values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn mob_spawned(&self, host: &mut dyn ServerHost, event: &mut mod_api::MobSpawnedEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("location", point_location(&level, event.dimension, event.pos)),
        ];
        fire_event(self, host, "spawn", None, values);
    }

    fn lightning_strike(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::LightningStrikeEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = event.player.and_then(|u| player_dim_of(host, u)) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        let cancelled = fire_event(self, host, "lightning strike", event.player, values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn bucket_empty(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::BucketEmptyEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        let cancelled = fire_event(self, host, "bucket empty", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn entity_tame(&self, host: &mut dyn ServerHost, event: &mut mod_api::EntityTameEvent<'_>) {
        let values = vec![("entity", Value::EntityType(event.kind.to_owned()))];
        fire_event(self, host, "tame", event.owner, values);
    }

    fn entity_breed(&self, host: &mut dyn ServerHost, _event: &mut mod_api::EntityBreedEvent) {
        fire_event(self, host, "breed", None, Vec::new());
    }

    fn entity_toggle_glide(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityToggleGlideEvent<'_>,
    ) {
        let values = vec![("state", Value::Bool(event.gliding))];
        fire_event(self, host, "glide toggle", Some(event.player), values);
    }

    fn player_fish(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerFishEvent<'_>) -> Flow {
        let cancelled = fire_event(self, host, "fish", Some(event.player), Vec::new());
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_item_mend(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerItemMendEvent<'_>,
    ) {
        let values = vec![
            ("item", item_value(event.item, 1)),
            ("number", Value::Int(i64::from(event.repair))),
        ];
        fire_event(self, host, "item mend", Some(event.player), values);
    }

    fn entity_mount(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityMountEvent<'_>,
    ) -> Flow {
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("vehicle", Value::EntityType(event.vehicle_kind.to_owned())),
        ];
        let name = if event.mounting { "mount" } else { "dismount" };
        let cancelled = fire_event(self, host, name, None, values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_edit_book(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerEditBookEvent<'_>,
    ) -> Flow {
        let values = vec![("number", Value::Int(i64::from(event.pages)))];
        let name = if event.signing { "book sign" } else { "book edit" };
        let cancelled = fire_event(self, host, name, Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_pickup_arrow(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerPickupArrowEvent<'_>,
    ) {
        let values = vec![("item", item_value(event.item, 1))];
        fire_event(self, host, "pick up arrow", Some(event.player), values);
    }

    fn player_trade(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerTradeEvent<'_>) {
        fire_event(self, host, "trade", Some(event.player), Vec::new());
    }

    fn entity_shoot_bow(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityShootBowEvent<'_>,
    ) {
        fire_event(self, host, "shoot", Some(event.player), Vec::new());
    }

    fn item_spawn(&self, host: &mut dyn ServerHost, event: &mut mod_api::ItemSpawnEvent<'_>) {
        // The event has no dimension; the item's position is read in the main world.
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("item", item_value(event.item, i64::from(event.count))),
            ("number", Value::Int(i64::from(event.count))),
            (
                "location",
                point_location(&level, "minecraft:overworld", event.pos),
            ),
        ];
        fire_event(self, host, "item spawn", None, values);
    }

    fn explosion_prime(
        &self,
        host: &mut dyn ServerHost,
        _event: &mut mod_api::ExplosionPrimeEvent,
    ) {
        fire_event(self, host, "explosion prime", None, Vec::new());
    }

    fn vehicle_create(&self, host: &mut dyn ServerHost, event: &mut mod_api::VehicleCreateEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("location", point_location(&level, "minecraft:overworld", event.pos)),
        ];
        fire_event(self, host, "vehicle create", None, values);
    }

    fn vehicle_destroy(&self, host: &mut dyn ServerHost, event: &mut mod_api::VehicleDestroyEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("location", point_location(&level, "minecraft:overworld", event.pos)),
        ];
        fire_event(self, host, "vehicle destroy", None, values);
    }

    fn vehicle_damage(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::VehicleDamageEvent<'_>,
    ) -> Flow {
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("number", Value::Num(f64::from(event.damage))),
        ];
        let cancelled = fire_event(self, host, "vehicle damage", None, values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn vehicle_moved(&self, host: &mut dyn ServerHost, event: &mut mod_api::VehicleMovedEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("entity", Value::EntityType(event.kind.to_owned())),
            ("from", point_location(&level, "minecraft:overworld", event.from)),
            ("to", point_location(&level, "minecraft:overworld", event.to)),
        ];
        fire_event(self, host, "vehicle move", event.player, values);
    }

    fn portal_create(&self, host: &mut dyn ServerHost, event: &mut mod_api::PortalCreateEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![("location", block_location(&level, event.dimension, event.pos))];
        fire_event(self, host, "portal create", None, values);
    }

    fn entity_portal(&self, host: &mut dyn ServerHost, event: &mut mod_api::EntityPortalEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![("location", block_location(&level, event.dimension, event.pos))];
        fire_event(self, host, "portal enter", None, values);
    }

    fn chunk_load(&self, host: &mut dyn ServerHost, event: &mut mod_api::ChunkLoadEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let origin = [f64::from(event.chunk[0]) * 16.0, 0.0, f64::from(event.chunk[1]) * 16.0];
        let values = vec![("location", point_location(&level, event.dimension, origin))];
        fire_event(self, host, "chunk load", None, values);
    }

    fn chunk_unload(&self, host: &mut dyn ServerHost, event: &mut mod_api::ChunkUnloadEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let origin = [f64::from(event.chunk[0]) * 16.0, 0.0, f64::from(event.chunk[1]) * 16.0];
        let values = vec![("location", point_location(&level, event.dimension, origin))];
        fire_event(self, host, "chunk unload", None, values);
    }

    fn explosion_detonate(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ExplosionDetonateEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![("location", point_location(&level, event.dimension, event.center))];
        fire_event(self, host, "explode", None, values);
    }

    fn item_despawn(&self, host: &mut dyn ServerHost, event: &mut mod_api::ItemDespawnEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("item", item_value(event.item, i64::from(event.count))),
            ("number", Value::Int(i64::from(event.count))),
            ("location", point_location(&level, "minecraft:overworld", event.pos)),
        ];
        fire_event(self, host, "item despawn", None, values);
    }

    fn anvil_damaged(&self, host: &mut dyn ServerHost, event: &mut mod_api::AnvilDamagedEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![("location", block_location(&level, event.dimension, event.pos))];
        fire_event(self, host, "anvil damage", None, values);
    }

    fn projectile_impact(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ProjectileImpactEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let mut values = vec![("location", point_location(&level, event.dimension, event.at))];
        if event.entity.is_some() {
            values.push(("entity", Value::EntityType(event.entity_kind.to_owned())));
        }
        let cancelled = fire_event(self, host, "projectile hit", None, values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_pick_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerPickItemEvent<'_>,
    ) -> Flow {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        let cancelled = fire_event(self, host, "pick item", Some(event.player), values);
        if cancelled {
            event.cancelled = true;
        }
        verdict(cancelled)
    }

    fn player_stop_using_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerStopUsingItemEvent<'_>,
    ) {
        fire_event(self, host, "stop using item", Some(event.player), Vec::new());
    }

    fn player_ready_arrow(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerReadyArrowEvent<'_>,
    ) {
        let values = vec![("item", item_value(event.arrow, 1))];
        fire_event(self, host, "ready arrow", Some(event.player), values);
    }

    fn player_deep_sleep(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerDeepSleepEvent<'_>,
    ) {
        fire_event(self, host, "deep sleep", Some(event.player), Vec::new());
    }

    fn player_egg_throw(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerEggThrowEvent<'_>,
    ) {
        fire_event(self, host, "egg throw", Some(event.player), Vec::new());
    }

    fn player_elytra_boost(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerElytraBoostEvent<'_>,
    ) {
        fire_event(self, host, "elytra boost", Some(event.player), Vec::new());
    }

    fn player_bucket_entity(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerBucketEntityEvent<'_>,
    ) {
        let values = vec![("entity", Value::EntityType(event.entity.to_owned()))];
        fire_event(self, host, "bucket entity", Some(event.player), values);
    }

    fn player_change_beacon_effect(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerChangeBeaconEffectEvent<'_>,
    ) {
        fire_event(self, host, "beacon effect change", Some(event.player), Vec::new());
    }

    fn player_input(&self, host: &mut dyn ServerHost, event: &mut mod_api::PlayerInputEvent<'_>) {
        let values = vec![("number", Value::Int(i64::from(event.flags)))];
        fire_event(self, host, "player input", Some(event.player), values);
    }

    fn player_exp_cooldown_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerExpCooldownChangeEvent<'_>,
    ) {
        let values = vec![("number", Value::Int(i64::from(event.ticks)))];
        fire_event(self, host, "experience cooldown change", Some(event.player), values);
    }

    fn player_stonecutter_recipe_select(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerStonecutterRecipeSelectEvent<'_>,
    ) {
        let values = vec![("number", Value::Int(i64::from(event.recipe)))];
        fire_event(self, host, "stonecutter recipe select", Some(event.player), values);
    }

    fn item_crafted(&self, host: &mut dyn ServerHost, event: &mut mod_api::ItemCraftedEvent<'_>) {
        fire_event(self, host, "craft", Some(event.player), Vec::new());
    }

    fn prepare_item_enchant(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PrepareItemEnchantEvent<'_>,
    ) {
        let level = lock(&self.st).level_name.clone();
        let mut values = Vec::new();
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        fire_event(self, host, "prepare enchant", Some(event.player), values);
    }

    fn enchant_item(&self, host: &mut dyn ServerHost, event: &mut mod_api::EnchantItemEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let mut values = vec![("number", Value::Int(i64::from(event.levels)))];
        if let Some(dimension) = player_dim_of(host, event.player) {
            values.push(("location", block_location(&level, &dimension, event.pos)));
        }
        fire_event(self, host, "enchant", Some(event.player), values);
    }

    fn furnace_smelt(&self, host: &mut dyn ServerHost, event: &mut mod_api::FurnaceSmeltEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("location", block_location(&level, "minecraft:overworld", event.pos)),
            ("item", item_value(event.result, 1)),
        ];
        fire_event(self, host, "furnace smelt", None, values);
    }

    fn brewing_complete(&self, host: &mut dyn ServerHost, event: &mut mod_api::BrewingCompleteEvent) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![("location", block_location(&level, "minecraft:overworld", event.pos))];
        fire_event(self, host, "brewing complete", None, values);
    }

    fn brewing_start(&self, host: &mut dyn ServerHost, event: &mut mod_api::BrewingStartEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("location", block_location(&level, "minecraft:overworld", event.pos)),
            ("item", item_value(event.ingredient, 1)),
        ];
        fire_event(self, host, "brewing start", None, values);
    }

    fn brewing_fuel(&self, host: &mut dyn ServerHost, event: &mut mod_api::BrewingFuelEvent<'_>) {
        let level = lock(&self.st).level_name.clone();
        let values = vec![
            ("location", block_location(&level, "minecraft:overworld", event.pos)),
            ("item", item_value(event.fuel, 1)),
        ];
        fire_event(self, host, "brewing fuel", None, values);
    }
}
