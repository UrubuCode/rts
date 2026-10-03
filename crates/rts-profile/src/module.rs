//! Which module a site belongs to, and the refusal that makes a stale record
//! fail loudly.

use std::fmt;

/// The identity of one module's source, as the content decides it.
///
/// # Why the content and not the path
///
/// Because a path cannot tell a changed file from an unchanged one, and a
/// profile read against a file that has been edited is the exact failure this
/// crate's design is organised around: positions have moved, so every
/// observation is attributed to whatever is at that offset now. Nothing
/// asserts a wrong answer — the guards all pass and the speculation is pointed
/// somewhere else.
///
/// Deriving the identity from the text makes that unrepresentable instead of
/// unlikely. An edited module does not match its own record, [`Profile`] refuses
/// it by name, and the compiler proceeds with no profile for that module, which
/// is the honest answer.
///
/// [`Profile`]: crate::Profile
///
/// # The cost, stated rather than discovered
///
/// Editing one line discards that module's observations entirely — there is no
/// per-function tolerance, and a scheme that tried to keep part of a module's
/// data would have to decide which positions still mean what they meant, which
/// is the question that has no answer. Granularity is the module, so editing one
/// file keeps every other module's record. That is the trade, and it is taken
/// deliberately: a smaller unit would have to guess, and refusing is the only
/// behaviour that cannot mislead.
///
/// # Why this hash and not a cryptographic one
///
/// Nothing here is adversarial — a profile is read from the build tree by the
/// build that wrote it. What is needed is that an edit changes the value, which
/// any decent 64-bit mixing gives, and a collision costs a misattributed
/// module rather than a security property. If a profile is ever fetched from
/// somewhere untrusted, this is the decision to revisit, and it is isolated to
/// this one function for that reason.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ModuleId(u64);

impl ModuleId {
    /// The identity of a module with this source text.
    ///
    /// FxHash's mixing step over the bytes, taken eight at a time. Deliberately
    /// not `DefaultHasher`: `std`'s `SipHash` is seeded per process in some
    /// configurations and every value here has to be the same in the run that
    /// writes and the run that reads.
    pub fn of_source(text: &str) -> Self {
        const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ (text.len() as u64).wrapping_mul(SEED);
        let bytes = text.as_bytes();
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
            hash = (hash ^ word).wrapping_mul(SEED).rotate_left(26);
        }
        let mut tail = 0u64;
        for (at, byte) in chunks.remainder().iter().enumerate() {
            tail |= u64::from(*byte) << (at * 8);
        }
        hash = (hash ^ tail).wrapping_mul(SEED).rotate_left(26);
        ModuleId(hash ^ (hash >> 32))
    }

    /// The identity as it is written to a record, and read back from one.
    ///
    /// Not `of_source` reversed: a reader has the number and not the text.
    pub fn from_bits(bits: u64) -> Self {
        ModuleId(bits)
    }

    /// The number this identity is.
    pub fn bits(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ModuleId {
    /// Sixteen hex digits, always, so a record is ordered the same way whether
    /// a person or a sort decides — `rts-cranelift` rule 13 for anything a
    /// person diffs between builds.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An edited module does not answer its own identity.
    ///
    /// This is the whole reason the identity is content-derived, so it is the
    /// first thing checked. A one-character change has to move it, including at
    /// the end of a module, where a hash that forgot its tail would not notice.
    #[test]
    fn an_edit_changes_the_identity() {
        let before = ModuleId::of_source("let a = 1\nlet b = 2\n");
        assert_ne!(
            before,
            ModuleId::of_source("let a = 1\nlet b = 3\n"),
            "an edit inside the text left the identity unchanged, so a stale \
             record would be accepted"
        );
        assert_ne!(
            ModuleId::of_source("abcdefgh"),
            ModuleId::of_source("abcdefghi"),
            "a byte past the last whole chunk left the identity unchanged"
        );
        assert_eq!(
            before,
            ModuleId::of_source("let a = 1\nlet b = 2\n"),
            "the same text answered two different identities, so no record \
             would ever be readable"
        );
    }

    /// An identity survives being written down.
    #[test]
    fn the_identity_round_trips_through_its_bits() {
        let id = ModuleId::of_source("print(1)");
        assert_eq!(id, ModuleId::from_bits(id.bits()));
    }

    /// The identity is empty-safe, because a module can be.
    #[test]
    fn an_empty_module_has_an_identity() {
        assert_ne!(ModuleId::of_source(""), ModuleId::of_source(" "));
    }
}
