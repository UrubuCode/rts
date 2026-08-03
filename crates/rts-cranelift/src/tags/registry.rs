//! Allocation of tags and singleton numbers.
//!
//! Clients declare how many value kinds and singletons they need and receive
//! encodings. Nothing here records who asked.

use super::{PAYLOAD_MASK, TAG_COUNT, TAG_MASK, TAG_RESERVED_COUNT, TAG_SINGLETON, encode};

/// A singleton's number.
///
/// Issued by [`TagRegistry::declare_singletons`]. The numbering is the
/// registry's output, never a constant written down a second time somewhere
/// else — a singleton reconstructed at a call site is the drift this type
/// exists to prevent.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct SingletonId(u32);

impl SingletonId {
    /// The encoded word for this singleton.
    pub fn word(self) -> u64 {
        encode(TAG_SINGLETON, u64::from(self.0))
    }

    /// The singleton's number, for a consumer keying its own table by it.
    pub fn number(self) -> u32 {
        self.0
    }
}

/// A value kind a client declared beyond the ones the machine defines.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ValueKind(u8);

impl ValueKind {
    /// Encodes a value of this kind with the given payload.
    pub fn encode(self, payload: u64) -> u64 {
        encode(self.0, payload)
    }

    /// The tag this kind was assigned.
    pub fn tag(self) -> u8 {
        self.0
    }
}

/// Hands out tags and singleton numbers.
///
/// The tag space is small — three bits, most of it spoken for. A client does not
/// mint tags freely, and running out is a real outcome rather than a theoretical
/// one, so [`TagRegistry::declare_kind`] reports it rather than panicking.
#[derive(Default)]
pub struct TagRegistry {
    next_tag: u8,
    singleton_count: u32,
}

/// Why a declaration could not be satisfied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TagError {
    /// Every tag the encoding can express is already assigned.
    TagSpaceExhausted {
        /// How many tags exist in total.
        capacity: u8,
    },
    /// More singletons were requested than the payload can number.
    TooManySingletons {
        /// How many were requested in total.
        requested: u64,
    },
}

impl TagRegistry {
    /// A registry with only the machine's own tags assigned.
    pub fn new() -> Self {
        Self {
            next_tag: TAG_RESERVED_COUNT,
            // Zero: the singleton space is entirely the client's. What this layer
            // needed from it, it took out again and gave a tag of its own.
            singleton_count: 0,
        }
    }

    /// Declares `count` singletons and returns their numbers in order.
    ///
    /// Called more than once, numbering continues rather than restarting: two
    /// clients declaring singletons never receive the same number, and neither
    /// learns that the other exists.
    pub fn declare_singletons(&mut self, count: u32) -> Result<Vec<SingletonId>, TagError> {
        let total = u64::from(self.singleton_count) + u64::from(count);
        if total > PAYLOAD_MASK {
            return Err(TagError::TooManySingletons { requested: total });
        }

        let first = self.singleton_count;
        self.singleton_count += count;
        Ok((first..self.singleton_count).map(SingletonId).collect())
    }

    /// Declares a value kind carrying its own payload.
    ///
    /// Fails once the tag space is spent. That failure is the honest outcome:
    /// widening the tag field changes the encoding for everyone and is a
    /// decision with its own justification, not something a declaration should
    /// trigger silently.
    pub fn declare_kind(&mut self) -> Result<ValueKind, TagError> {
        if u64::from(self.next_tag) > TAG_MASK {
            return Err(TagError::TagSpaceExhausted {
                capacity: TAG_COUNT,
            });
        }
        let kind = ValueKind(self.next_tag);
        self.next_tag += 1;
        Ok(kind)
    }

    /// How many singletons are declared.
    pub fn singleton_count(&self) -> u32 {
        self.singleton_count
    }

    /// How many tags remain unassigned.
    pub fn tags_remaining(&self) -> u8 {
        TAG_COUNT.saturating_sub(self.next_tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{is_encoded, payload_of, tag_of};

    #[test]
    fn singleton_numbering_is_continuous_across_declarations() {
        let mut reg = TagRegistry::new();
        let first = reg.declare_singletons(2).unwrap();
        let second = reg.declare_singletons(2).unwrap();

        assert_eq!(
            first[0].number(),
            0,
            "the singleton space is the client's alone"
        );
        assert_eq!(first[1].number(), 1);
        assert_eq!(
            second[0].number(),
            2,
            "numbering continues, it does not restart"
        );
        assert_eq!(reg.singleton_count(), 4);
    }

    #[test]
    fn a_singleton_word_is_encoded_and_carries_its_number() {
        let mut reg = TagRegistry::new();
        let ids = reg.declare_singletons(3).unwrap();
        let word = ids[2].word();

        assert!(is_encoded(word));
        assert_eq!(tag_of(word), crate::tags::TAG_SINGLETON);
        assert_eq!(payload_of(word), 2);
    }

    #[test]
    fn the_tag_space_runs_out_rather_than_wrapping() {
        let mut reg = TagRegistry::new();
        let available = reg.tags_remaining();
        for _ in 0..available {
            reg.declare_kind().expect("within capacity");
        }
        assert_eq!(
            reg.declare_kind(),
            Err(TagError::TagSpaceExhausted {
                capacity: TAG_COUNT
            })
        );
    }
}
