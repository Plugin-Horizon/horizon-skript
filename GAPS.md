# What Horizon is missing for this mod

Checked against Horizon 1453e0e61 (`cargo build` in `api/` passes). Names and types are checked by the compiler;
behaviour is not, and each UNVERIFIED tag below is about behaviour unless it says otherwise. Each line: what the mod
would need, which feature it serves, and what the mod does today.

## Hooks

- [ ] A chat hook that has the host: `fn player_chat(&self, host: &mut dyn Host, event: &mut
  PlayerChatEvent)`. Needed for `on chat:` to answer or cancel the message in the same tick
  (Skript's `cancel event`). Today: `native_player_chat` has no host, so the message is queued and
  the `chat` event fires on the next tick; a script cannot cancel the message.
- [ ] A server-ready hook after the worlds are open (`world_lifecycle` open ready, API 3, if it
  fits). Needed for `on load:` to run as the server starts. Today: the scripts load on the first
  tick or first command.
- [ ] Re-sending the command tree (`Host::resend_commands()` or the like). Needed for `command /name:`
  triggers added by `/skript reload` or `/skript enable` to appear without a restart. Today: the
  commands are registered at the first `register_commands` call; new ones appear after a restart.
- [ ] Field names of the generated events, UNVERIFIED in `api/src/lib.rs`: `PlayerChatEvent::message`,
  `PlayerLoggedInEvent::name` (the worldguard glue uses `name` and `player`), `PlayerLoggedOutEvent::player`.
  Check first when this compiles.
- [ ] Damage and move with a host. `living_incoming_damage` and `player_move` take no host, so the glue
  queues `damage` and `move` for the next tick: a script cannot cancel them, `move` has no `from` and
  `to`, and `damage` has no player victim. Needed for `on damage:` and `on move:` to cancel and to carry
  their values. Check whether the engine has a hooked variant with a host (`living_incoming_damage_v2`,
  `v3`, `player_move` are in `tools/native-hook-fragments`).
- [ ] A cancellable death. `player_died` is a listen hook, so `cancel event` in `on death:` does nothing.
  Needed: `living_death` (cancellable in the catalog) for the death event. Today: the death fires after
  the fact.
- [ ] A pre-login check for `first join`. `has_played_before` may already be true when
  `player_logged_in` runs, in which case `on first join:` never fires. Needed: the `player_pre_login`
  hook (in the catalog), or a player file check. UNVERIFIED timing.
- [ ] Respawn location: the `respawn` event has no `location` (needs the respawn position the engine
  already has: `player_respawn_position` in the catalog).
- [ ] Make-say from a player's hook: `chat_as` returns false inside a player's hook (the engine says
  so), so `make ... say` from `on join:` or `on command:` does nothing. Needed: the glue queues
  `chat_as` for the next tick (the chat queue does the same for `chat`).

## Helpers

- [ ] `Host::registry_has(registry: &str, id: &str) -> bool` (`minecraft:item`, `minecraft:entity_type`).
  Needed for `item type` and `entity type` values to be checked when a script names them. Today:
  every id is accepted (`Host::is_item_type` and `is_entity_type` default to true; the glue does not
  override them).
- [ ] Persisting variables: `Game::native_config_dir()` (API 26) plus `server_stopping` (verified)
  to write `variables` on stop and read them on start. Needed for `{x}` values to survive a restart,
  as Skript's `variables.csv` does. Today: variables live in memory only.
- [ ] A console-side `Host::log` that goes to the server log instead of stderr, if the engine has one.
  Today: `eprintln!`.
- [ ] Block and held-item values of the block events: the block id (`block`) of break, place and the
  clicks, and the held item (`item`) of the clicks. Needed: the engine's block lookup at a position
  (`level_block`) and the held stack (`held`, in the catalog). Today: only `location` is filled.
- [ ] `set block` (`Actions::set_block`) is a stub. Needed: the block id as a `BlockStateId`
  (`default_state` in the catalog) and the flags of `native_set_block(dimension, pos, state, flags)`.
- [ ] `spawn` (`Actions::spawn`) is a stub. Needed: `summon` with the entity's kind and NBT (the
  `native_summon` arguments; `summon_with_uuid` in the catalog).
- [ ] Heal by points and push are stubs. `heal N` needs the current health (the glue does not read
  `player_state` yet, see the core's `Host::player_state`); full heal is `set_player_status` at 20.
  `push` needs the entity velocity helper (`entity_set_velocity` in the catalog, signature to check).
  `push towards|away from` also needs `player_state`, so it fails with "not online" until the glue has it.
- [ ] Helper names and argument types used by `api/src/lib.rs` are UNVERIFIED: `run_command`,
  `run_command_as` (its result is ignored), `action_bar`, `title` (three times as `u32`), `chat_as`,
  `teleport_player`, `dimension_of_world`, `set_player_status` (optional health, food, experience),
  `has_played_before`, `world_of_dimension`, and the event types of the new hooks.

## Player and world queries (the syntax library reads them through `Host`)

The conditions and expressions about players (STATUS.md) read `Host::player_state`, `has_permission`,
`can_see`, `block_type_at` and `target_block`. The core has defaults for all of them (no state, no
permission, no block), so nothing breaks, but the glue has to fill them for the syntax to be useful.

- [ ] `Host::player_state(uuid) -> Option<PlayerState>`: name, display name, location, health and max
  health, food, level, game mode, flying, can fly, sneaking, sprinting, swimming, operator, the item in
  the main hand (`held`) and the filled inventory slots. Needed for `is op`, `is alive`, `is flying`,
  `is sneaking`, `is sprinting`, `is swimming`, `is holding`, `has <items>`, `is in world`, `health of`,
  `food level of`, `level of`, `game mode of`, `tool of`, `location of`, `world of`, `inventory of`,
  `x of`, and the `online` condition for players that are not online. Today: `None`, so these fail
  with "is not online" and `is online` is false. UNVERIFIED: the engine's player getters (not read yet).
- [ ] `Host::has_permission(uuid, node)`: the engine's permission check (LuckPerms through the engine),
  operators included. Needed for `has permission`. Today: always false.
- [ ] `Host::can_see(viewer, target)`: visibility (vanish, world, and the engine's tracking). Needed for
  `can see`. Today: always false.
- [ ] `Host::block_type_at(location)` and `Host::target_block(uuid, range)`: the block id at a position
  (the engine's block lookup) and the block a player looks at (a ray cast over `range` blocks). Needed
  for `block at` and `target block`. Today: `None`.
- [ ] `Host::random_unit`: the default seeds from the standard library. The engine's generator could
  replace it, which would make `random` reproducible in tests. Minor.
- [ ] `Host::online_players` is written in `api/src/lib.rs` from `server_players()`, the same call
  `player_by_name` uses. UNVERIFIED: the code compiles at 1453e0e61, but its behaviour has not run.

## Values the core does not have yet

- [ ] A vector type: Skript's `vector(x, y, z)`, `vector between`, `direction` and the velocity of
  `push` need a `Value::Vector { x, y, z }` (class `vector`) in `value.rs`, `classes.rs` and the
  functions. Today: missing; `vector()` is not registered.
- [ ] Time: `[the] time`, `world time`, `time played` and `time since`. Needs the world's time of day
  and the player's played time from the engine (`Host`). Today: missing.

## Core limits (not engine gaps, for the record)

- A function cannot wait (`wait` inside a function is an error). Skript allows it. Needed for a few
  scripts that wait inside helper functions.
- Syntax: the comparisons, the common expressions and functions are in (STATUS.md). Not written:
  vector, time, the entity expressions, inventory slots, the sections `for` and `filter`. Effects: the
  common ones are in (STATUS.md); entities are not (`kill`, `damage`, `heal`, `push` take players
  only, `spawn` takes an entity type).
- An article in an item type is not stripped: `holding a stick` reads the id `a_stick`. Write
  `holding stick`. Skript drops `a` and `an` there; the fix belongs in `classes::normalize_id`.
- A bare player name is only read where a slot names a player (`%player%`, `%players%`,
  `%player/location%`). `if Alice is Bob` with two bare names does not parse: write `attacker is victim`
  (event values), or the player's own expressions.
- A plural slot splits its text first (`64 stone and 1 flint` is two items), so an item name cannot
  contain ` and `, ` or ` or `, `.
- Event filters (`on break of stone`, `on death of zombie`) are not parsed: the event patterns take
  no filter. Region events (`on region enter`) need a region API (the worldguard mod) and are not
  written.
- Potions: `apply speed 2` is amplifier 1 (level 2), as Skript reads the number. Without a time
  the effect lasts 1 000 000 seconds (vanilla's largest duration), where Skript keeps it infinite.
  `ban` has no duration (vanilla has none). The vanilla limit of 1 000 000 seconds is UNVERIFIED.
- Commands: the effects that run vanilla commands (kick, ban, op, gamemode, give, clear, damage,
  effect, playsound) rely on the engine running them as the console; none checked yet.
- Pattern engine (`pattern.rs`, not fixed here): two optional parts with a space between them do not
  match when the first is given and the second is left out: `send title %text% [to %players%] [for
  %timespan%]` fails on `send title "Hi" to Alice`. The space after the first part is taken into it and
  the second part's absent form then needs it. The effects work around it by putting the space inside
  each part (`[ for %timespan%]`). Found 2026-11-09; the word-spacing change of the same date does not
  cover it. A fix belongs in `Parser::parse_alts`.
- Locations have no literal in the core: `teleport Alice to world 1 64 2` does not parse (the bare
  parser has no `location`). The tests pass a location in a variable (`{loc}`). Needed: a `location`
  expression or literal (Skript's `location(x, y, z, world "w")`), in the expressions part.
- Reloading reads the files on the main thread (small files, a few milliseconds).
- `%~type%` is parsed but treated like `%type%`.
