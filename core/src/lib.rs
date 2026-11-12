//! Language core of the Skript port for Horizon: loads `.sk` files, parses them with Skript's
//! pattern syntax and runs the triggers, one step at a time, on server ticks.
//!
//! Nothing here touches the Horizon engine. The engine reaches the core through [`host::Host`]
//! (send, broadcast, player lookup), the [`manager::Manager`] (load, events, commands, ticks) and
//! the [`registry::Registry`] (syntax elements, which `syntax/` adds).

pub mod builtins;
pub mod classes;
pub mod config;
pub mod error;
pub mod expr;
pub mod host;
pub mod manager;
pub mod parse;
pub mod pattern;
pub mod registry;
pub mod run;
pub mod script;
pub mod syntax;
pub mod value;
pub mod vars;
pub mod yaml;

pub use host::Host;
pub use manager::Manager;
pub use registry::Registry;
pub use value::Value;
