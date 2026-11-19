//! The script manager: finds the `.sk` files, keeps what they parsed to, starts triggers on
//! events and commands, runs the waits and repeats on server ticks, and answers `/skript`.
//!
//! A file whose name starts with `-` is loaded disabled (Skript's convention); `/skript enable`
//! and `/skript disable` switch scripts at run time without touching the files.

use crate::horizon_skript_core::config;
use crate::horizon_skript_core::error::ScriptError;
use crate::horizon_skript_core::host::Host;
use crate::horizon_skript_core::classes;
use crate::horizon_skript_core::parse::{
    self, ArgSpec, Body, CommandInfo, ExecutableBy, HeaderPart, Script, Trigger, TriggerKind,
};
use crate::horizon_skript_core::registry::Registry;
use crate::horizon_skript_core::run::{CmdArg, Env, Event, Functions, Outcome, Run, STEP_BUDGET};
use crate::horizon_skript_core::value::{self, Value, format_chat};
use crate::horizon_skript_core::vars::{self, RawRow};
use regex::Regex;

/// Statements all the runs of one server tick may execute together.
const TICK_BUDGET: u32 = 100_000;
use crate::horizon_skript_core::vars::Variables;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The database (`variables.csv`) the variables are saved in.
#[derive(Clone, Debug)]
pub struct Database {
    pub file: PathBuf,
    /// Which variable names this database saves (`pattern` in `config.sk`, `.*` by default).
    pub pattern: Regex,
}

/// The `config.sk` keys the port reads, with Skript's defaults.
#[derive(Clone, Debug)]
pub struct Settings {
    /// `case-insensitive variables`.
    pub fold_case: bool,
    /// `number accuracy`: decimals a non-whole number prints with.
    pub number_accuracy: u32,
    /// `variable changes until save`: saved changes after which the variables are written.
    pub changes_until_save: u64,
    /// The first CSV database. `None` keeps the variables in memory only.
    pub database: Option<Database>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fold_case: true,
            number_accuracy: 2,
            changes_until_save: 1000,
            database: None,
        }
    }
}

/// Reads the keys of a parsed `config.sk`. A file path of a database is taken from the server
/// folder (`./plugins/Skript/variables.csv`).
pub fn settings_from_config(root: &config::Node, server_dir: &Path) -> Settings {
    let mut s = Settings::default();
    let yes = |v: &str| matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "on");
    if let Some(n) = root.get("case-insensitive variables") {
        s.fold_case = yes(&n.value);
    }
    if let Some(n) = root.get("number accuracy").and_then(|n| n.value.parse().ok()) {
        s.number_accuracy = n;
    }
    if let Some(n) = root
        .get("variable changes until save")
        .and_then(|n| n.value.parse().ok())
    {
        s.changes_until_save = n;
    }
    if let Some(dbs) = root.get("databases") {
        s.database = dbs
            .children
            .iter()
            .find(|db| db.get("type").is_some_and(|t| t.value.eq_ignore_ascii_case("csv")))
            .and_then(|db| {
                let file = db.get("file")?.value.clone();
                let pattern = db.get("pattern").map_or(".*", |p| p.value.as_str());
                let pattern = Regex::new(&format!("^(?:{pattern})$")).ok()?;
                Some(Database {
                    file: server_dir.join(file),
                    pattern,
                })
            });
    }
    s
}

struct Loaded {
    script: Option<Arc<Script>>,
    enabled: bool,
    errors: Vec<ScriptError>,
    path: Option<PathBuf>,
}

struct Pending {
    due: u64,
    script: String,
    run: Run,
}

struct Repeat {
    script: String,
    body: Body,
    file: Arc<str>,
    every: u64,
    next: u64,
}

pub struct Manager {
    registry: Registry,
    dir: PathBuf,
    scripts: BTreeMap<String, Loaded>,
    vars: Variables,
    /// The rows of the variables file this port could not decode, written back on save.
    raw: Vec<RawRow>,
    settings: Settings,
    loaded_vars: bool,
    funcs: Functions,
    pending: Vec<Pending>,
    repeats: Vec<Repeat>,
    tick: u64,
    /// When a player last ran a command with a `cooldown:` (script, command, player): milliseconds.
    cooldowns: HashMap<(String, String, u128), i64>,
    /// The last target of `/skript reload`, for `/skript reload last`.
    last_reloaded: Option<String>,
}

/// Who runs a command: a player (UUID and name) or the console, and whether they are an operator.
#[derive(Clone, Debug)]
pub struct CommandSender {
    pub player: Option<(u128, String)>,
    pub op: bool,
}

impl Manager {
    /// A manager for the scripts in `dir`. Nothing is read until [`Manager::load_dir`].
    pub fn new(registry: Registry, dir: impl Into<PathBuf>) -> Self {
        Manager {
            registry,
            dir: dir.into(),
            scripts: BTreeMap::new(),
            vars: Variables::new(),
            raw: Vec::new(),
            settings: Settings::default(),
            loaded_vars: false,
            funcs: Functions::new(),
            pending: Vec::new(),
            repeats: Vec::new(),
            tick: 0,
            cooldowns: HashMap::new(),
            last_reloaded: None,
        }
    }

    pub fn variables(&self) -> &Variables {
        &self.vars
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Applies the settings: number accuracy, and whether names are case-insensitive. Call it
    /// before the variables are loaded.
    pub fn configure(&mut self, settings: Settings) {
        value::set_number_accuracy(settings.number_accuracy);
        if self.vars.is_empty() {
            self.vars = Variables::with_case_folding(settings.fold_case);
        }
        self.settings = settings;
    }

    /// Reads `config.sk` (the keys the port uses) and applies it. Returns the report lines.
    pub fn load_config(&mut self, config_file: &Path, server_dir: &Path) -> Vec<String> {
        match fs::read_to_string(config_file) {
            Ok(text) => {
                let root = config::parse(&text);
                self.configure(settings_from_config(&root, server_dir));
                Vec::new()
            }
            Err(e) => vec![format!(
                "[Skript] cannot read {}, using the defaults: {e}",
                config_file.display()
            )],
        }
    }

    /// Reads the variables file of the database, if there is one. A missing file starts empty.
    /// Returns the report lines (also logged).
    pub fn load_variables(&mut self, host: &dyn Host) -> Vec<String> {
        self.loaded_vars = true;
        let Some(db) = self.settings.database.clone() else {
            return Vec::new();
        };
        let mut report = Vec::new();
        match fs::read_to_string(&db.file) {
            Ok(text) => {
                let loaded = vars::load_csv(&text, self.settings.fold_case);
                report.push(format!(
                    "[Skript] loaded {} variable(s) from {}",
                    loaded.vars.len(),
                    db.file.display()
                ));
                if !loaded.raw.is_empty() {
                    report.push(format!(
                        "[Skript] {} variable(s) of a type this port does not read are kept as they are",
                        loaded.raw.len()
                    ));
                }
                for p in &loaded.problems {
                    report.push(format!("[Skript] {}: {p}", db.file.display()));
                }
                self.vars = loaded.vars;
                self.raw = loaded.raw;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.vars = Variables::with_case_folding(self.settings.fold_case);
                self.raw.clear();
            }
            Err(e) => report.push(format!(
                "[Skript] cannot read {}: {e}",
                db.file.display()
            )),
        }
        for line in &report {
            host.log(line);
        }
        report
    }

    /// Writes the variables to the database file (Skript's `variables.csv`): the names the
    /// database's pattern takes, memory-only names left out. Does nothing without a database.
    pub fn save_variables(&mut self, host: &dyn Host) -> Result<(), String> {
        let Some(db) = self.settings.database.clone() else {
            return Ok(());
        };
        let touched = self.vars.touched().clone();
        let raw: Vec<RawRow> = self
            .raw
            .iter()
            .filter(|r| !touched.contains(&r.name))
            .cloned()
            .collect();
        let keep = |name: &str| db.pattern.is_match(name);
        let (text, skipped) = vars::save_csv(&self.vars, &raw, &keep);
        if skipped > 0 {
            host.log(&format!(
                "[Skript] {skipped} variable(s) of a type that is not saved yet were left out"
            ));
        }
        let tmp = db.file.with_extension("csv.tmp");
        let written = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &db.file));
        match written {
            Ok(()) => {
                self.vars.saved();
                self.raw = raw;
                Ok(())
            }
            Err(e) => Err(format!("cannot write {}: {e}", db.file.display())),
        }
    }

    /// The command triggers of the enabled scripts with their metadata, for the glue to register
    /// (`aliases:`, `usage:`, `description:`).
    pub fn command_infos(&self) -> Vec<(String, CommandInfo)> {
        self.scripts
            .values()
            .filter(|l| l.enabled)
            .filter_map(|l| l.script.as_ref())
            .flat_map(|s| s.triggers.iter())
            .filter_map(|t| match &t.kind {
                TriggerKind::Command(c) => Some((c.clone(), t.command.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn variables_mut(&mut self) -> &mut Variables {
        &mut self.vars
    }

    /// Reads every `.sk` file under the scripts folder, the subfolders included. Returns the
    /// report lines (also written to the host's log).
    pub fn load_dir(&mut self, host: &dyn Host) -> Vec<String> {
        let mut report = Vec::new();
        if !self.loaded_vars {
            report.append(&mut self.load_variables(host));
        }
        let mut files = Vec::new();
        if let Err(e) = collect(&self.dir, "", &mut files) {
            let line = format!(
                "[Skript] cannot read the scripts folder {}: {e}",
                self.dir.display()
            );
            host.log(&line);
            report.push(line);
            return report;
        }
        files.sort();
        for rel in files {
            let path = self.dir.join(&rel);
            // A file whose name or any folder above it starts with `-` (`-examples/`, `-old.sk`)
            // is disabled: listed, but not parsed until `/skript enable` (Skript reports nothing
            // for it at load).
            let enabled = !rel.split('/').any(|part| part.starts_with('-'));
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let mut lines = self.insert(host, &rel, &text, enabled, Some(path));
                    report.append(&mut lines);
                }
                Err(e) => {
                    let line = format!("[Skript] cannot read {rel}: {e}");
                    host.log(&line);
                    report.push(line);
                }
            }
        }
        // `on skript load` is Skript's server-start event: the glue fires "server start" once
        // the server has started, not on every load of the folder (a `/skript reload` does not
        // fire it, as in Skript).
        report
    }

    /// Loads a script from text, under the name `key` (for example `greet.sk`). Errors are
    /// returned and logged; the script is then not loaded.
    pub fn load_source(
        &mut self,
        host: &dyn Host,
        key: &str,
        text: &str,
        enabled: bool,
    ) -> Result<(), Vec<ScriptError>> {
        self.unload_script(host, key);
        // The triggers without a problem load, as in Skript; the errors are returned.
        let (script, errors) = parse::parse_script_partial(&self.registry, host, key, text);
        self.scripts.insert(
            key.to_owned(),
            Loaded {
                script: Some(Arc::new(script)),
                enabled,
                errors: errors.clone(),
                path: None,
            },
        );
        self.rebuild();
        if enabled {
            self.fire_for(host, key, "load", Event::default());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Parses a file's text and stores it, logging its errors.
    fn insert(
        &mut self,
        host: &dyn Host,
        key: &str,
        text: &str,
        enabled: bool,
        path: Option<PathBuf>,
    ) -> Vec<String> {
        self.unload_script(host, key);
        let mut report = Vec::new();
        if !enabled {
            self.scripts.insert(key.to_owned(), Loaded { script: None, enabled, errors: Vec::new(), path });
            return report;
        }
        let (script, errors) = parse::parse_script_partial(&self.registry, host, key, text);
        for e in &errors {
            report.push(format!("[Skript] {e}"));
        }
        if errors.is_empty() {
            report.push(format!("[Skript] loaded {key}"));
        } else {
            report.push(format!(
                "[Skript] {key} has {} error(s); the triggers and functions without one were loaded",
                errors.len()
            ));
        }
        self.scripts.insert(
            key.to_owned(),
            Loaded {
                script: Some(Arc::new(script)),
                enabled,
                errors,
                path,
            },
        );
        for line in &report {
            host.log(line);
        }
        self.rebuild();
        if enabled {
            self.fire_for(host, key, "load", Event::default());
        }
        report
    }

    /// Recomputes the function table and the repeating triggers from the enabled scripts.
    fn rebuild(&mut self) {
        self.funcs.clear();
        self.repeats
            .retain(|r| self.scripts.get(&r.script).is_some_and(|l| l.enabled));
        let mut owner: BTreeMap<String, String> = BTreeMap::new();
        for (key, loaded) in &self.scripts {
            if !loaded.enabled {
                continue;
            }
            let Some(script) = &loaded.script else {
                continue;
            };
            for f in &script.functions {
                // The first script to define a function name keeps it.
                if owner.contains_key(&f.name) {
                    continue;
                }
                owner.insert(f.name.clone(), key.clone());
                self.funcs.insert(f.name.clone(), f.clone());
            }
            for t in &script.triggers {
                if let TriggerKind::Every(ticks) = t.kind {
                    let known = self
                        .repeats
                        .iter()
                        .any(|r| r.script == *key && Arc::ptr_eq(&r.body, &t.body));
                    if !known {
                        self.repeats.push(Repeat {
                            script: key.clone(),
                            body: t.body.clone(),
                            file: script.file.clone(),
                            every: ticks,
                            next: self.tick,
                        });
                    }
                }
            }
        }
    }

    /// Stops a script's triggers: fires its `unload` event, drops its waits and repeats.
    fn unload_script(&mut self, host: &dyn Host, key: &str) {
        let was_enabled = self
            .scripts
            .get(key)
            .is_some_and(|l| l.enabled && l.script.is_some());
        if was_enabled {
            self.fire_for(host, key, "unload", Event::default());
        }
        self.pending.retain(|p| p.script != key);
        self.repeats.retain(|r| r.script != key);
        self.scripts.remove(key);
    }

    /// Re-reads one script (or `all`) from its file. Returns the report lines.
    pub fn reload(&mut self, host: &dyn Host, target: &str) -> Vec<String> {
        let keys: Vec<String> = if target == "all" {
            self.scripts.keys().cloned().collect()
        } else {
            match self.find(target) {
                Some(k) => vec![k],
                None => return vec![format!("No script named {target}.")],
            }
        };
        let mut report = Vec::new();
        for key in keys {
            let (enabled, path) = match self.scripts.get(&key) {
                Some(l) => (l.enabled, l.path.clone()),
                None => continue,
            };
            let path = path.unwrap_or_else(|| self.dir.join(&key));
            match fs::read_to_string(&path) {
                Ok(text) => report.extend(self.insert(host, &key, &text, enabled, Some(path))),
                Err(e) => report.push(format!("[Skript] cannot read {key}: {e}")),
            }
        }
        if target == "all" {
            report.push(format!(
                "[Skript] reloaded {} script(s)",
                self.scripts.len()
            ));
        }
        report
    }

    /// Switches a script on: its `load` event fires and its triggers run again.
    pub fn enable(&mut self, host: &dyn Host, name: &str) -> Result<(), String> {
        let key = self
            .find(name)
            .ok_or_else(|| format!("No script named {name}."))?;
        let loaded = self.scripts.get_mut(&key).expect("found above");
        if loaded.enabled {
            return Ok(());
        }
        // A script that came from a file is read again when it is enabled, as Skript loads the
        // file it renamed back (a disabled one was not parsed at load).
        if let Some(text) = loaded.path.as_ref().and_then(|p| fs::read_to_string(p).ok()) {
            let (script, errors) = parse::parse_script_partial(&self.registry, host, &key, &text);
            for e in &errors {
                host.log(&format!("[Skript] {e}"));
            }
            loaded.script = Some(Arc::new(script));
            loaded.errors = errors;
        }
        if loaded.script.is_none() {
            return Err(format!("{key} has errors and cannot be enabled."));
        }
        loaded.enabled = true;
        self.rebuild();
        self.fire_for(host, &key, "load", Event::default());
        Ok(())
    }

    /// Switches a script off: its `unload` event fires and its waits and repeats stop.
    pub fn disable(&mut self, host: &dyn Host, name: &str) -> Result<(), String> {
        let key = self
            .find(name)
            .ok_or_else(|| format!("No script named {name}."))?;
        if !self.scripts.get(&key).is_some_and(|l| l.enabled) {
            return Ok(());
        }
        self.fire_for(host, &key, "unload", Event::default());
        self.pending.retain(|p| p.script != key);
        self.repeats.retain(|r| r.script != key);
        if let Some(l) = self.scripts.get_mut(&key) {
            l.enabled = false;
        }
        self.rebuild();
        Ok(())
    }

    /// The key of a script from its name, with or without `.sk`.
    fn find(&self, name: &str) -> Option<String> {
        let name = name.trim();
        self.scripts
            .keys()
            .find(|k| k.as_str() == name || k.strip_suffix(".sk") == Some(name))
            .cloned()
    }

    /// Starts every enabled trigger of `event` (`join`, `chat`, ...) with the event's data.
    pub fn fire(&mut self, host: &dyn Host, event: &str, data: Event) {
        let mut starts: Vec<(String, Body, Arc<str>)> = Vec::new();
        for (key, loaded) in &self.scripts {
            if !loaded.enabled {
                continue;
            }
            let Some(script) = &loaded.script else {
                continue;
            };
            for t in &script.triggers {
                if matches!(t.kind, TriggerKind::Event(e) if e == event) {
                    starts.push((key.clone(), t.body.clone(), script.file.clone()));
                }
            }
        }
        for (key, body, file) in starts {
            self.start(
                host,
                key,
                Run::new(body, file, data.clone(), Variables::new(), 0),
            );
        }
    }

    /// Fires an event for one script only (`load`, `unload`).
    fn fire_for(&mut self, host: &dyn Host, key: &str, event: &'static str, data: Event) {
        let mut starts: Vec<(Body, Arc<str>)> = Vec::new();
        if let Some(script) = self.scripts.get(key).and_then(|l| l.script.clone()) {
            for t in &script.triggers {
                if matches!(t.kind, TriggerKind::Event(e) if e == event) {
                    starts.push((t.body.clone(), script.file.clone()));
                }
            }
        }
        for (body, file) in starts {
            self.start(
                host,
                key.to_owned(),
                Run::new(body, file, data.clone(), Variables::new(), 0),
            );
        }
    }

    /// Runs a command trigger for `name` (lower case, no slash). `false` when no script has it.
    pub fn dispatch_command(&mut self, host: &dyn Host, name: &str, data: Event) -> bool {
        let mut starts: Vec<(String, Body, Arc<str>)> = Vec::new();
        // The player who ran the command, when a player did (the console has every permission).
        let sender = match &data.player {
            Some(Value::Player { uuid, .. }) => Some(*uuid),
            _ => None,
        };
        let mut denied: Option<String> = None;
        let mut found = false;
        for (key, loaded) in &self.scripts {
            if !loaded.enabled {
                continue;
            }
            let Some(script) = &loaded.script else {
                continue;
            };
            for t in &script.triggers {
                if !matches!(&t.kind, TriggerKind::Command(c) if c == name) {
                    continue;
                }
                found = true;
                let lacks = match (&t.command.permission, sender) {
                    (Some(node), Some(uuid)) => !host.has_permission(uuid, node),
                    _ => false,
                };
                if lacks {
                    denied.get_or_insert_with(|| {
                        t.command.permission_message.clone().unwrap_or_else(|| {
                            "You don't have the required permission to use this command".to_owned()
                        })
                    });
                    continue;
                }
                starts.push((key.clone(), t.body.clone(), script.file.clone()));
            }
        }
        if let (Some(message), Some(uuid)) = (denied, sender) {
            host.send(uuid, &message);
        }
        for (key, body, file) in starts {
            self.start(
                host,
                key,
                Run::new(body, file, data.clone(), Variables::new(), 0),
            );
        }
        found
    }

    /// Runs the script commands named `name` (or having it as an alias) for a command line:
    /// `rest` is the text after the name. The sender must be allowed (`executable by:`,
    /// `permission:`, `cooldown:`) and the arguments must fit the header, else the sender gets
    /// the reason (and the usage) and nothing runs. `false` when no script has such a command.
    pub fn run_command(
        &mut self,
        host: &dyn Host,
        name: &str,
        sender: &CommandSender,
        rest: &str,
    ) -> bool {
        let name = name.to_ascii_lowercase();
        let mut matched: Vec<(String, String, Trigger, Arc<str>)> = Vec::new();
        for (key, loaded) in &self.scripts {
            if !loaded.enabled {
                continue;
            }
            let Some(script) = &loaded.script else {
                continue;
            };
            for t in &script.triggers {
                if let TriggerKind::Command(c) = &t.kind {
                    if *c == name || t.command.aliases.iter().any(|a| *a == name) {
                        matched.push((key.clone(), c.clone(), t.clone(), script.file.clone()));
                    }
                }
            }
        }
        let found = !matched.is_empty();
        let now = host.now_millis();
        let uuid = sender.player.as_ref().map(|(u, _)| *u);
        let reply = |text: &str| match uuid {
            Some(u) => host.send(u, text),
            None => host.log(text),
        };
        for (key, command, trigger, file) in matched {
            let info = &trigger.command;
            if let Some(by) = &info.executable_by {
                let allowed = by.iter().any(|b| match b {
                    ExecutableBy::Players => uuid.is_some(),
                    ExecutableBy::Operators => sender.op,
                    ExecutableBy::Console => uuid.is_none(),
                    ExecutableBy::Blocks => false,
                });
                if !allowed {
                    reply(&format!(
                        "This command can only be used by {}",
                        executable_by_text(by)
                    ));
                    continue;
                }
            }
            if let (Some(node), Some(u)) = (&info.permission, uuid) {
                if !host.has_permission(u, node) {
                    reply(info.permission_message.as_deref().unwrap_or(
                        "You don't have the required permission to use this command",
                    ));
                    continue;
                }
            }
            let cooldown_key = (key.clone(), command.clone(), uuid.unwrap_or(0));
            if let (Some(ms), Some(u)) = (info.cooldown_ms, uuid) {
                let bypass = info
                    .cooldown_bypass
                    .as_deref()
                    .is_some_and(|b| !b.is_empty() && host.has_permission(u, b));
                let waiting = self
                    .cooldowns
                    .get(&cooldown_key)
                    .is_some_and(|last| now - last < ms);
                if waiting && !bypass {
                    reply(info.cooldown_message.as_deref().unwrap_or(
                        "You are using this command too often, please try again later",
                    ));
                    continue;
                }
            }
            let cmd_args = match parse_command_args(&info.parts, rest, host) {
                Ok(args) => args,
                Err(e) => {
                    if let Some(message) = e.message {
                        reply(&message);
                    }
                    reply(&usage_line(&command, info));
                    continue;
                }
            };
            if info.cooldown_ms.is_some() && uuid.is_some() {
                self.cooldowns.insert(cooldown_key, now);
            }
            let mut locals = Variables::new();
            for a in &cmd_args {
                if let (Some(n), Some(v)) = (&a.name, &a.value) {
                    match v {
                        Value::List(items) => {
                            for (i, item) in items.iter().enumerate() {
                                locals.set(&format!("_{n}::{}", i + 1), item.clone());
                            }
                        }
                        other => locals.set(&format!("_{n}"), other.clone()),
                    }
                }
            }
            let data = Event {
                player: sender
                    .player
                    .as_ref()
                    .map(|(u, n)| Value::Player { uuid: *u, name: n.clone() }),
                cmd_args,
                ..Event::default()
            };
            self.start(host, key, Run::new(trigger.body.clone(), file, data, locals, 0));
        }
        found
    }

    /// Every command name and alias the enabled scripts answer, sorted, for the glue to register
    /// and to see whether a reload changed the command tree.
    pub fn command_registrations(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .command_infos()
            .into_iter()
            .flat_map(|(name, info)| std::iter::once(name).chain(info.aliases))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Every command name a trigger of an enabled script answers, for the glue to register.
    pub fn command_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .scripts
            .values()
            .filter(|l| l.enabled)
            .filter_map(|l| l.script.as_ref())
            .flat_map(|s| s.triggers.iter())
            .filter_map(|t| match &t.kind {
                TriggerKind::Command(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// One server tick: runs the repeating triggers that are due and the waits that end now.
    pub fn tick(&mut self, host: &dyn Host) {
        self.tick += 1;
        let now = self.tick;
        if self.settings.database.is_some() && self.vars.changes() >= self.settings.changes_until_save
        {
            if let Err(e) = self.save_variables(host) {
                host.log(&format!("[Skript] {e}"));
            }
        }
        let mut due_repeats = Vec::new();
        for r in &mut self.repeats {
            if now >= r.next {
                r.next = now + r.every;
                due_repeats.push((r.script.clone(), r.body.clone(), r.file.clone()));
            }
        }
        // One budget for everything this tick runs: a run that finds it spent waits for the next.
        let mut budget = TICK_BUDGET;
        for (key, body, file) in due_repeats {
            let run = Run::new(body, file, Event::default(), Variables::new(), 0);
            self.advance(host, key, run, &mut budget);
        }
        let (due, later): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| p.due <= now);
        self.pending = later;
        for p in due {
            self.advance(host, p.script, p.run, &mut budget);
        }
    }

    /// Starts a run from an event or a command: it goes on at once, with its own budget.
    fn start(&mut self, host: &dyn Host, script: String, run: Run) {
        let mut budget = STEP_BUDGET;
        self.advance(host, script, run, &mut budget);
    }

    fn advance(&mut self, host: &dyn Host, script: String, mut run: Run, budget: &mut u32) {
        let outcome = {
            let mut env = Env {
                vars: &mut self.vars,
                funcs: &self.funcs,
                host,
            };
            run.step(&mut env, budget)
        };
        match outcome {
            Outcome::Done => {}
            Outcome::Wait(ticks) => self.pending.push(Pending {
                due: self.tick + ticks,
                script,
                run,
            }),
            Outcome::Failed(e, line) => {
                let file = run.file.to_string();
                host.log(&format!("[Skript] {file}, line {line}: {e}"));
            }
        }
    }

    /// `/skript help|reload|enable|disable|list|info|update|restore`, as the lines the sender sees.
    pub fn skript_command(&mut self, host: &dyn Host, args: &str) -> Vec<String> {
        self.skript_command_as(host, args, None)
    }

    /// The same, for the sender with this UUID (`None`: the console). The lines of `reload`,
    /// `enable` and `disable` also go to every other online player with `skript.reloadnotify`.
    pub fn skript_command_as(
        &mut self,
        host: &dyn Host,
        args: &str,
        sender: Option<u128>,
    ) -> Vec<String> {
        let args = args.trim();
        let (sub, rest) = args
            .split_once(char::is_whitespace)
            .map_or((args, ""), |(s, r)| (s, r.trim()));
        let changes_scripts = matches!(
            sub.to_ascii_lowercase().as_str(),
            "reload" | "enable" | "disable"
        ) && !rest.is_empty();
        let lines = match sub.to_ascii_lowercase().as_str() {
            "info" => self.info(),
            "list" => self.list(),
            "reload" if !rest.is_empty() => self.reload_command(host, rest),
            "enable" if !rest.is_empty() => self.enable_command(host, rest),
            "disable" if !rest.is_empty() => self.disable_command(host, rest),
            "reload" => skript_help_for("reload", &RELOAD_ARGS),
            "enable" => skript_help_for("enable", &ENABLE_ARGS),
            "disable" => skript_help_for("disable", &ENABLE_ARGS),
            "update" => vec![format_chat(
                "<red>Updates are not available: this port is part of the server and updates with it.",
            )],
            "restore" => vec![format_chat(
                "<red>Restoring scripts is not available: the scripts are the files in the scripts folder.",
            )],
            _ => skript_help(),
        };
        if changes_scripts {
            for (uuid, _) in host.online_players() {
                if Some(uuid) != sender && host.has_permission(uuid, "skript.reloadnotify") {
                    for line in &lines {
                        host.send(uuid, line);
                    }
                }
            }
        }
        lines
    }

    /// The scripts a `reload`, `enable` or `disable` target names: one script, or the scripts in a
    /// folder (`sub` or `sub/`). `None` when neither exists.
    fn targets(&self, target: &str) -> Option<(Vec<String>, bool)> {
        if let Some(key) = self.find(target) {
            return Some((vec![key], false));
        }
        let folder = format!("{}/", target.trim().trim_end_matches('/'));
        let keys: Vec<String> = self
            .scripts
            .keys()
            .filter(|k| k.starts_with(&folder) || k.starts_with(&format!("-{folder}")))
            .cloned()
            .collect();
        (!keys.is_empty()).then_some((keys, true))
    }

    fn no_such_script(target: &str) -> Vec<String> {
        vec![format_chat(&format!(
            "<red>Can't find the script <gray>'<gold>{target}<gray>'<red> in the scripts folder!"
        ))]
    }

    /// `reload all|scripts|config|aliases|last|<script or folder>`.
    fn reload_command(&mut self, host: &dyn Host, target: &str) -> Vec<String> {
        let started = std::time::Instant::now();
        let lower = target.to_ascii_lowercase();
        let (label, keys): (String, Vec<String>) = match lower.as_str() {
            "config" | "aliases" => {
                let what = if lower == "config" { "main config" } else { "aliases" };
                return vec![
                    format_chat(&format!("Reloading <gold>the {what}<reset>...")),
                    format_chat(
                        "<yellow>This port reads its settings from the mod's common config; reload that with the server's config reload.",
                    ),
                ];
            }
            "all" | "scripts" => ("all scripts".to_owned(), self.scripts.keys().cloned().collect()),
            "last" => match self.last_reloaded.clone() {
                None => {
                    return vec![format_chat(
                        "<red>No scripts have been reloaded yet. You must reload a script first.",
                    )]
                }
                Some(t) => return self.reload_command(host, &t),
            },
            _ => match self.targets(target) {
                None => return Self::no_such_script(target),
                Some((keys, true)) => (format!("all scripts in <gold>{target}<reset>"), keys),
                Some((keys, false)) => {
                    let key = keys[0].clone();
                    if self.scripts.get(&key).is_some_and(|l| !l.enabled) {
                        let name = key.trim_start_matches('-');
                        return vec![format_chat(&format!(
                            "<gold>{name}<reset> is currently disabled. Use <gray>/<gold>skript <cyan>enable <red>{name}<reset> to enable it."
                        ))];
                    }
                    (format!("<gold>{key}<reset>"), keys)
                }
            },
        };
        if lower != "all" && lower != "scripts" {
            self.last_reloaded = Some(target.to_owned());
        }
        let mut lines = vec![format_chat(&format!("Reloading {label}..."))];
        let mut errors = 0usize;
        for key in &keys {
            if self.scripts.get(key).is_some_and(|l| !l.enabled) {
                continue;
            }
            let _ = self.reload(host, key);
            if let Some(l) = self.scripts.get(key) {
                errors += l.errors.len();
                lines.extend(
                    l.errors
                        .iter()
                        .map(|e| format_chat(&format!("<light red>    {e}"))),
                );
            }
        }
        let ms = started.elapsed().as_millis();
        lines.push(if errors == 0 {
            format_chat(&format!(
                "<lime>Successfully reloaded {label}<lime>. <gray>(<gold>{ms}ms<gray>)"
            ))
        } else {
            format_chat(&format!(
                "<light red>Encountered <gold>{errors} <light red>error{} while reloading {label}<light red>! <gray>(<gold>{ms}ms<gray>)",
                if errors == 1 { "" } else { "s" }
            ))
        });
        lines
    }

    /// `enable all|<script or folder>`.
    fn enable_command(&mut self, host: &dyn Host, target: &str) -> Vec<String> {
        let all = target.eq_ignore_ascii_case("all");
        let (keys, single) = if all {
            (self.scripts.keys().cloned().collect(), false)
        } else {
            match self.targets(target) {
                None => return Self::no_such_script(target),
                Some((keys, folder)) => (keys, !folder),
            }
        };
        let mut lines = Vec::new();
        let mut enabled = 0usize;
        for key in keys {
            if self.scripts.get(&key).is_some_and(|l| l.enabled) {
                if single {
                    lines.push(format_chat(&format!(
                        "<gold>{key}<reset> is already enabled! Use <gray>/<gold>skript <cyan>reload <red>{key}<reset> to reload it if it was changed."
                    )));
                }
                continue;
            }
            match self.enable(host, &key) {
                Ok(()) => enabled += 1,
                Err(e) => lines.push(format_chat(&format!("<light red>{e}"))),
            }
            if let Some(l) = self.scripts.get(&key) {
                lines.extend(
                    l.errors
                        .iter()
                        .map(|e| format_chat(&format!("<light red>    {e}"))),
                );
            }
        }
        if enabled > 0 {
            lines.push(format_chat(&format!(
                "Successfully enabled & parsed <gold>{enabled}<reset> previously disabled script{}.",
                if enabled == 1 { "" } else { "s" }
            )));
        }
        lines
    }

    /// `disable all|<script or folder>`.
    fn disable_command(&mut self, host: &dyn Host, target: &str) -> Vec<String> {
        let keys: Vec<String> = if target.eq_ignore_ascii_case("all") {
            self.scripts.keys().cloned().collect()
        } else {
            match self.targets(target) {
                None => return Self::no_such_script(target),
                Some((keys, _)) => keys,
            }
        };
        let mut disabled = 0usize;
        for key in keys {
            if self.scripts.get(&key).is_some_and(|l| l.enabled) && self.disable(host, &key).is_ok() {
                disabled += 1;
            }
        }
        vec![format_chat(&if target.eq_ignore_ascii_case("all") {
            "Successfully disabled all scripts.".to_owned()
        } else {
            format!("Successfully disabled <gold>{disabled}<reset> script(s) in <gold>{target}<reset>.")
        })]
    }

    /// The words `/skript <args>` can complete: the sub-commands, then `all`, `scripts`, ... and
    /// the script names after `reload`, `enable` and `disable`. `args` is the text after the command
    /// name, ending in a space when the next word has not been started.
    pub fn completions(&self, args: &str) -> Vec<String> {
        let words: Vec<&str> = args.split_whitespace().collect();
        let starting = args.ends_with(char::is_whitespace) || words.is_empty();
        let (index, prefix) = if starting {
            (words.len(), "")
        } else {
            (words.len() - 1, words[words.len() - 1])
        };
        let mut pool: Vec<String> = match index {
            0 => SUB_COMMANDS.iter().map(|s| (*s).to_owned()).collect(),
            1 => {
                let mut pool: Vec<String> = match words[0].to_ascii_lowercase().as_str() {
                    "reload" => RELOAD_ARGS.iter().map(|s| (*s).to_owned()).collect(),
                    "enable" | "disable" => vec!["all".to_owned()],
                    _ => return Vec::new(),
                };
                pool.extend(self.scripts.keys().cloned());
                pool
            }
            _ => return Vec::new(),
        };
        let lower = prefix.to_ascii_lowercase();
        pool.retain(|w| w.to_ascii_lowercase().starts_with(&lower));
        pool
    }

    /// Counts for `/skript info`.
    pub fn info(&self) -> Vec<String> {
        let enabled = self.scripts.values().filter(|l| l.enabled).count();
        let broken = self.scripts.values().filter(|l| !l.errors.is_empty()).count();
        let (effects, conditions, expressions, events) = self.registry.counts();
        vec![
            format!(
                "Scripts: {} loaded, {enabled} enabled, {broken} with errors",
                self.scripts.len()
            ),
            format!(
                "Syntax: {effects} effects, {conditions} conditions, {expressions} expressions, {events} events"
            ),
            format!(
                "Functions: {}, waits: {}, variables: {}",
                self.funcs.len(),
                self.pending.len(),
                self.vars.len()
            ),
        ]
    }

    /// Each script with its state, for `/skript list`.
    pub fn list(&self) -> Vec<String> {
        if self.scripts.is_empty() {
            return vec!["No scripts.".to_owned()];
        }
        self.scripts
            .iter()
            .map(|(key, l)| {
                let state = match (&l.script, l.enabled) {
                    // disabled at load: not parsed yet
                    (None, false) if l.errors.is_empty() => "disabled".to_owned(),
                    (None, _) => format!("errors ({})", l.errors.len()),
                    (Some(_), true) if !l.errors.is_empty() => {
                        format!("enabled, errors ({})", l.errors.len())
                    }
                    (Some(_), true) => "enabled".to_owned(),
                    (Some(_), false) => "disabled".to_owned(),
                };
                format!("{key}: {state}")
            })
            .collect()
    }

    pub fn script_names(&self) -> Vec<String> {
        self.scripts.keys().cloned().collect()
    }

    /// The parse errors of a script, as text.
    pub fn errors_of(&self, key: &str) -> Vec<String> {
        self.scripts
            .get(key)
            .map(|l| l.errors.iter().map(ScriptError::to_string).collect())
            .unwrap_or_default()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// The sub-commands of `/skript`, as the original completes them.
const SUB_COMMANDS: [&str; 8] = [
    "help", "reload", "enable", "disable", "list", "info", "update", "restore",
];
const RELOAD_ARGS: [&str; 5] = ["all", "scripts", "config", "aliases", "last"];
const ENABLE_ARGS: [&str; 1] = ["all"];

/// `Usage: /skript reload ...` and what each argument does (the original's `sendHelp`).
fn skript_help_for(sub: &str, args: &[&str]) -> Vec<String> {
    let mut lines = vec![format_chat(&format!(
        "Usage: <gold>/skript <yellow>{sub} <red>..."
    ))];
    for arg in args {
        let what = match (sub, *arg) {
            ("reload", "all") => "Reloads the config, all aliases configs and all scripts",
            ("reload", "scripts") => "Reloads all scripts",
            ("reload", "config") => "Reloads the main config",
            ("reload", "aliases") => "Reloads the aliases configs",
            ("reload", "last") => "Reloads the last script that was reloaded",
            ("enable", _) => "Enables all scripts",
            _ => "Disables all scripts",
        };
        lines.push(format_chat(&format!("   <yellow>{arg} <gray>- <white>{what}")));
    }
    let script = match sub {
        "reload" => "Reloads a specific script or a folder of scripts",
        "enable" => "Enables a specific script or a folder of scripts",
        _ => "Disables a specific script or a folder of scripts",
    };
    lines.push(format_chat(&format!("   <yellow><script> <gray>- <white>{script}")));
    lines
}

/// `/skript` and `/skript help`.
fn skript_help() -> Vec<String> {
    [
        "<gold>Skript's main command",
        "   <yellow>help <gray>- <white>Prints this help message. Use '/skript reload/enable/disable/update' to get more info",
        "   <yellow>reload <gray>- <white>Reloads a specific script, all scripts, the config, or everything",
        "   <yellow>enable <gray>- <white>Enables all scripts or a specific one",
        "   <yellow>disable <gray>- <white>Disables all scripts or a specific one",
        "   <yellow>update <gray>- <white>Check for updates or read the changelog",
        "   <yellow>restore <gray>- <white>Stores loaded scripts back as files",
        "   <yellow>list <gray>- <white>Lists all enabled and disabled scripts",
        "   <yellow>info <gray>- <white>Prints the number of scripts and syntax elements",
    ]
    .iter()
    .map(|l| format_chat(l))
    .collect()
}

/// A command line that does not fit the header. `message` says what is wrong with an argument.
#[derive(Debug, PartialEq, Eq)]
pub struct ArgError {
    pub message: Option<String>,
}

/// The next argument of a line: a quoted phrase or a word, and what follows it.
fn next_token(rest: &str) -> (String, &str) {
    let r = rest.trim_start();
    if let Some(quoted) = r.strip_prefix('"') {
        if let Some(end) = quoted.find('"') {
            return (quoted[..end].to_owned(), quoted[end + 1..].trim_start());
        }
    }
    match r.find(char::is_whitespace) {
        Some(i) => (r[..i].to_owned(), r[i..].trim_start()),
        None => (r.to_owned(), ""),
    }
}

/// The value one argument takes from `token`; `None` when it is not valid for the type.
fn arg_value(spec: &ArgSpec, token: &str, host: &dyn Host) -> Option<Value> {
    if spec.plural {
        let mut items = Vec::new();
        for piece in token
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("and") && !p.eq_ignore_ascii_case("or"))
        {
            items.push(classes::parse_value(spec.class, piece, host)?);
        }
        Some(Value::List(items))
    } else {
        classes::parse_value(spec.class, token, host)
    }
}

/// Matches the text after a command's name with the header's words and arguments, as Skript's
/// command parsing does: words and quoted phrases for the arguments, the last text argument takes
/// the rest of the line, an optional argument left out takes its default.
pub fn parse_command_args(
    parts: &[HeaderPart],
    rest: &str,
    host: &dyn Host,
) -> Result<Vec<CmdArg>, ArgError> {
    let mut rest = rest.trim();
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        match part {
            HeaderPart::Word { text, optional } => {
                let (token, after) = next_token(rest);
                if !rest.is_empty() && token.eq_ignore_ascii_case(text) {
                    rest = after;
                } else if !optional {
                    return Err(ArgError { message: None });
                }
            }
            HeaderPart::Arg(spec) => {
                if rest.is_empty() {
                    if !spec.optional {
                        return Err(ArgError { message: None });
                    }
                    out.push(CmdArg {
                        name: spec.name.clone(),
                        class: spec.class,
                        value: spec
                            .default
                            .as_ref()
                            .and_then(|d| arg_value(spec, d, host)),
                    });
                    continue;
                }
                let greedy = i + 1 == parts.len() && (spec.class == "text" || spec.plural);
                let (token, after) = if greedy {
                    (rest.to_owned(), "")
                } else {
                    next_token(rest)
                };
                let Some(value) = arg_value(spec, &token, host) else {
                    return Err(ArgError {
                        message: Some(format!(
                            "\u{a7}4'{token}' is not a valid {}.",
                            spec.class
                        )),
                    });
                };
                out.push(CmdArg {
                    name: spec.name.clone(),
                    class: spec.class,
                    value: Some(value),
                });
                rest = after;
            }
        }
    }
    if rest.is_empty() {
        Ok(out)
    } else {
        Err(ArgError { message: None })
    }
}

/// `/name <text> [<number>]`, or the script's own `usage:`.
fn usage_line(name: &str, info: &CommandInfo) -> String {
    match &info.usage {
        Some(u) => u.clone(),
        None if info.header.is_empty() => format!("/{name}"),
        None => format!("/{name} {}", info.header),
    }
}

/// `players`, `the console`: for the line a refused sender gets.
fn executable_by_text(by: &[ExecutableBy]) -> String {
    let names: Vec<&str> = by
        .iter()
        .map(|b| match b {
            ExecutableBy::Players => "players",
            ExecutableBy::Operators => "operators",
            ExecutableBy::Console => "the console",
            ExecutableBy::Blocks => "blocks",
        })
        .collect();
    names.join(" and ")
}

/// The permission nodes a script's text names: the `permission:` line of a command and a
/// string literal right after `has permission` / `have permission`. Nodes with an expression
/// in them (`%`) or with spaces cannot be known before the script runs and are left out.
pub fn permission_nodes_in(source: &str) -> Vec<String> {
    let literal = Regex::new(r#"(?i)\bpermissions?\s+"([^"%]+)""#).expect("a valid regex");
    let mut out = Vec::new();
    for raw in source.lines() {
        let line = crate::horizon_skript_core::script::strip_comment(raw);
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("permission") {
                let node = value.trim();
                if !node.is_empty() && !node.contains(char::is_whitespace) && !node.contains('%') {
                    out.push(node.to_owned());
                }
                continue;
            }
        }
        for cap in literal.captures_iter(line) {
            let node = cap[1].trim();
            if !node.is_empty() && !node.contains(char::is_whitespace) {
                out.push(node.to_owned());
            }
        }
    }
    out
}

/// Every permission node the `.sk` files below `dir` name (disabled files too), sorted and
/// without duplicates. An unreadable folder or file gives nothing for it.
pub fn scan_permission_nodes(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if collect(dir, "", &mut files).is_err() {
        return Vec::new();
    }
    let mut nodes = std::collections::BTreeSet::new();
    for rel in files {
        if let Ok(text) = fs::read_to_string(dir.join(&rel)) {
            nodes.extend(permission_nodes_in(&text));
        }
    }
    nodes.into_iter().collect()
}

/// Collects the `.sk` files below `base`, as paths relative to it with `/` separators.
fn collect(dir: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect(&entry.path(), &rel, out)?;
        } else if name.ends_with(".sk") {
            out.push(rel);
        }
    }
    Ok(())
}
