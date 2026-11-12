> **Frozen branch (`horizon-plain`).** This is the version written for horizon-rust/horizon's native hooks / horizon-mixins. We stopped working on it on 2026-11-12: horizon-rust moved all mod API work to the fork [arismoko/horizon](https://github.com/arismoko/horizon) (see "Mod-related pull requests" in its CONTRIBUTING.md) and closed mod pull requests there. This branch is experimental and unsupported. The maintained version, for arismoko/horizon only, is on the default branch.

# horizon-skript

> **Unofficial Rust port of Skript for the Horizon server (https://github.com/horizon-rust/horizon). Not affiliated with or endorsed by the Skript authors.**

The Skript scripting language for [Horizon](https://github.com/horizon-rust/horizon) servers, as a
native trait mod: `.sk` files in a scripts folder, Skript's indentation and sections, its pattern
syntax for statements and expressions, variables and lists, functions, `wait`, events, commands and
the `/skript` command, with the same error messages and line numbers.

**Derived from Skript (https://github.com/SkriptLang/Skript, GPL-3.0).** The language follows
Skript's source: the pattern syntax (`[optional]`, `(a|b)`, `%type%`, `<regex>`, parse marks),
the structures (`on`, `command`, `function`, `options`, `if`/`else`, `loop`, `while`) and the
names of its syntax. The licence of this mod is therefore the GNU General Public License version 3
or later (see LICENSE); the copyright holders are in NOTICE.

**State: the language core is built and tested (125 tests in `core/`). The glue (`api/`) builds against
Horizon `1453e0e61`; it has not run inside a Horizon server yet (see STATUS.md and GAPS.md).**

Upstream: Skript (https://github.com/SkriptLang/Skript), commit `19172ca06e798dea53d97b647167e6ebe7f5fe94`
("Prepare For Release (2.17.1)"), GPL-3.0. Horizon pin: `1453e0e614af4b2d04dd0d228a94162b5d459910`.

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
- `/skript info`, `/skript list`, `/skript reload [name|all]`, `/skript enable <name>`,
  `/skript disable <name>` (in game for operators, or from the console).
- Waits and repeats run on server ticks. A tick runs at most 100 000 statements in total; the
  runs that do not fit go on on the next tick, so a loop cannot stall the server.

What is missing (most of Skript's syntax) is in STATUS.md. Writing the rest is the job of the
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

## Layout

- `core/` — the language, engine-free (`cargo test` here). `src/pattern.rs` is the pattern engine,
  `expr.rs` expressions, `parse.rs` statements and triggers, `run.rs` the interpreter,
  `manager.rs` the script manager, `classes.rs` the types, `builtins.rs` the builtin syntax,
  `syntax/` the syntax library (to be filled).
- `api/` — the trait mod: the hook glue (`api/src/lib.rs`).
- `horizon-mixin.toml` — the mod's manifest; `STATUS.md`, `GAPS.md` — what is done and what the
  engine lacks.

## Building and testing the core

```sh
(cd core && cargo test)
```

CI runs exactly that. The glue depends on the Horizon engine and is not built by CI.

## Licence

GPL-3.0-or-later. See LICENSE and NOTICE.
