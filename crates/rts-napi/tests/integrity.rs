//! Freezing, sealing, and every key an object has.

mod common;

use common::in_a_program;
use rts_napi::integrity::{napi_key_collection_mode, napi_key_conversion, napi_key_filter};
use rts_napi::{Env, handles, integrity, napi_status, objects, values};

/// An object with `a: 1`.
///
/// # Safety
///
/// `env` live.
unsafe fn one_property(env: rts_napi::napi_env) -> rts_napi::napi_value {
    let mut object = handles::none();
    let mut one = handles::none();
    // SAFETY: the caller's contract, local out-parameters.
    unsafe {
        objects::napi_create_object(env, &mut object);
        values::napi_create_double(env, 1.0, &mut one);
        objects::napi_set_named_property(env, object, c"a".as_ptr(), one);
    }
    object
}

/// What `napi_get_array_length` says, as a `usize`.
///
/// # Safety
///
/// `env` live, `array` a handle from an open scope.
unsafe fn length(env: rts_napi::napi_env, array: rts_napi::napi_value) -> usize {
    let mut count = 0u32;
    // SAFETY: the caller's contract, a local out-parameter.
    unsafe { objects::napi_get_array_length(env, array, &mut count) };
    count as usize
}

#[test]
fn freezing_stops_the_write_a_program_would_make() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let object = unsafe { one_property(raw) };

        // SAFETY: a handle from the open scope.
        let status = unsafe { integrity::napi_object_freeze(raw, object) };
        assert_eq!(status, napi_status::napi_ok);

        // The assertion is the EFFECT, not the status: a freeze that answered
        // `napi_ok` and changed nothing is exactly the hollow surface
        // `CLAUDE.md` refuses, and only a rejected write can tell them apart.
        let mut two = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 2.0, &mut two) };
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_set_named_property(raw, object, c"a".as_ptr(), two) };

        let mut back = handles::none();
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_get_named_property(raw, object, c"a".as_ptr(), &mut back) };
        let mut number = 0.0;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_get_value_double(raw, back, &mut number) };
        assert_eq!(number, 1.0);
    });
}

#[test]
fn sealing_keeps_the_keys_but_not_the_values() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let object = unsafe { one_property(raw) };

        // SAFETY: a handle from the open scope.
        let status = unsafe { integrity::napi_object_seal(raw, object) };
        assert_eq!(status, napi_status::napi_ok);

        // A seal is not a freeze, and the difference is the one an addon cares
        // about: an existing property still takes a write.
        let mut two = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 2.0, &mut two) };
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_set_named_property(raw, object, c"a".as_ptr(), two) };

        let mut back = handles::none();
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_get_named_property(raw, object, c"a".as_ptr(), &mut back) };
        let mut number = 0.0;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_get_value_double(raw, back, &mut number) };
        assert_eq!(number, 2.0);
    });
}

#[test]
fn own_only_answers_the_object_s_own_keys() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let object = unsafe { one_property(raw) };

        let mut names = handles::none();
        // SAFETY: a handle from the open scope, a local out-parameter.
        let status = unsafe {
            integrity::napi_get_all_property_names(
                raw,
                object,
                napi_key_collection_mode::napi_key_own_only,
                napi_key_filter::ALL_PROPERTIES,
                napi_key_conversion::napi_key_keep_numbers,
                &mut names,
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        // SAFETY: a handle from the open scope.
        assert_eq!(unsafe { length(raw, names) }, 1);
    });
}

#[test]
fn skipping_strings_leaves_nothing_rather_than_failing() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let object = unsafe { one_property(raw) };

        // Every key this engine reports is a string, so the answer is empty —
        // and it is an ANSWER. An addon filtering for symbols is asking a
        // question with a true empty answer, not making a mistake.
        let mut names = handles::none();
        // SAFETY: a handle from the open scope, a local out-parameter.
        let status = unsafe {
            integrity::napi_get_all_property_names(
                raw,
                object,
                napi_key_collection_mode::napi_key_own_only,
                napi_key_filter::SKIP_STRINGS,
                napi_key_conversion::napi_key_keep_numbers,
                &mut names,
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        // SAFETY: a handle from the open scope.
        assert_eq!(unsafe { length(raw, names) }, 0);
    });
}

#[test]
fn a_number_has_no_property_names() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 1.0, &mut number) };

        let mut names = handles::none();
        // SAFETY: a handle from the open scope, a local out-parameter.
        let status = unsafe {
            integrity::napi_get_all_property_names(
                raw,
                number,
                napi_key_collection_mode::napi_key_own_only,
                napi_key_filter::ALL_PROPERTIES,
                napi_key_conversion::napi_key_keep_numbers,
                &mut names,
            )
        };
        assert_eq!(status, napi_status::napi_object_expected);
    });
}
