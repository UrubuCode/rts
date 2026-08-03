//! What a JavaScript value is, said to the machine.
//!
//! The machine encodes values and does not know what any of them mean. It
//! reserves what it defines itself — an inline integer, a reference, a boolean —
//! and hands out numbers for everything else. This is where JavaScript says what
//! its everything else is.
//!
//! # Why `undefined` and `null` are declared here and not there
//!
//! They are not machine concepts. A machine has no opinion about whether a
//! language should have one absent value or two, and JavaScript's answer — two,
//! meaning different things, comparing equal under one operator and not the other
//! — is a fact about JavaScript that a machine layer holding it would be holding
//! on behalf of every language at once.
//!
//! So they are singletons, numbered by the machine's registry, and what they mean
//! stays here.
//!
//! # Booleans are the exception, and that asymmetry is the point
//!
//! The machine defines a boolean, because it needs one: a comparison produces
//! something, and a branch consumes something. So `true` and `false` are already
//! encoded before this crate says anything, and JavaScript uses the machine's
//! rather than declaring its own.
//!
//! The rule that decides which side a value lives on is whether the machine needs
//! it to do its own work. It needs booleans. It does not need `undefined`.

use rts_cranelift::tags::{SingletonId, TagRegistry};

/// The values JavaScript has exactly one of.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Singleton {
    /// A binding that exists and has not been given anything.
    Undefined,
    /// A value that was deliberately nothing.
    Null,
}

impl Singleton {
    /// Every one, in the order they are declared.
    pub const ALL: &'static [Singleton] = &[Singleton::Undefined, Singleton::Null];

    /// What `typeof` answers for it.
    ///
    /// `null` answering `"object"` is a mistake the language made in 1995 and
    /// cannot take back, and it is written here rather than worked around,
    /// because a client asking this wants what JavaScript does and not what it
    /// should have done.
    pub fn type_of(self) -> &'static str {
        match self {
            Singleton::Undefined => "undefined",
            Singleton::Null => "object",
        }
    }

    /// Whether it is false when a condition asks.
    ///
    /// Both are. Which is not the same as the two being interchangeable — they
    /// differ under strict equality, and a lowering that used this to decide that
    /// would be using a coarser answer than the question needed.
    pub fn is_falsy(self) -> bool {
        true
    }
}

/// JavaScript's values, in the machine's encoding.
///
/// Built once per program. The numbers it hands back are what compiled code
/// compares against, so building a second one and using both would be two
/// programs disagreeing about what `undefined` is.
pub struct ValueModel {
    undefined: SingletonId,
    null: SingletonId,
}

impl ValueModel {
    /// Tells the machine what values this language has.
    pub fn declare(tags: &mut TagRegistry) -> Self {
        let declared = tags
            .declare_singletons(Singleton::ALL.len() as u32)
            .expect("two singletons fit in any payload this encoding could have");

        Self {
            undefined: declared[0],
            null: declared[1],
        }
    }

    /// The encoding of a singleton.
    pub fn singleton(&self, which: Singleton) -> SingletonId {
        match which {
            Singleton::Undefined => self.undefined,
            Singleton::Null => self.null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_cranelift::tags::{TAG_SINGLETON, is_encoded, tag_of};

    #[test]
    fn the_two_absent_values_are_distinguishable() {
        let mut tags = TagRegistry::new();
        let model = ValueModel::declare(&mut tags);

        let undefined = model.singleton(Singleton::Undefined).word();
        let null = model.singleton(Singleton::Null).word();

        assert_ne!(
            undefined, null,
            "they compare unequal under strict equality, so they cannot be one value"
        );
        assert!(is_encoded(undefined) && is_encoded(null));
        assert_eq!(tag_of(undefined), TAG_SINGLETON);
    }

    #[test]
    fn typeof_null_says_object_because_that_is_what_javascript_says() {
        assert_eq!(Singleton::Null.type_of(), "object");
        assert_eq!(Singleton::Undefined.type_of(), "undefined");
    }

    #[test]
    fn a_second_model_does_not_reuse_the_first_ones_numbers() {
        let mut tags = TagRegistry::new();
        let first = ValueModel::declare(&mut tags);
        let second = ValueModel::declare(&mut tags);

        assert_ne!(
            first.singleton(Singleton::Undefined).word(),
            second.singleton(Singleton::Undefined).word(),
            "one program has one model; two would be two programs disagreeing"
        );
    }
}
