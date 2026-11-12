# Status (2026-11-09)

Legend: **done** = implemented and covered by tests in `core/`; **partly** = works for part of
Skript's cases; **missing** = not written yet (the syntax library, `core/src/syntax/`, is where
it goes). The glue (`api/`) builds against Horizon 1453e0e61 (`cargo build` in `api/`, no errors); its behaviour has
not run on a server. See GAPS.md.

The core crate has 125 tests, all green: 51 unit tests (`cargo test --lib`), 31 script-level tests
(`core/tests/language.rs`) that load small `.sk` sources, fire events, tick and check what the host
received, 20 effect tests (`tests/effects.rs`) and 23 syntax-library tests (`tests/syntax_library.rs`).

## Language core (`core/src/`)

| Feature | Status | Where |
|---|---|---|
| Loading `.sk` files from a folder, recursive, `-` prefix loads disabled | done | `manager.rs` `load_dir` |
| Reload one script or all, enable, disable, `/skript` command | done | `manager.rs` `reload`, `enable`, `disable`, `skript_command` |
| Comments (`#` outside strings, whole lines), blank lines | done | `script.rs` |
| Indentation: sections, one unit per level, tabs and spaces not mixed | done | `script.rs` |
| Parse errors: `file, line N: message: "text"`, all errors of a file together | done | `error.rs`, `parse.rs` |
| Run-time errors with the line they happened on | done | `run.rs`, `manager.rs` |
| `options:` and `{@name}` | done | `script.rs` |
| Triggers: `on <event>:`, `command /name <args>:`, `every <n> <unit>:` | done | `parse.rs` |
| Events: join, quit / leave, chat, load, unload | done | `builtins.rs` |
| Functions: parameters, return type, calls as expressions and statements, `return`, recursion limit | done | `parse.rs`, `run.rs` |
| Functions that wait | missing (an error for now) | `run.rs` |
| `if` / `else if` / `else` | done | `parse.rs`, `run.rs` |
| `loop <list>:` with `loop-value`, `loop-index` | done | `run.rs` |
| `while <condition>:` | done | `run.rs` |
| `stop`, `exit`, `stop trigger`, `exit loop`, `exit N loops`, `exit N sections` | done | `builtins.rs` |
| `wait <timespan>` (resumes on the tick, 50 ms each, at least 1) | done | `builtins.rs`, `manager.rs` |
| Step budget: 100 000 statements per server tick shared by all runs, 10 000 per event or command; the rest goes on next tick | done | `run.rs`, `manager.rs` |
| Pattern engine: literals (case-insensitive), `[optional]`, `(a\|b)`, top-level `\|`, `%type%`, `%types%` (lists), `%-type%` (nullable), `%number/integer%`, `<regex>`, parse marks `1¦`, tags `name:` | done | `pattern.rs` |
| Pattern engine: spaces around optional parts (`[the] player`) | done | `pattern.rs` |
| Pattern engine: `%~type%` kept as a marker only | partly | `pattern.rs` |
| Expressions: literals, text with `%expr%`, `%%`, `\x` escapes | done | `expr.rs` |
| Arithmetic `+ - * / ^`, precedence, parentheses; `+` joins text | done | `expr.rs` `arith` |
| Division by zero is a run-time error | done | `expr.rs` |
| Variables `{x}`, `{x::y}`, `{_local}`, interpolated names `{a.%player%}` | done | `expr.rs`, `vars.rs` |
| List variables `{x::*}`, `set`/`add`/`remove`/`delete`/`clear`, list copies, indices | done | `builtins.rs`, `vars.rs` |
| Variables saved to disk (Skript's `variables.csv`) | missing | GAPS.md |
| Types: text, number, integer, boolean, timespan, date (UTC), player (by name, host lookup), world, location, item type, entity type: parse, text form, convert | done (item and entity existence: host, default accepts) | `classes.rs`, `value.rs` |
| Static type check of slots, text takes any value | done | `classes::accepts` |
| Event data: player, message, `arg-N` | done | `builtins.rs`, `run.rs` |
| Random, time, math and text functions of Skript | missing | syntax library |

## Builtin syntax (`core/src/builtins.rs`)

| Syntax | Kind |
|---|---|
| `set %object% to %object%` | effect |
| `add %object% to %object%`, `remove %object% from %object%` | effect |
| `delete %object%`, `clear %object%` | effect |
| `wait %timespan%` | effect |
| `send %text% to %player%` / `message ...`, `broadcast %text%` | effect |
| `return [%object%]`, `stop [trigger]`, `exit [loop/section/N loops]` | effect |
| `%object% is set`, `%object% is not set` | condition |
| `[the] player`, `[the] message`, `arg-%integer%`, `loop-value`, `loop-index` | expression |
| `join`, `quit`, `leave`, `chat`, `load`, `unload` | event |

## Effects and events (`core/src/syntax/effects/`, `core/src/syntax/events/`)

The effects use the engine through `Actions` (`core/src/host.rs`); the glue's implementation is
UNVERIFIED and some are stubs (GAPS.md). Patterns follow Skript's registrations. The effects with
priority -1 or -2 are tried before the builtins (see `effects/mod.rs`).

| Effect (Skript's pattern, short form) | Status | Where |
|---|---|---|
| `send %texts% [to %players%]`, `message ...`, `broadcast %texts%` (several texts) | done | `effects/messages.rs` |
| `send action bar %text% [to %players%]` | done (glue UNVERIFIED) | `effects/messages.rs` |
| `send title %text% [with subtitle %text%] [to ...] [for T] [with fade in T] [and fade out T]`, `send subtitle ...`, Bukkit's default times | done (glue UNVERIFIED) | `effects/messages.rs` |
| `make %players% say %texts%`, `force ... to send the message` | done; no-op inside a player's hook (GAPS.md) | `effects/messages.rs` |
| `kick`, `ban [by reason of]`, `unban`, `op`, `[de-]op`, `kill` (players) | done (vanilla command as console) | `effects/players.rs` |
| `damage %players% by N hearts`, `heal %players% [by N hearts]` | damage done; heal by points is a stub | `effects/players.rs` |
| `set health of %players% to N`, `set food|hunger of ... to N`, `set game mode of ... to <mode>` | done (glue UNVERIFIED) | `effects/players.rs` |
| `teleport %players% to %location%` | done (glue UNVERIFIED) | `effects/players.rs` |
| `push %players% up|down|north|south|east|west [at speed N]`, `push ... towards|away from %location%` | done; glue stubs (GAPS.md) | `effects/players.rs` |
| `apply|grant <effect> [level] to %players% [for T]`, `affect ... with <effect>` | done (vanilla `effect give`) | `effects/players.rs` |
| `cancel [the] event`, `uncancel [the] event` | done (the glue reads the flag) | `effects/players.rs` |
| `execute [the] command %texts% [by %player%]`, `execute %player% command %texts%`, `make %player% execute ...` | done | `effects/players.rs` |
| `give %item types% to %players%`, `remove %item types% from %players%` (other `remove` goes to the builtin), `clear [the] inventory of %players%` | done (vanilla `give`, `clear`) | `effects/items.rs` |
| `set block at %location% to %item type%` | done; glue stub (GAPS.md) | `effects/world.rs` |
| `play sound %text% [with volume N] [and pitch N] [to %players%]` | done (vanilla `playsound` in `execute at`) | `effects/world.rs` |
| `spawn %entity types% at %location%` | done; glue stub (GAPS.md) | `effects/world.rs` |

| Event (Skript's pattern) | Status | Where |
|---|---|---|
| `first join`, `respawn`, `death`, `damage`, `move`, `command` | patterns done; the glue fires them, cancel and values partly (GAPS.md) | `events/player.rs` |
| `break`, `place`, `click`, `left click`, `right click` | patterns done; the glue fills `location` only (GAPS.md) | `events/block.rs` |
| `server start`, `server stop` | patterns done (glue UNVERIFIED) | `events/mod.rs` |

The values of the events are documented in `events/mod.rs` (`cx.event.value("victim")`, for the
expressions). Filters (`on break of stone`) are not parsed.

The effects and events have 33 tests: 13 unit tests in their files and 20 script tests in
`core/tests/effects.rs`.

## Conditions, expressions and functions (`core/src/syntax/conditions/`, `expressions/`, `functions.rs`)

Patterns are Skript's registrations (`CondCompare`, `CondContains`, `CondIsOnline`, `ExprName`, ...),
with the forms Skript has for the common ones. Player values come from the host (`Host::player_state`,
see GAPS.md): until the glue fills it, every player property fails with "is not online", and only
`online players` and the names work. Negated forms (`is not`, `isn't`, `does not contain`) carry
parse mark 1; `is dead` carries mark 2.

| Condition | Status | Where |
|---|---|---|
| `%objects% is/isn't [equal to] %objects%`, `=`, `!=` (numbers across integer and decimal, text and names ignoring case, locations, item types by id) | done | `conditions/comparison.rs` |
| `>`, `<`, `>=`, `<=` and the words (`greater than`, `or equal to`, `below`, ...); `is between a and b` (inclusive) | done (unordered classes are an error) | `conditions/comparison.rs` |
| `contains`, `does not contain` (text substring, lists by item) | done | `conditions/comparison.rs` |
| `starts with`, `ends with`, `is empty`, `is evenly divisible by` | done | `conditions/comparison.rs` |
| `is online`, `is an operator`, `is alive` / `is dead`, `is flying`, `can fly`, `is sneaking`, `is sprinting`, `is swimming` | done (glue state needed) | `conditions/player.rs` |
| `is holding <item types>` (by id, not amount), `has <item types>` / `does not have` (by amount in the inventory) | done (glue state needed) | `conditions/player.rs` |
| `has permission <texts>`, `can see <player>` | done (glue needed, default false) | `conditions/player.rs` |
| `is in world <world>` (players and locations) | done | `conditions/player.rs` |
| `chance of N` and `chance of N%` (the host's random number) | done | `conditions/misc.rs` |

| Expression | Status | Where |
|---|---|---|
| `[the] name of`, `display name`, `uuid`, `health`, `max health`, `food level`, `level`, `game mode`, `tool` / `held item`, `location`, `inventory`, `target block`, `[all] [the] [online] players` | done (all need the glue's player state, except `online players`) | `expressions/player.rs` |
| `x`/`y`/`z` of a location or player, `world of`, `distance between`, `block at` | done (`block at` and the target block need the glue) | `expressions/location.rs` |
| `length of`, `uppercase`/`lowercase`, `substring of ... from A to B` (1-based, inclusive), `character at index`, `join`/`concat` (no delimiter by default), `split ... at`, `replace all ... with ... in` | done | `expressions/text.rs` |
| `amount`/`number`/`size of`, `random element out of`, `random integer/number between A and B`, `now` | done | `expressions/misc.rs` |
| `attacker` / `damager`, `victim`, `event-<player/location/block/item/from/to/message/damage/cause>` (the event's values) | done (the glue fills the values, see events) | `expressions/event.rs` |

| Function | Status | Where |
|---|---|---|
| `abs`, `ceil`/`ceiling`, `floor`, `round(n[, digits])` (half up), `mod`, `exp`, `ln`, `log(n[, base])`, `sqrt`, `sin`/`cos`/`tan` and `asin`/`acos`/`atan`/`atan2` in degrees, `sum`, `product`, `min`, `max`, `mean`, `clamp` | done | `functions.rs` |
| `now()`, `date(y, m, d[, h, min, s])` (UTC), `location(x, y, z[, world])` (world `world` by default), `world(name)`, `concat(...)` | done | `functions.rs` |
| `vector(...)`, `uuid(...)`, `formatNumber`, `isNaN`, `toBase`/`fromBase`, `quaternion`, `rgb`, `offlineplayer`, `entity` | missing (no vector type in the core, see GAPS) | - |

The helpers are in `syntax/support.rs` (slots, lists, equality and order rules, UUID text). Built-in
functions are a table in the registry (`Registry::add_function`): a call `name(args)` with the right
number of arguments goes there, otherwise to a script function.

The conditions, expressions and functions have 32 tests: 9 unit tests in their files, and 23 script
tests in `core/tests/syntax_library.rs` (one group per syntax family). The optional-part rule of
the pattern engine (`max[imum] health`) has a unit test in `pattern.rs`; the list-first rule of the
parser (`64 stone and 1 flint`) is covered by a script test.

## Syntax library (`core/src/syntax/`, to be written)

Skript's source has these groups (file counts in `skript/src/main/java/ch/njol/skript/`, from the
reference clone at the time of writing). Nothing of them is in the core yet, except the rows
marked partly, where the builtins above cover a few members.

| Group (Skript folder) | Files | Status |
|---|---|---|
| `effects` | 107 | partly (the common ones, see the effects table above) |
| `conditions` | 130 | partly (about 23 elements, see the conditions table below) |
| `expressions` | 342 | partly (about 32 elements, see the expressions table below) |
| `events` | 45 | partly (the player, block and server events, see above; entities and inventories are missing) |
| `sections` | 9 | partly (`if`, `else`, `loop`, `while` are in the core; `for`, `filter`, `catch errors`, `spawn`, `shoot` missing) |
| `structures` | 10 | partly (`function`, `options`, `on`, `command`, `every` in the core; `variables`, `aliases`, `using`, `auto reload`, `example` missing) |
| `classes` (types registered with `registerClass`) | ~83 | partly (the 12 types of the core; the others missing) |
| `functions` (`registerFunction`, Skript's built-in functions) | ~43 | partly (the math, time and text ones, see below; no vector, no `uuid()`) |
| converters, comparators, operations (`registerConverter` 59, `registerComparator` 44, `registerOperation` 27) | ~130 | partly: the comparison rules are in `syntax/support.rs` (not Skript's comparator registry); no other converters |

The comparisons, the common expression properties and functions are in (see the tables below and
above), with the gaps in GAPS.md. Next: the rest of Skript's expressions (time, vector, entities,
inventories by slot) and the sections (`for`, `filter`).

## Glue (`api/`)

| Feature | Status |
|---|---|
| Scripts folder from config (`config/skript-common.toml`, `scripts-folder`), default `plugins/Skript/scripts` | written |
| Scripts loaded when the server first asks for commands or ticks (no server-ready hook used) | written |
| `/skript` (operators), console `skript ...` | written |
| Script commands registered, dispatched with `<args:text>` | written |
| join (`PlayerLoggedInEvent`), quit (`PlayerLoggedOutEvent`) | written (UNVERIFIED field names) |
| chat: queued in the chat hook, fired on the next tick | written (no cancel, one tick late) |
| Tick: runs waits and repeats | written |
| Messages to players (`tell`), broadcast to online players, log to stderr | written |
| Engine actions for the effects (`Actions`: commands as console and as a player, action bar, title, chat, teleport, set block, spawn, health, food, heal, push) | written, UNVERIFIED names; set block, spawn, heal by points and push are stubs (GAPS.md) |
| Events from the engine: first join, respawn, death, damage (queued), break, place, click and left/right click, move (queued), command, server start and stop | written, UNVERIFIED types; cancel works for the hooks with a host (GAPS.md) |
| Compiled against the engine | not yet |
