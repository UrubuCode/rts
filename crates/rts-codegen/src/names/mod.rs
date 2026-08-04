//! Identifiers, and what the machine keys layouts by.
//!
//! A program is full of the same handful of names repeated. Comparing them as
//! text is the obvious thing and the wrong one: every comparison walks bytes, and
//! the answer is needed constantly — resolving a variable, matching a property,
//! deciding whether two objects have the same shape.
//!
//! So a name is interned to a number once, and everything afterwards compares
//! numbers. That number is also exactly what the machine layer wants: it keys
//! layouts by an opaque key and never asks what one is called, which is the same
//! shape of answer from the other side of the boundary.

use std::collections::HashMap;

use rts_cranelift::shape::{Key, KeyRegistry};

/// An identifier, interned.
///
/// Two names are the same identifier when their numbers match. Nothing else is
/// a comparison of identifiers, and in particular comparing the text is not:
/// text comparison is what interning exists to stop happening in a loop.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Name(u32);

impl Name {
    /// The number this name is.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Every identifier a program uses.
///
/// Holds the text as well as the number, which the machine layer deliberately
/// does not. That asymmetry is the boundary working: a machine has no use for
/// what a property is called, and a diagnostic is useless without it.
#[derive(Default)]
pub struct Names {
    text: Vec<String>,
    interned: HashMap<String, Name>,
    /// What the machine calls each of these, once something has needed one.
    ///
    /// Filled on demand rather than up front: most identifiers in a program are
    /// variables, which never become a property key, and minting one for each
    /// would spend the machine's key space on names it will never see.
    keys: HashMap<Name, Key>,
}

impl Names {
    /// A program that has named nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The identifier some text is, interning it the first time.
    pub fn intern(&mut self, text: &str) -> Name {
        if let Some(&existing) = self.interned.get(text) {
            return existing;
        }
        let name = Name(self.text.len() as u32);
        self.text.push(text.to_owned());
        self.interned.insert(text.to_owned(), name);
        name
    }

    /// What an identifier is called.
    ///
    /// For diagnostics, and for nothing else: a decision that reads the text of a
    /// name is a decision that could have compared numbers and did not.
    pub fn text(&self, name: Name) -> &str {
        &self.text[name.index()]
    }

    /// What the machine calls this identifier, minting one the first time.
    ///
    /// Only names used as property keys ever get one, which is why this takes the
    /// registry rather than holding it: a program that names a thousand variables
    /// and two properties should spend two keys.
    pub fn key(&mut self, name: Name, keys: &mut KeyRegistry) -> Key {
        if let Some(&existing) = self.keys.get(&name) {
            return existing;
        }
        let key = keys.declare_one();
        self.keys.insert(name, key);
        key
    }

    /// The text of every name that became a property key, in key order.
    ///
    /// # Why the host needs this and the count was not enough
    ///
    /// A key crosses to the runtime as a number, and for a property the
    /// *compiler* resolved that is sufficient: both sides hold the same number
    /// and neither needs the text. Seeding the runtime with the count was
    /// therefore enough, and was what the host did.
    ///
    /// A **computed** key breaks that. `o[k]` hands the runtime a string while
    /// running, and the runtime has to arrive at the number the compiler
    /// already chose for it — which it cannot do from a count. It has to have
    /// been told which text is which number.
    ///
    /// Found by running `o.n = 7; o["n"]`, which read a different property: the
    /// runtime interned `"n"` afresh, past the seeded range, and answered a key
    /// the compiler had never used.
    pub fn keyed_texts(&self) -> Vec<&str> {
        let mut ordered: Vec<(Key, &str)> = self
            .keys
            .iter()
            .map(|(name, key)| (*key, self.text[name.index()].as_str()))
            .collect();
        // Sorted by key, because the runtime interns them in this order and
        // interning is what mints the numbers. A different order is a different
        // mapping, and a `HashMap` has no order at all.
        ordered.sort_by_key(|(key, _)| key.index());
        ordered.into_iter().map(|(_, text)| text).collect()
    }

    /// Whether this identifier has ever been used as a property key.
    pub fn has_key(&self, name: Name) -> bool {
        self.keys.contains_key(&name)
    }

    /// How many identifiers exist.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether none do.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_is_the_same_identifier() {
        let mut names = Names::new();
        assert_eq!(names.intern("value"), names.intern("value"));
        assert_ne!(names.intern("value"), names.intern("other"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn an_identifier_keeps_its_text_for_diagnostics() {
        let mut names = Names::new();
        let name = names.intern("length");
        assert_eq!(names.text(name), "length");
    }

    #[test]
    fn only_names_used_as_properties_cost_the_machine_a_key() {
        let mut names = Names::new();
        let mut keys = KeyRegistry::new();

        let variable = names.intern("counter");
        let property = names.intern("length");
        let key = names.key(property, &mut keys);

        assert!(!names.has_key(variable), "a variable is not a property");
        assert_eq!(
            names.key(property, &mut keys),
            key,
            "and asking twice does not mint a second"
        );
        assert_eq!(keys.len(), 1);
    }
}
