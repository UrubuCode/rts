//! Conversions between representations.
//!
//! The ones here need no heap: a value that is already a number, a boolean or a
//! singleton converts by arithmetic. Anything reaching a string or calling an
//! object's `valueOf` needs the heap and belongs with the code that has one —
//! kept out of here so that this module compiles and is testable on a target
//! with no allocator at all.

use super::{Kind, Value};

/// Which singletons exist, as far as conversion is concerned.
///
/// The language layer decides what its singletons *mean* and numbers them; this
/// layer only needs to know which are falsy and which convert to what. Passed
/// in rather than assumed, because a second language on this machine would
/// number its own differently — and hardcoding JavaScript's numbering here is
/// exactly the kind of knowledge this crate is not supposed to hold.
#[derive(Clone, Copy, Debug)]
pub struct Singletons {
    /// The one that reads as `NaN` when converted to a number.
    pub undefined: u32,
    /// The one that reads as `+0`.
    pub null: u32,
    /// A posição AUSENTE de um array — o que `[1, , 3]` tem no índice 1.
    ///
    /// Não é um valor da linguagem e nenhum programa o observa: toda leitura de
    /// elemento o converte em `undefined`. Ele existe para que quem pergunta se
    /// a posição EXISTE possa responder — `0 in [,1]` é falso e
    /// `0 in [undefined,1]` é verdadeiro, e sem um marcador os dois seriam a
    /// mesma coisa.
    ///
    /// Numerado pela linguagem junto dos outros dois, e não escolhido aqui, pela
    /// razão que [`Singletons`] inteira documenta: o espaço é do cliente.
    pub hole: u32,
}

/// Which tag the language gave each kind it declared for itself.
///
/// The same shape as [`Singletons`] and for the same reason: the machine
/// reserves four tags and hands the rest out by number, so which number means
/// `symbol` is something the language decided and this crate is told. A constant
/// written here would be this crate holding a fact about JavaScript, and a
/// second language on the same machine would number its own differently.
///
/// # Why a symbol and a bigint are here and an object is not
///
/// Because they are **primitives**. `typeof` answers `"symbol"` and `"bigint"`,
/// `s.x = 1` writes nothing, and `1n === 1n` is true — none of which is what a
/// reference does. Encoding either as a cell buys the heap it needs for its
/// description or its digits and loses every one of those properties, which is
/// what the first version of `Symbol` here did and what this replaces.
#[derive(Clone, Copy, Debug)]
pub struct Kinds {
    /// A symbol. The payload is its number, and two symbols differ in it.
    pub symbol: u8,
    /// A bigint. The payload is where its digits are, because arbitrary
    /// precision does not fit in forty-eight bits — a primitive whose data is
    /// on the heap, which is what every engine does with one.
    pub bigint: u8,
}

/// The falsy set.
///
/// `undefined`, `null`, `false`, `+0`, `-0`, `NaN`, and the empty string. Every
/// other value is truthy — *every* object, including an empty one, an empty
/// array, and `new Boolean(false)`. Nothing is unwrapped, and no `valueOf` runs,
/// which is why this needs no heap.
///
/// Two cases cannot be answered without the heap, so a caller passes
/// `falsy_on_heap` and this asks it: the **empty string**, and **`0n`** — a
/// bigint is falsy exactly when it is zero, and whether it is zero is in digits
/// this layer cannot see. Making it a parameter rather than reaching for the
/// heap keeps the falsy rule in one place and leaves the two heap questions with
/// the code that owns the heap.
///
/// The callback takes the whole `Value` rather than a payload, because the two
/// questions are not the same question: a payload alone cannot say whether it
/// names a string cell or a bigint, and a caller told only the number would have
/// to guess.
pub fn to_boolean(
    value: Value,
    singletons: Singletons,
    falsy_on_heap: impl Fn(Value) -> bool,
) -> bool {
    match value.kind() {
        Kind::Float => {
            let number = f64::from_bits(value.bits());
            // Catches +0, -0 and NaN in one condition: NaN fails every
            // comparison, and both zeros fail `!= 0.0`.
            !(number == 0.0 || number.is_nan())
        }
        Kind::Int => value.as_i32() != Some(0),
        Kind::Bool => value.as_bool() == Some(true),
        Kind::Singleton(id) => id != singletons.undefined && id != singletons.null,
        // A symbol reaches here too and is always truthy — which the caller
        // answers by saying it is not falsy, rather than by this layer learning
        // that one of the language's kinds is a symbol.
        Kind::Reference(_) | Kind::Client { .. } => !falsy_on_heap(value),
    }
}

/// `ToNumber`, for the values that need no heap.
///
/// Returns `None` for a reference, which needs `ToPrimitive` and therefore a
/// heap and possibly a call into user code. An absent answer is the honest one:
/// guessing zero here is how a conversion that should have run a `valueOf` turns
/// into a wrong number nobody traces back.
pub fn to_number(value: Value, singletons: Singletons) -> Option<f64> {
    match value.kind() {
        Kind::Float | Kind::Int => value.numeric(),
        // `true` is 1 and `false` is +0.
        Kind::Bool => Some(if value.as_bool() == Some(true) {
            1.0
        } else {
            0.0
        }),
        Kind::Singleton(id) if id == singletons.undefined => Some(f64::NAN),
        Kind::Singleton(id) if id == singletons.null => Some(0.0),
        // A reference needs `ToPrimitive`, and a language kind needs the heap
        // its payload names — a bigint converts from digits this layer cannot
        // see, and a symbol does not convert at all. `None` is the same honest
        // absence in all three cases: guessing zero is how a conversion that
        // should have run a `valueOf` becomes a wrong number nobody traces back.
        Kind::Singleton(_) | Kind::Reference(_) | Kind::Client { .. } => None,
    }
}

/// `ToInt32`: what every bitwise operator does to its operands.
///
/// Modular, not saturating and not a `as` cast: `2**32` becomes `0`, and
/// `2**31` becomes `i32::MIN`. Anything non-finite becomes zero, which is why
/// `NaN | 0` is `0` rather than an error.
pub fn to_int32(number: f64) -> i32 {
    if !number.is_finite() {
        return 0;
    }
    // Truncate toward zero, then reduce modulo 2^32 and reinterpret the low
    // half as signed. `as i32` alone would saturate, which is the wrong
    // arithmetic: it makes `2**31 | 0` produce i32::MAX instead of i32::MIN.
    let truncated = number.trunc();
    let wrapped = truncated.rem_euclid(4_294_967_296.0);
    wrapped as u32 as i32
}

/// `ToUint32`: what `>>>` does to its left operand.
///
/// The reason `>>>` is the one bitwise operator whose result does not fit a
/// signed 32-bit value — it ranges to 2³²−1, so `(-1) >>> 0` is `4294967295`.
pub fn to_uint32(number: f64) -> u32 {
    to_int32(number) as u32
}

#[cfg(test)]
mod tests {
    use rts_cranelift::tags::{TAG_REFERENCE, TAG_SINGLETON, encode};

    use super::*;

    const S: Singletons = Singletons { undefined: 0, null: 1, hole: 2 };

    fn never_empty(_: Value) -> bool {
        false
    }

    #[test]
    fn the_falsy_set_is_exactly_seven_things() {
        assert!(!to_boolean(Value::from_f64(0.0), S, never_empty));
        assert!(!to_boolean(Value::from_f64(-0.0), S, never_empty));
        assert!(!to_boolean(Value::from_f64(f64::NAN), S, never_empty));
        assert!(!to_boolean(Value::from_i32(0), S, never_empty));
        assert!(!to_boolean(Value::from_bool(false), S, never_empty));

        assert!(to_boolean(Value::from_f64(0.1), S, never_empty));
        assert!(to_boolean(Value::from_i32(-1), S, never_empty));
        assert!(to_boolean(Value::from_bool(true), S, never_empty));
        assert!(to_boolean(Value::from_f64(f64::INFINITY), S, never_empty));
    }

    #[test]
    fn an_object_is_truthy_however_empty_it_is() {
        let object = Value(encode(TAG_REFERENCE, 5));
        assert!(
            to_boolean(object, S, never_empty),
            "no unwrapping happens: new Boolean(false) is truthy"
        );
    }

    #[test]
    fn undefined_becomes_nan_and_null_becomes_zero() {
        let undefined = Value(encode(TAG_SINGLETON, 0));
        let null = Value(encode(TAG_SINGLETON, 1));

        assert!(to_number(undefined, S).unwrap().is_nan());
        assert_eq!(to_number(null, S), Some(0.0));
        assert!(!to_boolean(undefined, S, never_empty));
        assert!(!to_boolean(null, S, never_empty));
    }

    #[test]
    fn to_number_refuses_a_reference_rather_than_guessing() {
        let object = Value(encode(TAG_REFERENCE, 5));
        assert_eq!(
            to_number(object, S),
            None,
            "it needs ToPrimitive, which may run user code; zero would be a \
             wrong number nobody traces back"
        );
    }

    #[test]
    fn to_int32_wraps_where_a_cast_would_saturate() {
        assert_eq!(to_int32(4_294_967_296.0), 0, "2**32 | 0 is 0");
        assert_eq!(
            to_int32(2_147_483_648.0),
            i32::MIN,
            "2**31 | 0 is i32::MIN — `as i32` would saturate to i32::MAX"
        );
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(1.9), 1, "truncates toward zero");
        assert_eq!(to_int32(-1.9), -1);
    }

    #[test]
    fn non_finite_becomes_zero_rather_than_failing() {
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(f64::NEG_INFINITY), 0);
    }

    #[test]
    fn unsigned_shift_is_the_one_that_outgrows_a_signed_result() {
        assert_eq!(to_uint32(-1.0), 4_294_967_295, "(-1) >>> 0");
        assert_eq!(to_int32(-1.0), -1, "and (-1) >> 0 is still -1");
    }
}

impl Kinds {
    /// The numbering a test uses when no compilation declared one.
    ///
    /// The machine reserves four tags and hands the rest out in order, so this
    /// is what `ValueModel::declare` produces for the first program in a
    /// process. Written here rather than in each test, because a test that
    /// picked its own numbers would be asserting against a numbering nothing
    /// else uses.
    #[cfg(test)]
    pub fn in_declaration_order() -> Self {
        Kinds {
            symbol: rts_cranelift::tags::TAG_RESERVED_COUNT,
            bigint: rts_cranelift::tags::TAG_RESERVED_COUNT + 1,
        }
    }
}
