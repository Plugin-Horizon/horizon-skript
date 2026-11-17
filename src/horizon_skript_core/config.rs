//! The keys of `plugins/Skript/config.sk` that the port reads. The file is Skript's own format,
//! not YAML: tab indentation, `key: value` lines, empty values open a section and `#` starts a
//! comment. It is read with Skript's rules here; the vendored YAML reader is for the other ports.

/// One line of the config with the lines indented under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Node {
    pub key: String,
    pub value: String,
    pub children: Vec<Node>,
}

impl Node {
    /// The first child with this key (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|c| c.key.eq_ignore_ascii_case(key))
    }
}

/// Reads the config text into a tree. The root has no key; its children are the top-level lines.
pub fn parse(text: &str) -> Node {
    let mut root = Node::default();
    // The path of open sections: indentation level and the child index at each level.
    let mut path: Vec<usize> = Vec::new();
    for raw in text.lines() {
        let content = strip_comment(raw);
        if content.trim().is_empty() {
            continue;
        }
        let level = content.chars().take_while(|c| *c == '\t').count();
        let body = content.trim();
        let (key, value) = match body.split_once(':') {
            Some((k, v)) => (k.trim().to_owned(), v.trim().to_owned()),
            None => continue,
        };
        path.truncate(level);
        let mut parent = &mut root;
        for &i in &path {
            parent = &mut parent.children[i];
        }
        parent.children.push(Node {
            key,
            value,
            children: Vec::new(),
        });
        path.push(parent.children.len() - 1);
    }
    root
}

/// The line without its comment: a `#` at the start of the line or after a space or tab, outside
/// quotes. A doubled `##` is a literal `#` in Skript's config, so it does not start a comment.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quoted = false;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => quoted = !quoted,
            b'#' if !quoted => {
                let doubled = bytes.get(i + 1) == Some(&b'#') || (i > 0 && bytes[i - 1] == b'#');
                let starts = i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t';
                if starts && !doubled {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_values_are_read() {
        let root = parse(
            "# a comment\n\
             number accuracy: 2\n\
             databases:\n\
             \tdefault:\n\
             \t\ttype: CSV\n\
             \t\tfile: ./plugins/Skript/variables.csv\n\
             \t\tpattern: .*\n\
             \t# another comment\n\
             variable changes until save: 1000 # trailing\n",
        );
        assert_eq!(root.get("number accuracy").unwrap().value, "2");
        let db = root.get("databases").unwrap().get("default").unwrap();
        assert_eq!(db.get("type").unwrap().value, "CSV");
        assert_eq!(db.get("file").unwrap().value, "./plugins/Skript/variables.csv");
        assert_eq!(db.get("pattern").unwrap().value, ".*");
        assert_eq!(
            root.get("variable changes until save").unwrap().value,
            "1000"
        );
    }

    #[test]
    fn double_hash_is_not_a_comment() {
        assert_eq!(strip_comment("key: a##b"), "key: a##b");
        assert_eq!(strip_comment("key: value # note"), "key: value ");
        assert_eq!(strip_comment("# only a comment"), "");
    }
}
