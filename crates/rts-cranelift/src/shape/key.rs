//! What a property is called, as far as the machine is concerned.

/// A property name, opaque.
///
/// A number the client gave meaning to. This layer compares keys and does
/// nothing else with them — it does not know they are strings, does not order
/// them, does not hash them as text.
///
/// The same reasoning as everywhere else in this crate: a key this layer
/// understood would be a key it could be wrong about. Two names that are equal
/// in one client are distinct in another, one client interns and another does
/// not, and which is right is not a machine question. Comparing numbers is the
/// only comparison that cannot be wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Key(pub(crate) u32);

impl Key {
    /// The number this key is, for a client keying its own table by it.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Hands out keys.
///
/// Takes a count and returns numbers, like every other registry here. It records
/// no names, so it cannot answer what a key is called — which is the point: the
/// client already knows, and a second copy of that knowledge is a second copy to
/// keep in agreement.
#[derive(Default)]
pub struct KeyRegistry {
    issued: u32,
}

impl KeyRegistry {
    /// A registry that has issued nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Issues `count` keys, in order.
    ///
    /// Called more than once, numbering continues rather than restarting: two
    /// clients never receive the same key, and neither learns the other exists.
    pub fn declare(&mut self, count: u32) -> Vec<Key> {
        let first = self.issued;
        self.issued += count;
        (first..self.issued).map(Key).collect()
    }

    /// Issues one key.
    pub fn declare_one(&mut self) -> Key {
        self.declare(1)[0]
    }

    /// The key a number names, if this registry issued it.
    ///
    /// # Why this exists when the constructor is private
    ///
    /// A key crosses an ABI boundary as a number — that is the whole point of
    /// numbering names, since a compiled program resolved the name while it was
    /// being compiled and handing over text at every access would hand back
    /// something already decided. What crosses has to come back.
    ///
    /// The invariant is unchanged, and that is why this asks rather than
    /// converts: a number this registry never issued answers `None`. Nobody can
    /// invent a key; somebody holding one can name it again.
    pub fn key(&self, number: u32) -> Option<Key> {
        (number < self.issued).then_some(Key(number))
    }

    /// How many exist.

    pub fn len(&self) -> usize {
        self.issued as usize
    }

    /// Whether none do.
    pub fn is_empty(&self) -> bool {
        self.issued == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_distinct_and_numbering_continues() {
        let mut registry = KeyRegistry::new();
        let first = registry.declare(2);
        let second = registry.declare_one();

        assert_ne!(first[0], first[1]);
        assert_eq!(
            second.index(),
            2,
            "numbering continues, it does not restart"
        );
    }
}
