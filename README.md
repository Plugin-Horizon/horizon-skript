# horizon-skript

> **Unofficial Rust port of Skript for the Horizon server (https://github.com/arismoko/horizon). Not affiliated with or endorsed by the Skript authors.**

The Skript scripting language for [Horizon](https://github.com/arismoko/horizon) servers, as a
native trait mod: `.sk` files in a scripts folder, Skript's indentation and sections, its pattern
syntax for statements and expressions, variables and lists, functions, `wait`, events, commands and
the `/skript` command, with the same error messages and line numbers.

**Derived from Skript (https://github.com/SkriptLang/Skript, GPL-3.0).** The language follows
Skript's source: the pattern syntax (`[optional]`, `(a|b)`, `%type%`, `<regex>`, parse marks),
the structures (`on`, `command`, `function`, `options`, `if`/`else`, `loop`, `while`) and the
names of its syntax. The licence of this mod is therefore the GNU General Public License version 3
or later (see LICENSE); the copyright holders are in NOTICE.

Upstream: Skript (https://github.com/SkriptLang/Skript), commit `19172ca06e798dea53d97b647167e6ebe7f5fe94`
("Prepare For Release (2.17.1)"), GPL-3.0. Horizon pin: `1453e0e614af4b2d04dd0d228a94162b5d459910`.

> **Built for and working only with [arismoko/horizon](https://github.com/arismoko/horizon)** (the Horizon fork that carries the mod API). [horizon-rust/horizon](https://github.com/horizon-rust/horizon) is experimental for mods and not supported: the earlier version for its native hooks / horizon-mixins is frozen on the `horizon-plain` branch (work stopped on 2026-11-12).

## Which Horizon

This is a native trait mod: `horizon-mod.toml`, a `Cargo.toml` that depends on `horizon-mod-api`, `src/lib.rs` with the hooks, and the plugin logic in the `core/` crate (plain Rust, no engine needed for its tests).

- Pin: `[native] horizon = "54090ee36c4e2c7b43b66db864220c577b94b7e0"` in `horizon-mod.toml` (game `26.3`). That is a development build of arismoko/horizon `main` plus mod API additions that are not all merged upstream yet, so a stock `main` may lack some host methods this mod calls until they land.
- It compiled and loaded on that build and was smoke-tested there; it has not seen a long run with real players. Treat it as experimental.
- Feature descriptions below were written for the earlier horizon-rust version; where they name engine calls (`native_*`) or old file paths, read them as descriptions only. The Known gaps list is current.

## What works

- Scripts: every `.sk` file under the scripts folder (`plugins/Skript/scripts` by default, set in
  `config/skript-common.toml`), subfolders included. A file whose name starts with `-` is loaded
  disabled. Parse errors name the file and the line and stop that file from loading.
- Structure: triggers (`on join:`, `on quit:`, `on chat:`, `on load:`, `on unload:`, `command /name <args>:`,
  `every 5 seconds:`), functions (`function name(p: type) :: type:`), `options:` with `{@name}`,
  `if` / `else if` / `else`, `loop`, `while`, `return`, `stop`, `exit` (`exit loop`, `exit 2 loops`).
- Statements: `set`, `add`, `remove`, `delete`, `clear`, `wait` (timespans, resumed on the right
  tick), `send ... to <player>`, `broadcast`, `return`, `stop`, `exit`.
- Values: text with `%expression%` interpolation, numbers with `+ - * / ^` and parentheses,
  timespans (`90 seconds`, `1 minute, 30 seconds`), `true`/`false`, lists (`1, 2 and 3`, `{x::*}`).
  Types: text, number, integer, boolean, timespan, date, player, location, world, item type,
  entity type.
- Variables: `{name}`, `{name::sub}`, `{_local}`, list variables with indices, `loop-value` and
  `loop-index` inside loops, `arg-1` in commands, `player` and `message` in events.
- Commands: `command /name <text> [<number=1>] word:` with typed, optional, defaulted and named
  (`<who: player>` is `{_who}`) arguments, `arg-1`, `arg-text`, `number-arg 2`, `last arg`, the last text
  argument taking the rest of the line, a usage line for a wrong line, `aliases`, `executable by`
  (players, console, operators), `permission` and `permission message`, `cooldown` with its message and
  bypass. Scripts commands run for players and from the console.
- Structures: `options`, `variables` (defaults for unset variables), `aliases` (`name = item`), `function` and
  `local function`, `on <event> with priority <p>`, `using <experiment>` (accepted, no effect), `every`.
  A trigger or function with a problem is reported and left out; the rest of the file loads.
- `/skript help|info|list|reload [all|scripts|config|aliases|last|<script or folder>]|enable|disable
  [all|<script or folder>]` with Tab completion (in game for operators, or from the console). Players with
  `skript.reloadnotify` see the lines of a reload, enable or disable. A reload that adds or removes commands
  refreshes the command tree.
- `on chat`: `cancel event` stops the message, `set message to ...` changes it.
- Waits and repeats run on server ticks. A tick runs at most 100 000 statements in total; the
  runs that do not fit go on on the next tick, so a loop cannot stall the server.

What is missing (most of Skript's syntax) is in the Known gaps section. Writing the rest is the job of the
syntax library in `core/src/syntax/` (see its README).

## Example

```skript
options:
	prefix: &7[Shop]

command /hello <text>:
	send "{@prefix} Hello, %arg-1%!" to player

on join:
	wait 2 seconds
	send "Welcome, %player%." to player

function double(n: integer) :: integer:
	return {_n} * 2
```

## Build

Build it as a native trait mod with arismoko's launcher (see the "native mods" how-to in the arismoko/horizon docs):

```sh
horizon-launcher native-build <path to this folder> --server-output <dir> --version <tag> --trust-native-code
```

`--trust-native-code` is your consent to compile and run this code inside the server. The logic crate tests run without the engine: `(cd core && cargo test)`.

## Known gaps

- Round 6 added entity-state reads and effects (fire, flight, speed, air, vehicle, bed, biome, world time/weather/difficulty, line of sight, invulnerable, silent, respawn) and the core effects `<effect> if <condition>`, `continue`, `assert`, `log to file`, script enable/reload/unload, `sort by`, `transform`. Still missing for lack of an engine read: ping, fall distance, light level, spectator target, ban/whitelist/version/plugin conditions, entity validity, damage cause (see GAPS_UPSTREAM.md). Missing in the port although the API exists: velocity (the vector class exists since round 8, the velocity expression does not), boss bar flags, explosions with fire, potion effect flags, `run %executable%`.

- Round 9 added inventory slots (`slot n of player`, `hotbar slot`, `tool`, `offhand tool`, `first empty slot`, `set slot`, `clear slot`), potion effect values (`potion effects of`, `has potion`, apply and remove by name), `nearest entity`, `entity type of`, random, spherical and cylindrical vectors, the look-direction vector and the `x|y|z component` setters. Still missing: the potion flags (ambient, icon, infinite) and the tier and duration expressions of a potion effect, the `direction` values, a custom name on a stack written to a slot (no `set_display` on main), non-player entity values with identity (see HMOD_PORT.md round 9).
- Effects and conditions beyond the common ones (about 39 of 153 effect classes and 31 of 170 condition classes exist): no entity-flag effects or conditions (ignite, sprint, fly, glide, sleep), no `restart server`; see HMOD_PORT.md round 3 for the list.

What does not work yet on arismoko/horizon (each is marked `TODO(arismoko-api: ...)` in `src/lib.rs`):

- `chat_as`, `place` triggers, player-move triggers, run-command-as-player and forced particle drawing have no arismoko API yet and are refused or approximated.
- `cancel event` in `on damage` cannot stop the hit (the damage hook has a level host that cannot run the scripts; HMOD-GAP(damage-cancel)).
- A stack's custom name is not read, `has played before` is approximated (no join-history call on main), and titles go through the vanilla `title` command.
- Script commands have no native argument parsing: the line is parsed by the mod, so there is no client-side completion or `player` selectors for their arguments. `prefix:`, `cooldown storage:`, `suggestions:` and `subcommand` entries of a command are read but not used; `every day at 12:00` and `every <realtime>` are not parsed.
- About 610 syntax classes of Skript 2.17.1 are still missing (most effects, conditions, expressions, events and types); the types without a value variant (inventory, boss bar, display and others) cannot be added without a new value model. Vector, slot and potion effect values exist since rounds 8 and 9. See HMOD_PORT.md round 3 for what exists.
- `set block` by name needs the block's default state; `summon` with NBT, hiding effect particles and the respawn point use workarounds or are missing.
- Expressions that need data the engine does not give yet (ping, fly and walk speed, total experience, velocity, respawn location, vehicle, item lore, biome, world time and weather) are missing; see HMOD_PORT.md round 4.
- Events of the level-host hooks (piston extend and retract, block spread, fade, form, ignite and burn, leaves decay, sponge absorb, bell ring, entity heal and jump, enderman enrage) never fire; the engine hands those hooks a level host, which cannot run scripts. See HMOD_PORT.md round 5.
- Particle names are limited to a fixed list of about 50.

## Licence

GPL-3.0-or-later. See LICENSE and NOTICE.

## Known gaps

- Most of Skript's syntax is still missing (about 346 of 1168 audited classes, see HMOD_PORT.md). Most missing
  effects, conditions and expressions need an engine call the mod API does not offer yet; they are listed under
  `skript` in the upstream gaps list.
- Not parsed: `on <event> of <type>` filters, `ExprTernary`, `ExprFormatDate`, `ExprHash`, `/skript update|restore`.
- Item custom names, lore and enchantments (no engine read), slot and inventory-slot values, potion effect values, entities as values and most other types beyond the classes listed above. Vectors exist since round 8 (random, spherical and cylindrical vectors and `x of %vector%` do not).
- Round 7 (expressions) is written without a build; run the tests once before relying on it.
- Round 8 (vector type, vector expressions, the player inventory read) is written without a build; run `cargo test` and check `inventory_slots` on the engine once before relying on it.
