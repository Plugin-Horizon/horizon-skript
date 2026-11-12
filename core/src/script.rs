//! Reads a `.sk` file into a tree of lines: comments and blank lines removed, indentation turned
//! into levels, sections (lines ending in `:`) holding their indented lines, and `options:`
//! substituted into `{@name}`.
//!
//! Indentation: the first indented line fixes the unit (a tab, or some number of spaces); every
//! level is one unit, and a file may not mix tabs and spaces.

use crate::error::ScriptError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// 1-based line number in the file.
    pub number: usize,
    pub level: usize,
    /// The code of the line: no comment, no indentation, no trailing spaces.
    pub text: String,
    /// The line as written, for error messages.
    pub raw: String,
}

/// A line and the lines indented under it (empty for a simple statement).
#[derive(Clone, Debug)]
pub struct Item {
    pub line: Line,
    pub children: Vec<Item>,
}

impl Item {
    /// Whether the line opens a section (`if x > 1:`).
    pub fn is_section(&self) -> bool {
        self.line.text.ends_with(':')
    }

    /// The section's header without its colon, or the statement itself.
    pub fn header(&self) -> &str {
        let t = self.line.text.as_str();
        t.strip_suffix(':').unwrap_or(t).trim_end()
    }
}

/// The code before a `#` that is not inside a string, without surrounding spaces.
pub fn strip_comment(line: &str) -> &str {
    let mut quote = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quote = false;
            }
        } else if c == '"' {
            quote = true;
        } else if c == '#' {
            return line[..i].trim();
        }
    }
    line.trim()
}

/// Reads a file. Problems are added to `errors`; lines that cannot be read are left out.
pub fn read(file: &str, source: &str, errors: &mut Vec<ScriptError>) -> Vec<Item> {
    let mut lines = Vec::new();
    let mut unit: Option<(char, usize)> = None;
    for (i, raw) in source.lines().enumerate() {
        let raw = raw.trim_end_matches('\r');
        let code = strip_comment(raw);
        if code.is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let indent: &str = &raw[..raw.len() - raw.trim_start().len()];
        let mut fail = |message: &str| {
            errors.push(ScriptError {
                file: file.to_owned(),
                line: i + 1,
                message: message.to_owned(),
                text: raw.trim().to_owned(),
            })
        };
        let level = match indent.chars().next() {
            None => Some(0),
            Some(kind) => {
                if indent.chars().any(|c| c != kind) {
                    fail("mixed tabs and spaces in the indentation");
                    None
                } else {
                    match unit {
                        None => {
                            unit = Some((kind, indent.len()));
                            Some(1)
                        }
                        Some((k, _)) if k != kind => {
                            fail(
                                "the indentation uses other characters than the first indented line",
                            );
                            None
                        }
                        Some((_, len)) if indent.len() % len != 0 => {
                            fail("inconsistent indentation");
                            None
                        }
                        Some((_, len)) => Some(indent.len() / len),
                    }
                }
            }
        };
        if let Some(level) = level {
            lines.push(Line {
                number: i + 1,
                level,
                text: code.to_owned(),
                raw: raw.trim().to_owned(),
            });
        }
    }
    let mut pos = 0;
    let mut items = build(file, &lines, &mut pos, 0, errors);
    apply_options(&mut items);
    items
}

fn build(
    file: &str,
    lines: &[Line],
    pos: &mut usize,
    level: usize,
    errors: &mut Vec<ScriptError>,
) -> Vec<Item> {
    let mut out = Vec::new();
    while let Some(line) = lines.get(*pos) {
        if line.level < level {
            break;
        }
        if line.level > level {
            errors.push(ScriptError {
                file: file.to_owned(),
                line: line.number,
                message: "unexpected indentation".to_owned(),
                text: line.raw.clone(),
            });
            *pos += 1;
            continue;
        }
        *pos += 1;
        let children = if line.text.ends_with(':') {
            build(file, lines, pos, level + 1, errors)
        } else {
            Vec::new()
        };
        out.push(Item {
            line: line.clone(),
            children,
        });
    }
    out
}

/// Replaces `{@name}` with the value of `name:` in a top-level `options:` section.
fn apply_options(items: &mut [Item]) {
    let mut options: Vec<(String, String)> = Vec::new();
    for item in items.iter() {
        if item.is_section() && item.header().eq_ignore_ascii_case("options") {
            for child in &item.children {
                if let Some((k, v)) = child.line.text.split_once(':') {
                    options.push((k.trim().to_owned(), v.trim().to_owned()));
                }
            }
        }
    }
    if options.is_empty() {
        return;
    }
    fn walk(items: &mut [Item], options: &[(String, String)]) {
        for item in items {
            for (name, value) in options {
                let key = format!("{{@{name}}}");
                if item.line.text.contains(&key) {
                    item.line.text = item.line.text.replace(&key, value);
                }
            }
            walk(&mut item.children, options);
        }
    }
    walk(items, &options);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(src: &str) -> (Vec<Item>, Vec<ScriptError>) {
        let mut errors = Vec::new();
        let items = read("t.sk", src, &mut errors);
        (items, errors)
    }

    #[test]
    fn comments_and_blank_lines_are_dropped() {
        let (items, errors) =
            items("# top\n\non join: # trailing\n\t# inside\n\tbroadcast \"a # not a comment\"\n");
        assert!(errors.is_empty());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].header(), "on join");
        assert_eq!(items[0].children.len(), 1);
        assert_eq!(
            items[0].children[0].line.text,
            "broadcast \"a # not a comment\""
        );
        assert_eq!(items[0].children[0].line.number, 5);
    }

    #[test]
    fn indentation_builds_sections() {
        let src = "on load:\n    if 1 is 1:\n        stop\n    broadcast \"x\"\n";
        let (items, errors) = items(src);
        assert!(errors.is_empty(), "{errors:?}");
        let kids = &items[0].children;
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].children.len(), 1);
        assert_eq!(kids[0].children[0].line.level, 2);
    }

    #[test]
    fn bad_indentation_is_reported_with_its_line() {
        let (_, errors) = items("on load:\n  a\n     b\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 3);
        let (_, mixed) = items("on load:\n\tx\n  y\n");
        assert!(mixed.iter().any(|e| e.line == 3));
        let (_, unexpected) = items("on load:\n    x\n        y\n");
        assert!(
            unexpected
                .iter()
                .any(|e| e.message == "unexpected indentation")
        );
    }

    #[test]
    fn options_are_substituted() {
        let (items, _) = items("options:\n    greeting: Hello\nbroadcast \"{@greeting} all\"\n");
        assert_eq!(items[1].line.text, "broadcast \"Hello all\"");
    }
}
