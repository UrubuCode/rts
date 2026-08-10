//! Throwing, and finding out that something threw.

mod common;

use common::{in_a_program, string};
use rts_napi_rwk::{Env, env, errors, handles, napi_status, objects, values};

#[test]
fn a_thrown_error_is_pending_and_comes_back_once() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env, NUL-terminated literals.
        let status = unsafe {
            errors::napi_throw_error(raw, c"ENOENT".as_ptr(), c"no such file".as_ptr())
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut pending = false;
        // SAFETY: local out-parameter.
        unsafe { errors::napi_is_exception_pending(raw, &mut pending) };
        assert!(pending);

        let mut caught = handles::none();
        // SAFETY: live env.
        unsafe { errors::napi_get_and_clear_last_exception(raw, &mut caught) };

        // The `code` is an ordinary property, which is what `err.code` reads.
        let mut read = handles::none();
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, caught, c"code".as_ptr(), &mut read) };
        let mut buffer = [0i8; 16];
        let mut written = 0usize;
        // SAFETY: sixteen writable bytes.
        unsafe {
            objects::napi_get_value_string_utf8(
                raw,
                read,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut written,
            )
        };
        assert_eq!(written, 6, "ENOENT");

        // Cleared by the take: leaving it set would make the next call look
        // like it threw.
        // SAFETY: local out-parameter.
        unsafe { errors::napi_is_exception_pending(raw, &mut pending) };
        assert!(!pending, "taking the exception clears it");
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_string_is_throwable_because_the_language_says_so() {
    // `napi_throw` takes a VALUE. Refusing anything but an error object here
    // would be this crate deciding a language question it has no business
    // deciding — `throw "x"` is legal JavaScript.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let text = unsafe { string(raw, c"not an error") };
        // SAFETY: a handle from the open scope.
        assert_eq!(
            unsafe { errors::napi_throw(raw, text) },
            napi_status::napi_ok
        );

        let mut caught = handles::none();
        // SAFETY: live env.
        unsafe { errors::napi_get_and_clear_last_exception(raw, &mut caught) };
        assert_eq!(
            rts_core::entry::text_of(
                // SAFETY: a handle from the open scope.
                unsafe { handles::value_of(caught) }.expect("a slot")
            )
            .as_deref(),
            Some("not an error")
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn asking_with_nothing_pending_answers_undefined_rather_than_failing() {
    // The ABI's own wording, and an addon asks without knowing.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut caught = handles::none();
        // SAFETY: live env.
        let status = unsafe { errors::napi_get_and_clear_last_exception(raw, &mut caught) };
        assert_eq!(status, napi_status::napi_ok);

        let mut kind = rts_napi_rwk::napi_valuetype::napi_object;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_typeof(raw, caught, &mut kind) };
        assert_eq!(kind, rts_napi_rwk::napi_valuetype::napi_undefined);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn an_error_is_one_by_instanceof_and_a_plain_object_is_not() {
    // Asked the way the program asks. The first version of `napi_is_error`
    // looked for a `message` property, which every object can have.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut message = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_get_undefined(raw, &mut message) };
        // SAFETY: live env.
        let text = unsafe { string(raw, c"boom") };

        let mut error = handles::none();
        // SAFETY: handles from the open scope.
        let status = unsafe {
            errors::napi_create_error(raw, handles::none(), text, &mut error)
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut is_error = false;
        // SAFETY: a handle from the open scope.
        unsafe { errors::napi_is_error(raw, error, &mut is_error) };
        assert!(is_error, "built by `napi_create_error`");

        let mut plain = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut plain) };
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_set_named_property(raw, plain, c"message".as_ptr(), text) };
        // SAFETY: same.
        unsafe { errors::napi_is_error(raw, plain, &mut is_error) };
        assert!(
            !is_error,
            "an object with a `message` is not an Error, and a heuristic would \
             have said it was"
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_created_error_is_not_thrown_until_it_is() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let text = unsafe { string(raw, c"later") };
        let mut error = handles::none();
        // SAFETY: handles from the open scope.
        unsafe { errors::napi_create_type_error(raw, handles::none(), text, &mut error) };

        let mut pending = true;
        // SAFETY: local out-parameter.
        unsafe { errors::napi_is_exception_pending(raw, &mut pending) };
        assert!(!pending, "creating is not throwing");

        // SAFETY: a handle from the open scope.
        unsafe { errors::napi_throw(raw, error) };
        // SAFETY: local out-parameter.
        unsafe { errors::napi_is_exception_pending(raw, &mut pending) };
        assert!(pending);

        // Left pending on purpose is not an option: the next test would see it.
        let mut caught = handles::none();
        // SAFETY: live env.
        unsafe { errors::napi_get_and_clear_last_exception(raw, &mut caught) };
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
