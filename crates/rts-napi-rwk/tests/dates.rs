//! `Date` across the boundary, and the thing it must agree with.

mod common;

use common::in_a_program;
use rts_napi_rwk::{Env, dates, handles, napi_status, objects, values};

#[test]
fn a_date_made_here_answers_the_milliseconds_it_was_given() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let when = 1_600_000_000_000.0;
        let mut made = handles::none();
        // SAFETY: live env, local out-parameter.
        let status = unsafe { dates::napi_create_date(raw, when, &mut made) };
        assert_eq!(status, napi_status::napi_ok);

        let mut back = 0.0;
        // SAFETY: a handle from the open scope.
        let status = unsafe { dates::napi_get_date_value(raw, made, &mut back) };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(back, when);
    });
}

#[test]
fn what_this_calls_a_date_is_what_the_language_calls_one() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut made = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { dates::napi_create_date(raw, 0.0, &mut made) };

        let mut is_date = false;
        // SAFETY: a handle from the open scope.
        unsafe { dates::napi_is_date(raw, made, &mut is_date) };
        assert!(is_date);

        // The pin is the agreement, not the flag: a date this crate built and
        // the language did not recognise would pass the assertion above and be
        // useless to an addon that hands it to a program.
        let mut instance = false;
        let mut date_class = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { convert_global(raw, &mut date_class) };
        // SAFETY: two handles from the open scope.
        unsafe { rts_napi_rwk::class::napi_instanceof(raw, made, date_class, &mut instance) };
        assert!(instance);
    });
}

/// `globalThis.Date`, as a handle.
///
/// # Safety
///
/// `env` live, `out` writable.
unsafe fn convert_global(
    env: rts_napi_rwk::napi_env,
    out: *mut rts_napi_rwk::napi_value,
) -> napi_status {
    let mut global = handles::none();
    // SAFETY: the caller's contract.
    unsafe { rts_napi_rwk::convert::napi_get_global(env, &mut global) };
    let name = c"Date";
    // SAFETY: the caller's contract, and a NUL-terminated literal.
    unsafe { objects::napi_get_named_property(env, global, name.as_ptr(), out) }
}

#[test]
fn a_plain_object_is_not_a_date_and_asking_for_its_value_is_refused() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };

        let mut is_date = true;
        // SAFETY: a handle from the open scope.
        unsafe { dates::napi_is_date(raw, object, &mut is_date) };
        assert!(!is_date);

        let mut back = 0.0;
        // SAFETY: a handle from the open scope.
        let status = unsafe { dates::napi_get_date_value(raw, object, &mut back) };
        assert_eq!(status, napi_status::napi_date_expected);
    });
}

#[test]
fn a_number_is_not_a_date_either() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 1.0, &mut number) };

        let mut is_date = true;
        // SAFETY: a handle from the open scope.
        unsafe { dates::napi_is_date(raw, number, &mut is_date) };
        assert!(!is_date);
    });
}
