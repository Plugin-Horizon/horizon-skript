//! The script manager: finds the `.sk` files, keeps what they parsed to, starts triggers on
//! events and commands, runs the waits and repeats on server ticks, and answers `/skript`.
//!
//! A file whose name starts with `-` is loaded disabled (Skript's convention); `/skript enable`
//! and `/skript disable` switch scripts at run time without touching the files.

use crate::config;
use crate::error::ScriptError;
use crate::host::Host;
use crate::parse::{self, Body, CommandInfo, Script, TriggerKind};
use crate::registry::Registry;
use crate::run::{Env, Event, Functions, Outcome, Run, STEP_BUDGET};
use crate::value::{self, Value};
use crate::vars::{self, RawRow};
use regex::Regex;

/// Statements all the runs of one server tick may execute together.
const TICK_BUDGET: u32 = 100_000;
use crate::vars::Variables;
use std::collections::BTreeMap;
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
        match parse::parse_script(&self.registry, host, key, text) {
            Ok(script) => {
                self.scripts.insert(
                    key.to_owned(),
                    Loaded {
                        script: Some(Arc::new(script)),
                        enabled,
                        errors: Vec::new(),
                        path: None,
                    },
                );
                self.rebuild();
                if enabled {
                    self.fire_for(host, key, "load", Event::default());
                }
                Ok(())
            }
            Err(errors) => {
                self.scripts.insert(
                    key.to_owned(),
                    Loaded {
                        script: None,
                        enabled,
                        errors: errors.clone(),
                        path: None,
                    },
                );
                Err(errors)
            }
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
        match parse::parse_script(&self.registry, host, key, text) {
            Ok(script) => {
                self.scripts.insert(
                    key.to_owned(),
                    Loaded {
                        script: Some(Arc::new(script)),
                        enabled,
                        errors: Vec::new(),
                        path,
                    },
                );
                report.push(format!(
                    "[Skript] loaded {key}{}",
                    if enabled { "" } else { " (disabled)" }
                ));
            }
            Err(errors) => {
                for e in &errors {
                    report.push(format!("[Skript] {e}"));
                }
                report.push(format!(
                    "[Skript] {key} was not loaded because of {} error(s)",
                    errors.len()
                ));
                self.scripts.insert(
                    key.to_owned(),
                    Loaded {
                        script: None,
                        enabled,
                        errors,
                        path,
                    },
                );
            }
        }
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
        if loaded.script.is_none() && loaded.errors.is_empty() {
            // Disabled at load: parsed only now.
            let text = loaded.path.as_ref().and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
            match parse::parse_script(&self.registry, host, &key, &text) {
                Ok(script) => loaded.script = Some(Arc::new(script)),
                Err(errors) => {
                    let n = errors.len();
                    loaded.errors = errors;
                    return Err(format!("{key} has {n} error(s) and cannot be enabled."));
                }
            }
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

    /// `/skript info|list|reload [name|all]|enable <name>|disable <name>`.
    pub fn skript_command(&mut self, host: &dyn Host, args: &str) -> Vec<String> {
        let args = args.trim();
        let (sub, rest) = args
            .split_once(' ')
            .map_or((args, ""), |(s, r)| (s, r.trim()));
        match sub {
            "" | "info" => self.info(),
            "list" => self.list(),
            "reload" => self.reload(host, if rest.is_empty() { "all" } else { rest }),
            "enable" | "disable" => {
                if rest.is_empty() {
                    return vec![format!("Usage: /skript {sub} <script>")];
                }
                let result = if sub == "enable" {
                    self.enable(host, rest)
                } else {
                    self.disable(host, rest)
                };
                match result {
                    Ok(()) => vec![format!("{rest}: {sub}d")],
                    Err(e) => vec![e],
                }
            }
            _ => vec![
                "Usage: /skript info|list|reload [script|all]|enable <script>|disable <script>"
                    .to_owned(),
            ],
        }
    }

    /// Counts for `/skript info`.
    pub fn info(&self) -> Vec<String> {
        let enabled = self.scripts.values().filter(|l| l.enabled).count();
        let broken = self.scripts.values().filter(|l| l.script.is_none() && !l.errors.is_empty()).count();
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
