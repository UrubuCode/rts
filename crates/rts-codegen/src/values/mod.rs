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

use rts_cranelift::tags::{SingletonId, TagRegistry, ValueKind};

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
    hole: SingletonId,
    symbol: ValueKind,
    bigint: ValueKind,
}

impl ValueModel {
    /// Tells the machine what values this language has.
    pub fn declare(tags: &mut TagRegistry) -> Self {
        // Um a mais que os valores da linguagem: o BURACO.
        //
        // Ele não está em `Singleton` de propósito — aquele enum é "os valores
        // que JavaScript tem exatamente um de", e um buraco não é um valor, é a
        // AUSÊNCIA de um. Nenhum programa jamais o observa: toda leitura de
        // elemento o converte em `undefined`, e o que o distingue é só quem
        // pergunta se a posição EXISTE (`in`, `hasOwnProperty`, `Object.keys`,
        // e os métodos que a especificação manda pular).
        //
        // Por que ainda assim é a linguagem quem o numera: o espaço de
        // singleton é do cliente (`tags::TagRegistry` diz isso), então um
        // número escolhido pelo runtime poderia colidir com um que a linguagem
        // declarasse depois. Um padrão de bits inventado seria pior — já houve
        // um `0` usado como sentinela aqui que destruiu `+0.0` armazenado.
        let declared = tags
            .declare_singletons(Singleton::ALL.len() as u32 + 1)
            .expect("three singletons fit in any payload this encoding could have");
        // Two of the four tags the machine leaves to a client. Both are
        // JavaScript **primitives**, and that is the reason they are kinds
        // rather than references: `typeof` has to answer from the word alone,
        // `s.x = 1` has to write nothing, and `1n === 1n` has to be true. A cell
        // gives none of those, which is what the first `Symbol` here learned.
        //
        // Two remain unassigned, and the machine's registry reports exhaustion
        // rather than panicking — so a third primitive is a decision with a cost
        // a reader can see rather than a silent overflow.
        let symbol = tags
            .declare_kind()
            .expect("the machine reserves four tags and leaves four");
        let bigint = tags
            .declare_kind()
            .expect("the machine reserves four tags and leaves four");

        Self {
            undefined: declared[0],
            null: declared[1],
            // O último, depois dos que `Singleton::ALL` nomeia.
            hole: declared[Singleton::ALL.len()],
            symbol,
            bigint,
        }
    }

    /// O marcador de posição AUSENTE num array.
    ///
    /// Ver a nota em [`ValueModel::declare`]: não é um valor da linguagem, e um
    /// programa nunca o vê. O runtime precisa dele para responder `0 in [,1]`
    /// com `false` enquanto `[,1][0]` responde `undefined`.
    pub fn hole(&self) -> SingletonId {
        self.hole
    }

    /// The encoding of a singleton.
    pub fn singleton(&self, which: Singleton) -> SingletonId {
        match which {
            Singleton::Undefined => self.undefined,
            Singleton::Null => self.null,
        }
    }

    /// The encoding of a primitive this language declared for itself.
    pub fn kind(&self, which: Primitive) -> ValueKind {
        match which {
            Primitive::Symbol => self.symbol,
            Primitive::BigInt => self.bigint,
        }
    }
}

/// The primitives JavaScript has that the machine does not define.
///
/// A symbol and a bigint. Both are values a program can hold, compare and ask
/// `typeof` about, and neither is a number, a boolean or a reference — so each
/// needs a tag of its own, and what it means stays here for the reason
/// `undefined` does: a machine has no opinion about whether a language has
/// symbols.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Primitive {
    /// A value equal to nothing but itself, usable as a property key.
    Symbol,
    /// An integer of arbitrary size. Its digits are on the heap and its
    /// **equality is by value** — `1n === 1n` — which is what separates it from
    /// a reference carrying the same digits.
    BigInt,
}

impl Primitive {
    /// Every one, in the order they are declared.
    pub const ALL: &'static [Primitive] = &[Primitive::Symbol, Primitive::BigInt];

    /// What `typeof` answers for it.
    pub fn type_of(self) -> &'static str {
        match self {
            Primitive::Symbol => "symbol",
            Primitive::BigInt => "bigint",
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
