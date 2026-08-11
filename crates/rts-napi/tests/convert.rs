//! Reading a value as a number, and turning one thing into another.

mod common;

use common::{in_a_program, string};
use rts_napi::{Env, convert, env, handles, napi_status, objects, values};

#[test]
fn int32_is_to_int32_and_not_a_cast() {
    // The three answers a hand-rolled version gets wrong. `2^31` wraps to
    // negative, `NaN` is zero, and a fraction truncates toward zero — all of
    // them the language's, because this calls `x | 0` rather than reimplementing
    // it.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        for (input, expected) in [
            (2147483648.0f64, i32::MIN),
            (-1.5, -1),
            (f64::NAN, 0),
            (4294967296.0, 0),
        ] {
            let mut value = handles::none();
            // SAFETY: live env, local out-parameter.
            unsafe { values::napi_create_double(raw, input, &mut value) };
            let mut read = 0i32;
            // SAFETY: a handle from the open scope.
            let status = unsafe { convert::napi_get_value_int32(raw, value, &mut read) };
            assert_eq!(status, napi_status::napi_ok);
            assert_eq!(read, expected, "ToInt32({input})");
        }
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn uint32_is_the_unsigned_shift() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut value = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, -1.0, &mut value) };
        let mut read = 0u32;
        // SAFETY: a handle from the open scope.
        unsafe { convert::napi_get_value_uint32(raw, value, &mut read) };
        assert_eq!(read, u32::MAX, "ToUint32(-1)");
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn int64_clamps_and_zeroes_rather_than_saturating_a_cast() {
    // `as i64` answers these differently, which is why they are written out.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        for (input, expected) in [
            (f64::INFINITY, 0i64),
            (f64::NEG_INFINITY, 0),
            (f64::NAN, 0),
            (-2.9, -2),
            (1e300, i64::MAX),
        ] {
            let mut value = handles::none();
            // SAFETY: live env.
            unsafe { values::napi_create_double(raw, input, &mut value) };
            let mut read = 1i64;
            // SAFETY: a handle from the open scope.
            unsafe { convert::napi_get_value_int64(raw, value, &mut read) };
            assert_eq!(read, expected, "int64 of {input}");
        }
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn asking_a_string_for_a_number_is_refused() {
    // `napi_get_value_int32` is not a coercion — the ABI has
    // `napi_coerce_to_number` for that, and answering 0 here would hide an
    // addon's type error.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let text = unsafe { string(raw, c"7") };
        let mut read = 0i32;
        // SAFETY: a handle from the open scope.
        let status = unsafe { convert::napi_get_value_int32(raw, text, &mut read) };
        assert_eq!(status, napi_status::napi_number_expected);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn coercion_is_the_language_s_and_not_a_reimplementation() {
    // `Number("0x10")` is 16, which every hand-rolled parser gets wrong first.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let text = unsafe { string(raw, c"0x10") };
        let mut number = handles::none();
        // SAFETY: a handle from the open scope.
        let status = unsafe { convert::napi_coerce_to_number(raw, text, &mut number) };
        assert_eq!(status, napi_status::napi_ok);
        let mut read = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, number, &mut read) };
        assert_eq!(read, 16.0);

        // And a string of a number that is not one answers NaN rather than
        // failing, because that is what `Number("x")` does.
        // SAFETY: live env.
        let bad = unsafe { string(raw, c"nope") };
        // SAFETY: handles from the open scope.
        unsafe { convert::napi_coerce_to_number(raw, bad, &mut number) };
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, number, &mut read) };
        assert!(read.is_nan());

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn coercing_to_a_string_runs_the_object_s_own_to_string() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 2.5, &mut number) };
        let mut text = handles::none();
        // SAFETY: a handle from the open scope.
        unsafe { convert::napi_coerce_to_string(raw, number, &mut text) };
        assert_eq!(
            rts_core::entry::text_of(
                // SAFETY: a handle from the open scope.
                unsafe { handles::value_of(text) }.expect("a slot")
            )
            .as_deref(),
            Some("2.5")
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn strict_equality_is_the_operator_and_not_a_bit_compare() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // Two separately made strings of the same text: `===` is true for them
        // and a comparison of encoded words might not be.
        // SAFETY: live env.
        let first = unsafe { string(raw, c"same") };
        // SAFETY: same.
        let second = unsafe { string(raw, c"same") };
        let mut equal = false;
        // SAFETY: handles from the open scope.
        let status = unsafe { convert::napi_strict_equals(raw, first, second, &mut equal) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(equal, "two strings of one text are `===`");

        let mut zero = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 0.0, &mut zero) };
        // SAFETY: handles from the open scope.
        unsafe { convert::napi_strict_equals(raw, first, zero, &mut equal) };
        assert!(!equal);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn the_global_object_is_the_one_the_program_sees() {
    // Read through the same door a program uses, so a property an addon hangs
    // on it is one the program finds.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut global = handles::none();
        // SAFETY: live env.
        let status = unsafe { convert::napi_get_global(raw, &mut global) };
        assert_eq!(status, napi_status::napi_ok);

        let mut value = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 5.0, &mut value) };
        // SAFETY: handles from the open scope, NUL-terminated literal.
        unsafe { objects::napi_set_named_property(raw, global, c"fromAnAddon".as_ptr(), value) };

        let read = rts_core::entry::with_runtime(|context| {
            let name = rts_core::entry::make_string(context, "fromAnAddon");
            let _ = name;
            rts_core::entry::get_member(
                context,
                // SAFETY: a handle from the open scope.
                unsafe { handles::value_of(global) }.expect("a slot"),
                "fromAnAddon",
            )
        });
        assert_eq!(rts_core::entry::number_of(read), Some(5.0));

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
