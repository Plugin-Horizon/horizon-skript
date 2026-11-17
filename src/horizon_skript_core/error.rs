//! Errors of the language core: parse errors with file and line, and run-time errors.

use std::fmt;

/// A problem in a script file, reported the way Skript reports it:
/// `name.sk, line 3: Can't understand this effect: "text"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub file: String,
    pub line: usize,
    pub message: String,
    pub text: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.text.is_empty() {
            write!(f, "{}, line {}: {}", self.file, self.line, self.message)
        } else {
            write!(
                f,
                "{}, line {}: {}: \"{}\"",
                self.file, self.line, self.message, self.text
            )
        }
    }
}

/// A run-time failure of a trigger or function. The interpreter adds the line it happened on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError {
    pub message: String,
}

impl RunError {
    pub fn new(message: impl Into<String>) -> Self {
        RunError {
            message: message.into(),
        }
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RunError {}
