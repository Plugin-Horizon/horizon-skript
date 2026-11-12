# Syntax library

This folder holds Skript's syntax beyond the language core: the effects, conditions,
expressions and events a script can use, one module per group (`effects_player.rs`,
`conditions_item.rs`, ...). The core (`core/src/`) knows how to read and run them; this folder
only says what they match and what they do.

## Register an element

Every element is a function or a type that implements one of the traits in
`core/src/registry.rs`, and a `register` call in the group's module. Then the group's `register`
is called from `syntax::register` in `mod.rs`.

```rust
use crate::error::RunError;
use crate::pattern::PatternError;
use crate::registry::{Flow, Matched, Registry};
use crate::run::Ctx;
use crate::value::Value;

pub fn register(reg: &mut Registry) -> Result<(), PatternError> {
    // Effect: a statement. Patterns are Skript's: %types% are slots (slot 0 is the first one),
    // [..] optional, (a|b) alternatives, <regex> a regex.
    reg.add_effect("heal", &["heal %players%"], 0, heal)?;

    // Expression: a value and the class it returns (checked where it is used).
    reg.add_expression("name", &["[the] name of %player%"], 0, "text", name_of)?;

    // Event: `on death:` fires when the glue calls `Manager::fire(host, "death", event)`.
    reg.add_event("death", &["death"])?;
    Ok(())
}

fn heal(cx: &mut Ctx<'_>, m: &Matched) -> Result<Flow, RunError> {
    let Some(slot) = m.expr(0) else { return Ok(Flow::Next) };
    // A plural slot is a list (`heal Alice and Bob`); a single player is not.
    let targets = match slot.eval(cx)? {
        Value::List(items) => items,
        one => vec![one],
    };
    for target in targets {
        if let Value::Player { uuid, .. } = target {
            cx.host.send(uuid, "You were healed.");
        }
    }
    Ok(Flow::Next)
}

fn name_of(cx: &mut Ctx<'_>, m: &Matched) -> Result<Value, RunError> {
    let player = m.expr(0).ok_or_else(|| RunError::new("missing player"))?.eval(cx)?;
    match player {
        Value::Player { name, .. } => Ok(Value::Text(name)),
        other => Err(RunError::new(format!("expected a player, got {}", other.class()))),
    }
}
```

Closures work too: `reg.add_effect("x", &["x"], 0, |cx: &mut Ctx<'_>, m: &Matched| ...)`; the
traits have blanket impls for functions with these signatures.

## Rules

- **Slots**: `%type%` takes an expression of that class. `%types%` (plural) takes a list
  (`a, b and c`). `%-type%` may be absent (the expression is then `None` in `Matched`). Names
  are the classes in `core/src/classes.rs` (`object`, `text`, `number`, `integer`, `boolean`,
  `timespan`, `date`, `player`, `world`, `location`, `item type`, `entity type`).
- **Evaluate slots** with `m.expr(i)` and `expr.eval(cx)`, then convert with
  `classes::convert(value, "number", cx.host)` when the code needs one class.
- **Marks and tags**: `(1¦is|2¦are)` sets mark 1 when the first alternative matches (`m.has_mark(1)`);
  `(fast:now|slow)` sets the tag `fast` (`m.has_tag("fast")`). Regex groups: `m.regex(0)`.
- **Effects return a `Flow`**: `Flow::Next`, `Flow::Wait(ticks)` (a `wait`), `Flow::Stop`,
  `Flow::Exit{..}`, `Flow::Return(..)`. Never block: a long job is split across ticks with
  `Flow::Wait`.
- **Errors**: return `Err(RunError::new("message"))`; the core adds the file and line.
- **Events** carry their data through `Event` in `core/src/run.rs` (`player`, `message`, `args`);
  the glue fills it when it calls `Manager::fire`. Add a field there if a new event needs more.
- **Priority**: lower runs first. Builtins use 0. Use a higher number when a pattern is a
  catch-all that should come after the specific ones.
- **Tests**: every group ships tests next to it, and at least one script test in
  `core/tests/` that parses and runs the syntax (see `core/tests/language.rs`).

## Where things are

| Need | Look at |
|---|---|
| Pattern syntax | `core/src/pattern.rs` (module docs) |
| Types and conversions | `core/src/classes.rs` |
| Values and text form | `core/src/value.rs` |
| The engine's host (send, broadcast, player lookup) | `core/src/host.rs` |
| Variables | `core/src/vars.rs` |
| What is built in | `core/src/builtins.rs` |

Keep the group's file names in `STATUS.md` up to date (which Skript syntax groups exist and
which are missing).
