//! A symbol in the stream: one spelling, read as a value and as a key.
//!
//! # What a stream can carry of a symbol
//!
//! A symbol is its identity, and identity does not serialise — which is why
//! `structuredClone` refuses one. A save file still needs the three kinds a
//! program uses, and each has something that DOES cross programs:
//!
//! - a **well-known** symbol (`Symbol.iterator`, …) is the same value in every
//!   program, and its key text `@@iterator` names it;
//! - a **registered** symbol (`Symbol.for("x")`) is whatever the reading
//!   program's registry answers for the same key, and its key text `@@for:x`
//!   names that;
//! - an **unregistered** symbol (`Symbol("x")`) can only come back as a NEW
//!   symbol of the same description — Python's pickle has the same rule for
//!   anything whose identity is its process — with its identity kept INSIDE
//!   the stream: every place that pointed at one symbol points at one revived
//!   symbol.
//!
//! # The spelling
//!
//! One text, written through the string table, which is the same for a value
//! (`OP_SYMBOL` + strref) and for a property key (a strref, as every key is):
//!
//! - shared: the symbol's own key text, `@@iterator` or `@@for:x`;
//! - unregistered: `@@sym:<n>` where `<n>` numbers the symbol WITHIN THE
//!   STREAM in the order it is first met, and the first mention carries the
//!   description after a second colon — `@@sym:0:x` — or nothing for
//!   `Symbol()`. A later mention is `@@sym:0`, which is also what a repeated
//!   strref to the first spelling parses to.
//!
//! The program's own key text for an unregistered symbol, `@@sym:7`, is the
//! same shape and MUST NOT be written: `7` is the order the program minted it
//! in, and means something else in every other program — exactly what
//! `names::portable` says about a private name's class number.
//!
//! A symbol takes no memo id: the stream's number IS its identity, and a memo
//! id is for containers whose children can point back at them.
//!
//! # Why keys go through this and not through the string table alone
//!
//! A key is a strref, and a plain interned `@@sym:0:x` would be an ordinary
//! property nothing can read — `Symbol()` in the reading program mints
//! `@@sym:<its own count>`, never `@@sym:0:x`. So the reader resolves the
//! symbol first and asks IT for the key, which is what a computed access does
//! (`symbol::key_of`), and the two agree by construction.

use super::format::Broken;
use super::read::Reader;
use super::super::symbol;
use super::write::Writer;
use crate::text::Str;

/// The reserved prefix of an unregistered symbol's spelling, in a program and
/// in a stream alike.
fn unregistered_prefix() -> String {
    format!("{}sym:", symbol::PREFIX)
}

impl Writer<'_> {
    /// A symbol VALUE's spelling, or `None` for a value that is not a symbol.
    pub(super) fn symbol_text(&mut self, bits: u64) -> Option<Str> {
        let key = symbol::key_text_of(self.context, bits)?;
        Some(self.spell(bits, &key))
    }

    /// A symbol KEY's spelling, or `None` for a key that is not a symbol's.
    pub(super) fn symbol_key_text(&mut self, key: &Str) -> Option<Str> {
        if !symbol::is_symbol_key(key) || symbol::is_private_key(key) {
            return None;
        }
        let bits = symbol::value_of_key_text(self.context, &key.to_rust()?)?;
        let key = symbol::key_text_of(self.context, bits)?;
        Some(self.spell(bits, &key))
    }

    fn spell(&mut self, bits: u64, key: &str) -> Str {
        if !key.starts_with(&unregistered_prefix()) {
            return Str::from_str(key);
        }
        if let Some(number) = self.symbols.get(&bits) {
            return Str::from_str(&format!("{}{number}", unregistered_prefix()));
        }
        let number = self.symbols.len() as u64;
        self.symbols.insert(bits, number);
        match self.context.symbol_of(bits).and_then(|symbol| symbol.description.clone()) {
            Some(description) => Str::from_str(&format!("{}{number}:{description}", unregistered_prefix())),
            None => Str::from_str(&format!("{}{number}", unregistered_prefix())),
        }
    }
}

impl Reader<'_, '_> {
    /// The symbol a spelling names in this program, minting it when the
    /// stream is the first to name it.
    pub(super) fn symbol(&mut self, text: &Str) -> Result<u64, Broken> {
        let spelled = text.to_rust().ok_or("pickle: a symbol spelled in bad text")?;
        if let Some(rest) = spelled.strip_prefix(&unregistered_prefix()) {
            let (number, description) = match rest.split_once(':') {
                Some((number, description)) => (number, Some(description.to_owned())),
                None => (rest, None),
            };
            let number: usize = number.parse().map_err(|_| "pickle: a symbol with a bad number")?;
            return match number.cmp(&self.symbols.len()) {
                std::cmp::Ordering::Less => Ok(self.symbols[number]),
                std::cmp::Ordering::Equal => {
                    let made = symbol::unique(self.context, description);
                    self.symbols.push(made);
                    Ok(made)
                }
                std::cmp::Ordering::Greater => Err("pickle: a symbol numbered ahead of its first mention".into()),
            };
        }
        let Some(name) = spelled.strip_prefix(symbol::PREFIX).filter(|name| !name.starts_with('#')) else {
            return Err("pickle: a symbol spelled as something else".into());
        };
        if let Some(found) = symbol::value_of_key_text(self.context, &spelled) {
            return Ok(found);
        }
        // Not minted here yet: a registered one under the key it was written
        // with, which is what `Symbol.for` would do; a well-known one by name.
        Ok(match name.strip_prefix("for:") {
            Some(key) => symbol::shared(self.context, spelled.clone(), Some(key.to_owned())),
            None => symbol::well_known(self.context, name),
        })
    }
}
