//! More enum types: one word of a fixed set, kept as lower-case text like the enum classes in
//! `classes.rs`, which read this list too. A type here has no value variant of its own, so a
//! value of it is text. Only the names whose values are single words belong here.

/// Enum classes beyond `classes::ENUM_CLASSES`. An empty word list takes any word.
pub const MORE_ENUM_CLASSES: &[(&str, &[&str])] = &[
    ("damagesource", &[]),
    ("gamerulevalue", &[]),
];
