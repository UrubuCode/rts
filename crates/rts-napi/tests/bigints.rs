//! Bigints across the boundary: sign, words, and what "lossless" reports.

mod common;

use common::in_a_program;
use rts_napi::{Env, bigints, handles, napi_status, napi_valuetype, values};

#[test]
fn an_int64_comes_back_as_the_same_number_and_is_a_bigint() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut made = handles::none();
        // SAFETY: live env, local out-parameter.
        let status = unsafe { bigints::napi_create_bigint_int64(raw, -42, &mut made) };
        assert_eq!(status, napi_status::napi_ok);

        // The type is asked through `napi_typeof`, not assumed: an integer that
        // came back as a NUMBER would pass every value assertion below and be
        // the wrong thing entirely.
        let mut kind = napi_valuetype::napi_undefined;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_typeof(raw, made, &mut kind) };
        assert_eq!(kind, napi_valuetype::napi_bigint);

        let mut back = 0i64;
        let mut lossless = false;
        // SAFETY: a handle from the open scope, local out-parameters.
        let status =
            unsafe { bigints::napi_get_value_bigint_int64(raw, made, &mut back, &mut lossless) };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(back, -42);
        assert!(lossless);
    });
}

#[test]
fn a_uint64_above_the_signed_range_survives_the_round_trip() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let big = u64::MAX - 1;
        let mut made = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { bigints::napi_create_bigint_uint64(raw, big, &mut made) };

        let mut back = 0u64;
        let mut lossless = false;
        // SAFETY: a handle from the open scope.
        unsafe { bigints::napi_get_value_bigint_uint64(raw, made, &mut back, &mut lossless) };
        assert_eq!(back, big);
        assert!(lossless);

        // The same value read as SIGNED does not fit, and the ABI's answer is
        // the truncation plus a false `lossless` — not a refusal. This is the
        // half an addon relies on to detect the overflow at all.
        let mut narrow = 0i64;
        let mut fits = true;
        // SAFETY: a handle from the open scope.
        let status =
            unsafe { bigints::napi_get_value_bigint_int64(raw, made, &mut narrow, &mut fits) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(!fits);
    });
}

#[test]
fn words_go_out_and_come_back_with_the_sign_intact() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // Two words, so the answer cannot be right by accident from a
        // single-word path: 2^64 + 7, negated.
        let given = [7u64, 1u64];
        let mut made = handles::none();
        // SAFETY: live env, a local array of the stated length.
        let status = unsafe {
            bigints::napi_create_bigint_words(raw, 1, given.len(), given.as_ptr(), &mut made)
        };
        assert_eq!(status, napi_status::napi_ok);

        // The sizing call first — a null `words`, which is how an addon that
        // does not know the magnitude asks how much room to make.
        let mut sign = 0i32;
        let mut count = 0usize;
        // SAFETY: a handle from the open scope, local out-parameters.
        let status = unsafe {
            bigints::napi_get_value_bigint_words(
                raw,
                made,
                &mut sign,
                &mut count,
                core::ptr::null_mut(),
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(sign, 1);
        assert_eq!(count, 2);

        let mut back = [0u64; 2];
        // SAFETY: a handle from the open scope; `count` matches the array.
        unsafe {
            bigints::napi_get_value_bigint_words(
                raw,
                made,
                &mut sign,
                &mut count,
                back.as_mut_ptr(),
            )
        };
        assert_eq!(back, given);
    });
}

#[test]
fn an_undersized_buffer_is_told_how_much_it_actually_got() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let given = [1u64, 2u64];
        let mut made = handles::none();
        // SAFETY: live env, a local array of the stated length.
        unsafe {
            bigints::napi_create_bigint_words(raw, 0, given.len(), given.as_ptr(), &mut made)
        };

        // Room for one word where two are needed. The count that comes back is
        // what was WRITTEN, which is the only way an addon can tell it lost
        // half the number.
        let mut sign = 0i32;
        let mut count = 1usize;
        let mut back = [0u64; 1];
        // SAFETY: a handle from the open scope; `count` matches the array.
        let status = unsafe {
            bigints::napi_get_value_bigint_words(
                raw,
                made,
                &mut sign,
                &mut count,
                back.as_mut_ptr(),
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(count, 1);
        assert_eq!(back[0], 1);
    });
}

#[test]
fn a_number_is_not_a_bigint() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 5.0, &mut number) };

        let mut back = 0i64;
        let mut lossless = false;
        // SAFETY: a handle from the open scope.
        let status =
            unsafe { bigints::napi_get_value_bigint_int64(raw, number, &mut back, &mut lossless) };
        assert_eq!(status, napi_status::napi_bigint_expected);
    });
}
