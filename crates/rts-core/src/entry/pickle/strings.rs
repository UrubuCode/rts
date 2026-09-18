//! The stream's table of strings, from the reading side.
//!
//! Every string a v2 stream carries — a value, a key, a class or module name —
//! is either written out in full, which appends it to the table, or named by
//! its index there. The writer's half is `write::Writer::string`; this is the
//! other, split out of `read` when that module passed the crate's 500-line
//! ceiling.
//!
//! An entry remembers what it has already been turned into: a key is interned
//! once per entry however many objects use it, and a string value gets one
//! arena text however many times the stream names it — so the build interns
//! one cell for it rather than one per occurrence.

use super::format::{Broken, text};
use super::read::Reader;
use super::super::clone::Slot;
use crate::object::Key;
use crate::text::Str;

/// A string of the stream's table, with what it has already been turned into.
pub(super) struct Entry {
    text: Str,
    key: Option<Key>,
    slot: Option<Slot>,
}

impl Reader<'_, '_> {
    /// The index of a string of the table, reading it in if it is new.
    pub(super) fn entry(&mut self) -> Result<usize, Broken> {
        match self.cursor.varint()? {
            0 => {
                let text = self.raw_text()?;
                self.table.push(Entry { text, key: None, slot: None });
                Ok(self.table.len() - 1)
            }
            index => {
                let index = usize::try_from(index - 1).map_err(|_| "pickle: a bad string reference")?;
                match index < self.table.len() {
                    true => Ok(index),
                    false => Err("pickle: a reference to a string not yet written".into()),
                }
            }
        }
    }

    /// A string of the table, as a value — one text slot per entry, however
    /// many times the stream names it.
    pub(super) fn string(&mut self) -> Result<Slot, Broken> {
        let index = self.entry()?;
        if let Some(slot) = self.table[index].slot {
            return Ok(slot);
        }
        let slot = self.graph.text(self.table[index].text.clone());
        self.table[index].slot = Some(slot);
        Ok(slot)
    }

    /// The text of an entry already read.
    pub(super) fn entry_text(&self, index: usize) -> Str {
        self.table[index].text.clone()
    }

    /// A string of the table, as text.
    pub(super) fn string_text(&mut self) -> Result<Str, Broken> {
        let index = self.entry()?;
        Ok(self.table[index].text.clone())
    }

    /// A string of the table, as a property key — interned once per entry.
    pub(super) fn key(&mut self) -> Result<Key, Broken> {
        let index = self.entry()?;
        if let Some(key) = self.table[index].key {
            return Ok(key);
        }
        let key = Key::Name(self.context.interner.intern(&self.table[index].text, &mut self.context.keys));
        self.table[index].key = Some(key);
        Ok(key)
    }

    /// A length-prefixed string, as v1 wrote every one and v2 writes a new
    /// table entry.
    pub(super) fn raw_text(&mut self) -> Result<Str, Broken> {
        let bytes = self.cursor.block()?;
        text(bytes).ok_or_else(|| "pickle: text that is not UTF-8".into())
    }
}
