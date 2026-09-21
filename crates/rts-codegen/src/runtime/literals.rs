//! The program's string table, and the one rule for numbering it.
//!
//! # Why this is a type and not a `Vec` in each emitter
//!
//! Because a string's index is an agreement with the runtime — `RuntimeOp::StringConst`
//! takes it and `rts_core::entry::text::string_const` reads the table the host installed
//! — and two emitters numbering their own would be two tables of one number, which the
//! `reuse-check` skill calls fatal for the reason this case shows: the runtime holds ONE
//! table, so a second numbering reaches the wrong string rather than failing.
//!
//! `emit/` had the `Vec` and the numbering rule on `Ctx`. The new lowering needed the
//! same numbering, and taking a copy of three lines is exactly how the two would come to
//! disagree the first time either learned something — a normalisation, a cap, an ordering.
//!
//! # Deduplicated, and what that is FOR
//!
//! Two occurrences of one piece across a whole program are one string. That is not a size
//! optimisation: `===` over two string literals of the same text answers true because
//! they are the same value, and a table that numbered them apart would make the answer
//! depend on how many times a program wrote them.

/// Every string a program holds, numbered by first appearance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Literals {
    units: Vec<Vec<u16>>,
}

impl Literals {
    /// Nothing interned yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number for this text, interning it the first time it is seen.
    ///
    /// Takes code UNITS and not `&str`, and that is the load-bearing half: `"\uD83D"` is a
    /// legal one-unit string and there is no `&str` that spells it. A caller holding Rust
    /// text uses [`Self::intern_str`], which loses nothing on the way in — `encode_utf16`
    /// of valid UTF-8 is exactly its code units.
    pub fn intern(&mut self, units: &[u16]) -> u32 {
        if let Some(found) = self.units.iter().position(|held| held == units) {
            return found as u32;
        }
        self.units.push(units.to_vec());
        (self.units.len() - 1) as u32
    }

    /// The same, for text that is Rust's.
    ///
    /// Through the same table, because a literal an emitter SYNTHESISES — a module
    /// specifier, a private name's key, a template's raw text — is the same string as one
    /// the program wrote with those characters.
    pub fn intern_str(&mut self, text: &str) -> u32 {
        let units: Vec<u16> = text.encode_utf16().collect();
        self.intern(&units)
    }

    /// How many distinct strings.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether none were interned.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The table, as the host installs it.
    pub fn units(&self) -> &[Vec<u16>] {
        &self.units
    }

    /// The table, given away.
    pub fn into_units(self) -> Vec<Vec<u16>> {
        self.units
    }
}

impl From<Vec<Vec<u16>>> for Literals {
    fn from(units: Vec<Vec<u16>>) -> Self {
        Self { units }
    }
}
