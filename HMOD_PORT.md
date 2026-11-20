# hmod port: skript
Parity: ~29.6% now / ~29.6% with our PRs (about 346 of 1168 audited classes, on the audit scale of /home/leo/horizon-wt/audit/skript.md; 332 before round 8, +14 classes: expressions +12, conditions +1, types +1; round 7 was 327 -> 332)

Round 8 (types: vector, inventory read, blind; checklist round 100)
- Added the `vector` type: `Value::Vector { x, y, z }` (`value.rs`); text form `x: 1, y: 2, z: 3` as Skript's VectorParser; "vector" in the class list (no text parse, as Skript's `canParse` is false); equality (`syntax/support.rs`); `is a vector` in the general type condition (`conditions/more2.rs`); arithmetic in `expr.rs` (`vector_arith`): `+ -` and `* /` between two vectors per component, `v * n`, `n * v`, `v / n`, `n / v`. Division by zero is an error (Bukkit gives infinity; a choice).
- Added `syntax/expressions/vectors.rs`: ExprVectorFromXYZ (`vector x, y, z`), ExprXYZComponent (`x|y|z component of`), ExprVectorLength (`vector|standard|normal length of`), ExprVectorSquaredLength, ExprVectorNormalize (zero stays zero), ExprVectorDotProduct, ExprVectorCrossProduct, ExprVectorAngleBetween (degrees, as Skript), ExprVectorProjection, ExprVectorOfLocation (the position, as `Location.toVector`), ExprVectorBetweenLocations, ExprVectorFromYawAndPitch (Skript's own yaw and pitch formula), and the CondIsVectorNormalized condition (`is normalized`, tolerance 1e-6 read from Bukkit's `isNormalized` bytecode). Unit tests with literal values in the same file.
- Core fix: `player_state` in `src/lib.rs` never read the inventory (`inventory: Vec::new()`), so `inventory of player` was always empty and `player has N of X` always failed. It now fills the stacks with items from `PlayerRef::inventory_slots` (slot order; armour, off hand and hotbar included; a stack's custom name is not read). The call shape is the one essentials uses (`cmd_items.rs`); not compiled yet.
- Checklist round 100:
  - [x] type vector; vector arithmetic; `is a vector`
  - [x] expressions: ExprVectorFromXYZ, ExprXYZComponent (the `component` form only), ExprVectorLength, ExprVectorSquaredLength, ExprVectorNormalize, ExprVectorDotProduct, ExprVectorCrossProduct, ExprVectorAngleBetween, ExprVectorProjection, ExprVectorOfLocation, ExprVectorBetweenLocations, ExprVectorFromYawAndPitch
  - [x] conditions: CondIsVectorNormalized
  - [x] inventory of player and player has (the inventory read)
  - [ ] ExprVectorRandom (Skript samples uniformly in the cube with rejection; not ported), ExprVectorFromDirection (needs location directions), ExprVectorSpherical and ExprVectorCylindrical
  - [ ] bare `x of %vector%`, `%vector%'s x` and the setters (`set x of`, `add ... to x of`): the location coordinate owns `x of` and changing a vector needs effects
  - [ ] slot type and `slot n of player`, `first empty slot`, `inventory slot`: the engine reads the slot index (`inventory_slots`), PlayerState keeps only the stacks; add a slot index field to PlayerState first
  - [ ] potion effect and potion effect type values, `potion effects of`, `has potion`, apply and remove: the engine reads exist (`living_effects`, `living_add_effect`, `living_remove_all_effects`); the port side is not written
  - [ ] entity type values beyond the existing ones: ExprNearestEntity, ExprEntityType (`entity type of`), is alive: `entities_in_world` exists in the host; the distance code is not written
  - [ ] item stack lore, enchantments and custom name: no engine read (GAPS_UPSTREAM.md, two new lines under `skript`)
  - [ ] the vector-valued variables: a vector is not written to variables.csv (vars.rs `encode` has no vector, the same as locations)
- Unsure: all of round 8 is written blind, not built or run. Choices: division by zero is an error for vectors; `normalized` of a zero vector is zero (Skript skips it, the same result); the vector of a location is the position (Skript's `toVector`), not a direction; the inventory read keeps only non-empty stacks and relies on `inventory_slots` returning every slot, which the glue has not checked against the engine yet.

Round 7 (expressions, blind; checklist round 100)
- Added: `syntax/expressions/more3.rs`: `<n>% of %numbers%` (literal percent), `%number% percent of %numbers%`, `inverse|opposite of %booleans%`, `midpoint of %location% and %location%` (same world only, yaw and pitch zero), `shuffled %objects%` (Fisher-Yates on the host's random number), `%objects% except|excluding %objects%` (the `equal` rules of the comparison code). Unit tests with literal values in the same file.
- Checklist round 100 (ticks = done this round or earlier; `[ ]` = still to do; the audit count is the original syntax class name):
  - [x] expressions: ExprPercent, ExprInverse, ExprMidpoint, ExprShuffledList, ExprExcept (round 7); ExprRound, ExprNumbers, ExprSortedList, ExprReversedList, ExprRepeat, ExprAlphabetList, ExprCodepoint, ExprDateAgoLater, ExprTimeSince, ExprRandomUUID (earlier rounds)
  - [x] structures: `variables:`, `aliases:`, `using`, `local function`, `on <event> with priority` (round 2); `/skript` help, reload and enable/disable forms, Tab completion (round 2)
  - [x] command header, console execution, `executable by`, `cooldown`, `permission` (round 2)
  - [ ] `/skript update|restore` (no updater in the host)
  - [ ] `on <event> of <type>` filters and `with priority` on the new events (core work, parse.rs)
  - [ ] `suppress warnings` and `suppress type hints` sections (no-op in the port; section parsing)
  - [ ] `ExprTimespanDetails` (`%timespan%'s days` ...): needs the TimePeriod constants checked against the original before writing
  - [ ] `ExprFormatDate`, `ExprHash` (MD5/SHA family: no digest crate in Cargo.toml)
  - [ ] `ExprTernary` (`%objects% if <condition> otherwise %objects%`): needs a condition sub-parse, not a pattern
  - [ ] the other pure syntax classes of the audit (most effects and conditions are on engine APIs, see the list below)
  - [ ] API-blocked (one line each in GAPS_UPSTREAM.md under `skript`, already present): ping, fall distance, spectator target, lore and durability, light level, solid blocks, living entities as values, living states, entity flags, detonation, mob AI, potion flags, block changes, event cancels, item meta, visibility, world and server calls, transfer, the level-host event hooks, ban list, whitelist, server version, plugin list, damage cause, entity validity, chunk state
- Unsure: all of round 7 is written blind, not built or run. The percent literal form relies on the pattern escape `\%` as the chance condition does.


Round 6 (glue reads and core-only syntax)
- Core: `<effect> if <condition>` (EffDoIf), `continue [the Nth loop]` (EffContinue), `assert ... [to fail] [with error ...]` (EffAssert), `log ... to file ... [with severity ...]` (EffLog, written to plugins/Skript/logs), `enable|reload|disable|unload script ...` (EffScriptFile, applied by the glue after the running trigger), `sort ... by <expr of input>` (EffSort mapping form), `transform ... using ...` (EffTransform), `input` and `input index` (ExprInput).
- Host: `Vitals` read (abilities, experience, saturation, absorption, fire, air, freeze, velocity, on ground, invisible, sleeping, gliding, blocking, glowing, silent, invulnerable, vehicle, passengers, respawn), `WorldInfo` read (day time, game time, weather, difficulty), `biome_at`, `line_of_sight`; Actions `set_flag`, `set_fire_ticks`, `set_air`, `set_velocity`, `set_abilities`, `set_absorption`, `set_saturation`, `respawn`.
- Expressions: fly speed, walk speed (changeable, Bukkit scale), total experience, fire time (changeable), remaining air (changeable), saturation (changeable), vehicle, passengers, bed location, biome, world time, weather, difficulty.
- Conditions: burning, on ground, invisible, visible, frozen, blocking, sleeping, riding, gliding, glowing, silent, invulnerable, line of sight.
- Effects: ignite, extinguish, allow/disallow flight, make fly, make invulnerable/vulnerable, silence/unsilence, force respawn.
- Still needs an engine API (see GAPS_UPSTREAM.md): ping, fall distance, light level, spectator target, ban/whitelist/version/plugin conditions, entity validity, damage cause, lore/durability, highest block.
- Glue or core work left although the engine API exists: velocity (needs a vector class), boss bar flags (needs a boss bar value), explosions with fire, sleep/swing actions, potion flags, leash and mob actions, `run %executable%`.

Round 1
- Fixed: src/lib.rs `has_permission` (script `permission:` nodes and `player has permission`): `PlayerRef::has_permission` replaced by a typed `permission_bool` query on a `PermissionNode` built from the node name; any error is a denial.
- Fixed: src/lib.rs `player_commands_sent` and `player_command`: `has_permission(SKRIPT_NODE)` replaced by typed `permission_bool` on `skript_node()`.
- Fixed: src/lib.rs `register_commands`: `Command::op(...).with_node(SKRIPT_NODE)` now passes `&skript_node()` (typed node).
- Fixed: src/lib.rs new `permission_gather_nodes` hook declares `skript.admin` with default level 2 (plugin.yml default op).
- Fixed: src/lib.rs `entities_in_world`: `EntityTypeKey` mapped to its id string with `kind.id(host)` before the mannequin comparison.
- Fixed: src/lib.rs `set_equipment` (living entity path): `ItemStack::set_display` is not on main; call removed, the name is computed but not set.
- Permission nodes:
  - `skript.admin` -> Level 2, declared in `permission_gather_nodes` (`skript_node()`), used by `/skript`, `/sk`, Tab filter and `player_command`.
  - Script-defined nodes (`permission:` lines and `has permission` with any text): not declared (they come from scripts at run time), so they are refused as unregistered.
- Gaps:
  - (closed in round 2) HMOD-GAP(script-permission-nodes): the nodes the script files name are now declared at gathering.
  - HMOD-GAP(item-display): `ItemStack::set_display` (custom name) does not exist on main. A living entity's equipment from `set_equipment` gets the bare item without its name.
- Unsure:
  - Undeclared script nodes are denied (spec: any error is a denial) rather than falling back to operator level 2 as before; this is a choice, not a verified equivalent.

Round 2 (parity audit)
- Fixed: script `permission:` and `has permission "..."` nodes are scanned from the scripts folder and declared (level 2) in `permission_gather_nodes`; an undeclared (dynamic) node answers for operators only, as Bukkit does; `skript.effectcommands`, `skript.reloadnotify` and `skript.see_runtime_errors` are declared too.
- Fixed: script commands run from the console and parse their header (`<type>`, `[<type=default>]`, `<name: type>`, words), with `executable by`, `cooldown`, `aliases`; a wrong line prints the usage; `arg-N`, `arg-<type>`, `<type>-arg`, `last arg` and named arguments work.
- Fixed: a trigger or function with an error is left out and the rest of the file loads; `variables:`, `aliases:`, `using`, `local function` and `with priority` are accepted.
- Fixed: a reload, enable or disable that changes the script commands calls `refresh_commands`; script aliases are registered.
- Fixed: `on chat` runs at once; `cancel event` and `set message to` act on the message.
- Fixed: `/skript` help, `reload all|scripts|config|aliases|last|<script or folder>`, `enable|disable all|<folder>`, Tab completion, reload notices for `skript.reloadnotify`.
- Gaps: HMOD-GAP(damage-cancel): `living_incoming_damage` gets a level host, which cannot run the scripts, so `cancel event` in `on damage` has no effect. First join stays approximated (no join-history call). `every day at <time>` and the missing syntax classes remain.
- Unsure: a node that is not declared is answered with permission level >= 2 (operator), a choice that follows Bukkit's default, not a verified equivalent. Command usage lines print without a prefix, as Skript's own usage did.

Round 3 (effects and conditions)
- Added effects (about 22 -> about 38 of 153 classes): feed, look at, make (in)visible, ride/dismount, equip/unequip, clear potion effects, stop sound, ip-ban (`effects/player_state.rs`); strike lightning (+ effect), explosion (+ fake), drop, break (naturally) (`effects/place.rs`); stop server, save world, enforce whitelist, bare-word weather, send message, wait for (`effects/server.rs`); increase, decrease, reset, toggle, replace, regex replace, copy (`effects/variables.rs`).
- Added conditions (about 20 -> about 30 of 170 classes): is wearing, within N blocks of, within A and B, in the past/future, has passed, was more/less than N ago, alphanumeric, matches (full and partial), event is cancelled, edible, in water/lava/liquid, and the `doesn't have permission` form (`conditions/more.rs`, `conditions/player.rs`).
- Done by vanilla commands run as the console (a command cannot name a world, so a place in `*_nether` or `*_the_end` runs in that dimension and any other world in the overworld).
- Gaps (no vanilla command or no host data): ignite/extinguish, toggle flight, sprinting, swing hand, respawn, knockback, invulnerable, silent, open inventory, PvP toggle, restart server, unenforce whitelist, world load; conditions that need entity flags (gliding, sleeping, riding, on ground, burning, invisible), bans/whitelist lists, slime chunk, item properties other than edible. An explosion is a creeper that blows up at once (no `with fire`, no `safe`), the lightning effect is sparks and thunder, a dropped item has no random velocity.
- Unsure: all new syntax was written without building; patterns with a leading optional space were written with the space inside the optional part, as the pattern notes ask.

Round 3 (expressions, events and types)
- Added expressions (42 new registrations, about 110 new patterns): `expressions/lists.rs` (sorted, alphabetically sorted, reversed, repeated, numbers/integers/decimals between, first/last/nth/range elements, upper/lower/proper/camel/pascal/snake/kebab case, character counts, code points, random uuid and character, unix date and timestamp, ago/later, time since/until, colour channels, `<n> of <item>`, amount of items in an inventory, `parsed as`), `expressions/world.rs` (all `<entity type>` in a world or within a radius, `<n> blocks above/below/north of ...`, `location at x, y, z`, player count) and `expressions/event.rs` (the damage, damage cause, clicked block, weather, command, full command, command sender, location, world, `me`, `event-world|entity|number|vehicle|gamemode|item type`).
- Added events (about 16 -> 54, `events/misc.rs`, hooks in `src/lib.rs`): teleport, world change, game mode change, flight/sneak/sprint toggle, jump, inventory open/close, item break, hunger change, level change, experience change, kick, move (fires when the player enters another block), item swap, bed enter/leave, drop, riptide, arm swing, weather change, spawn, item spawn, lightning strike, bucket empty, tame, breed, glide toggle, fish, item mend, mount/dismount, book edit/sign, pick up arrow, trade, shoot, explosion prime.
- Added types (12 -> 61 names, 46 classes, `classes.rs`): entity, command sender, uuid, color (dye names and hex), 30 enum classes (game mode, difficulty, weather type, damage cause, teleport cause, ...), and the names byte, short, long, float, double, string, item stack, material, block, block data, offline player, living entity, projectile, vehicle, inventory holder that stand for a class the port has.
- Gaps: no vector, quaternion, inventory, slot, block (an object), potion effect, firework effect, world border, boss bar or display types, because the value model has no variant for them; an offline player is read as an online one; the enum classes with no fixed word list take any word. Events not fired: item pickup (the hook fires every tick the item is touched), inventory click (the hook is for mod menus only), block ignite/fade/form/spread and the other level-host hooks (they cannot run scripts), and `drop` carries no item (the item handle has no readable stack). `on <event> of <type>` filters and `with priority` for the new events are not parsed.


Round 4 (conditions, blind)
- Added: `conditions/more2.rs`, the general `<objects> is [not] a <type>` for text, number, integer, boolean, timespan, date, world, location, item type, entity type and entity (Skript's `CondIsOfType` general form; `is a player` stays in the entity group). Registered at priority -1 in `conditions/mod.rs`. Unit test with literal values.
- Gaps: the conditions that need an engine call the host does not offer (ban list, whitelist, server version, plugin list, join history, entity flags such as burning/on ground/invisible, damage cause, line of sight, entity validity, chunk state and slime chunks) are listed in GAPS_UPSTREAM.md. Not built or run (blind round).

Round 4 (expressions, blind)
- Added expressions (`expressions/more2.rs`, 2 new classes plus 4 forms of existing ones): `round`, `round down|floored`, `round up|ceiled` on numbers and lists (half up, as Java `Math.round`, integer result); `%numbers% in degrees|radians` (a radian value is turned into degrees, as `ExprAngle`); `first|last [N] character(s) of %text%`; `name of %world%`.
- Gaps (need an engine API, listed in `/home/leo/horizon-wt/audit/GAPS_UPSTREAM.md` under skript): ping, fly and walk speed, total experience, fall distance, fire ticks, air, saturation, absorption, velocity, respawn and bed location, vehicle, passenger, spectator target, item lore and durability, biome and light level, highest or lowest solid block, world time, weather and difficulty, living entities as values.
- Unsure: the blind code has no build; the highest solid block is not done because `block_type_at` does not tell solid blocks.

Round 5 (events and types, blind)
- Added events (56 -> 86 registrations, 30 new, in `events/world.rs`, `events/player_more.rs`, `events/craft.rs`; hooks appended to the end of the impl in `src/lib.rs`): vehicle create, destroy, damage (cancellable) and move; portal create and portal enter; chunk load and unload; explode; item despawn; anvil damage; projectile hit (cancellable); pick item (cancellable); stop using item; ready arrow; deep sleep; egg throw; elytra boost; bucket entity; beacon effect change; player input; experience cooldown change; stonecutter recipe select; craft; prepare enchant; enchant; furnace smelt; brewing complete, start and fuel. Each has a load test in its file.
- Added types (2 enum classes, `classes_more.rs`, read by `classes.rs` through `enum_classes()`): damage source and game rule value, both as text with any word.
- Gaps: the events of the level-host hooks (piston extend and retract, block spread, fade, form, ignite and burn, leaves decay, sponge absorb, bell ring, entity heal and jump, enderman enrage) do not fire, because those hooks get a level host and cannot run scripts; listed in GAPS_UPSTREAM.md. `bucket fill` is not added (the engine event has no player and no readable stack). `prepare craft` is not added (the hook returns an item stack this code does not build). `on <event> of <type>` filters are not parsed. Types with a value variant of their own (vector, inventory, slot, potion effect, and others) still need a new value model.
- Unsure: all of this was written blind, not built or run. The events with no dimension in the engine (vehicle create and destroy, item despawn, furnace smelt, brewing, chunk events) read the overworld, as the item spawn event does. The count test in `events/mod.rs` was updated to 86.

Round 6 (effects, blind)
- Added effect `sort` (`syntax/effects/sorting.rs`): `sort %objects% [in descending|ascending order]` on a list variable, natural order (numbers by value, text in lower case); entries of mixed kinds stay as they are; only direct entries are renumbered. The `by <expression>` and `based on` forms are not parsed (they need the input expression). Effects now 23 of 153 by the audit count (22 before).
- Gaps (need an engine API): 88 effects, listed in 15 lines under `skript` in `/home/leo/horizon-wt/audit/GAPS_UPSTREAM.md` (flight, sprint/sleep/swing states, respawn, entity flags, fire, explosions, mob AI, potion flags, block changes, event cancels, item meta, boss bar flags, visibility, world and server calls, transfer). Their upper bound is 111 of 153 effects.
- Not done, core work (no engine API): `<effect> if <condition>` (`EffDoIf`), `assert`, `continue`, `debug`, `log ... to file`, `run`, script file enable/reload/unload, `transform ... using`, suppress-warnings sections, and the `sort ... by` form.
- Unsure: blind, nothing built or run. Unit tests in `sorting.rs` use literal values (natural order, descending, mixed kinds).
