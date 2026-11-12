//! Variable storage. `{name}`, `{name::sub}` and list variables are all keys of one ordered map:
//! a list is the set of keys below its name, `name::1`, `name::2`, ...

use crate::value::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Variables {
    map: BTreeMap<String, Value>,
}

impl Variables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.map.get(name)
    }

    /// Sets one variable. `Null` removes it, as Skript does for `set {x} to nothing`.
    pub fn set(&mut self, name: &str, value: Value) {
        if value.is_null() {
            self.map.remove(name);
        } else {
            self.map.insert(name.to_owned(), value);
        }
    }

    /// Removes a variable and everything below it (`{x}` takes `{x::1}` with it).
    /// Returns how many values went.
    pub fn delete(&mut self, name: &str) -> usize {
        let below = format!("{name}::");
        let keys: Vec<String> = self
            .map
            .keys()
            .filter(|k| k.as_str() == name || k.starts_with(&below))
            .cloned()
            .collect();
        for k in &keys {
            self.map.remove(k);
        }
        keys.len()
    }

    /// Removes the values below a name, keeping the name itself. Returns how many went.
    pub fn delete_children(&mut self, base: &str) -> usize {
        let below = format!("{base}::");
        let keys: Vec<String> = self
            .map
            .keys()
            .filter(|k| k.starts_with(&below))
            .cloned()
            .collect();
        for k in &keys {
            self.map.remove(k);
        }
        keys.len()
    }

    /// Removes the first value below a list name that equals `value`. Returns whether one went.
    pub fn remove_matching(&mut self, base: &str, value: &Value) -> bool {
        let below = format!("{base}::");
        let key = self
            .map
            .iter()
            .find(|(k, v)| k.starts_with(&below) && *v == value)
            .map(|(k, _)| k.clone());
        match key {
            Some(k) => {
                self.map.remove(&k);
                true
            }
            None => false,
        }
    }

    /// Values below a list name (`{x::*}` is `values_below("x")`), in key order.
    pub fn values_below(&self, base: &str) -> Vec<Value> {
        let below = format!("{base}::");
        self.map
            .range(below.clone()..)
            .take_while(|(k, _)| k.starts_with(&below))
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// The next free integer index of a list (`1` for an empty list).
    pub fn next_index(&self, base: &str) -> i64 {
        let below = format!("{base}::");
        let max = self
            .map
            .range(below.clone()..)
            .take_while(|(k, _)| k.starts_with(&below))
            .filter_map(|(k, _)| k[below.len()..].parse::<i64>().ok())
            .max()
            .unwrap_or(0);
        max + 1
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.map.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_keys_below_a_name() {
        let mut v = Variables::new();
        v.set("list::1", Value::Int(10));
        v.set("list::2", Value::Int(20));
        v.set("listy", Value::Int(99));
        assert_eq!(v.values_below("list"), vec![Value::Int(10), Value::Int(20)]);
        assert_eq!(v.next_index("list"), 3);
        assert_eq!(v.next_index("none"), 1);
    }

    #[test]
    fn delete_takes_children_but_not_siblings() {
        let mut v = Variables::new();
        v.set("a", Value::Int(1));
        v.set("a::b", Value::Int(2));
        v.set("ab", Value::Int(3));
        assert_eq!(v.delete("a"), 2);
        assert_eq!(v.get("ab"), Some(&Value::Int(3)));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn null_removes() {
        let mut v = Variables::new();
        v.set("x", Value::Int(1));
        v.set("x", Value::Null);
        assert!(v.is_empty());
    }
}
