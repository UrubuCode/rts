//! An addon's own pointer, behind a JavaScript object.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi_rwk::abi::{napi_env, napi_ref};
use rts_napi_rwk::{Env, env, handles, napi_status, napi_valuetype, objects, values, wrap};

/// Set by [`note_finalized`] so a test can see that it ran.
static mut FINALIZED: usize = 0;

/// A finalizer that records the pointer it was handed.
///
/// # Safety
///
/// Called by this crate with the two pointers the test registered.
unsafe extern "C" fn note_finalized(_env: napi_env, data: *mut c_void, _hint: *mut c_void) {
    // SAFETY: single-threaded test, written and read between calls.
    unsafe { FINALIZED = data as usize };
}

#[test]
fn a_wrapped_pointer_comes_back_and_the_object_is_still_an_object() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };

        let mut owned = 1234u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        // SAFETY: a handle from the open scope; `owned` outlives the wrap.
        let status = unsafe {
            wrap::napi_wrap(
                raw,
                object,
                pointer,
                None,
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: as above.
        unsafe { wrap::napi_unwrap(raw, object, &mut read) };
        assert_eq!(read, pointer);

        let mut kind = napi_valuetype::napi_undefined;
        // SAFETY: as above.
        unsafe { values::napi_typeof(raw, object, &mut kind) };
        assert_eq!(
            kind,
            napi_valuetype::napi_object,
            "wrapping does not turn an object into an external"
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn wrapping_twice_is_refused_rather_than_stranding_the_first_pointer() {
    // Overwriting would leave the addon owning memory whose finalizer can never
    // run, and nothing would ever say so.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };
        let mut first = 1u64;
        let mut second = 2u64;
        // SAFETY: handles from the open scope.
        unsafe {
            wrap::napi_wrap(
                raw,
                object,
                (&mut first as *mut u64).cast(),
                None,
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        // SAFETY: same.
        let status = unsafe {
            wrap::napi_wrap(
                raw,
                object,
                (&mut second as *mut u64).cast(),
                None,
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        assert_eq!(status, napi_status::napi_invalid_arg);

        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: same.
        unsafe { wrap::napi_unwrap(raw, object, &mut read) };
        assert_eq!(read, (&mut first as *mut u64).cast(), "the FIRST pointer");
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn wrapping_a_number_is_object_expected() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 1.0, &mut number) };
        let mut owned = 1u64;
        // SAFETY: a handle from the open scope.
        let status = unsafe {
            wrap::napi_wrap(
                raw,
                number,
                (&mut owned as *mut u64).cast(),
                None,
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        assert_eq!(status, napi_status::napi_object_expected);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn removing_a_wrap_runs_the_finalizer_and_hands_the_pointer_back() {
    in_a_program(|| {
        // SAFETY: single-threaded test.
        unsafe { FINALIZED = 0 };
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };
        let mut owned = 77u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        // SAFETY: a handle from the open scope.
        unsafe {
            wrap::napi_wrap(
                raw,
                object,
                pointer,
                Some(note_finalized),
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };

        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: same.
        let status = unsafe { wrap::napi_remove_wrap(raw, object, &mut read) };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(read, pointer);
        // SAFETY: single-threaded test.
        assert_eq!(unsafe { FINALIZED }, pointer as usize, "the finalizer ran");

        // And the wrap is gone, so unwrapping now fails rather than answering a
        // pointer the addon has taken back.
        // SAFETY: same.
        assert_eq!(
            unsafe { wrap::napi_unwrap(raw, object, &mut read) },
            napi_status::napi_invalid_arg
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn an_external_is_its_own_type_and_a_wrapped_object_is_not() {
    // The ABI distinguishes them and the language cannot, which is why
    // `napi_typeof` asks `wrap::is_external` at all.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut owned = 5u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        let mut external = handles::none();
        // SAFETY: live env, local out-parameter.
        let status = unsafe {
            wrap::napi_create_external(
                raw,
                pointer,
                None,
                core::ptr::null_mut(),
                &mut external,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut kind = napi_valuetype::napi_undefined;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_typeof(raw, external, &mut kind) };
        assert_eq!(kind, napi_valuetype::napi_external);

        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: same.
        assert_eq!(
            unsafe { wrap::napi_get_value_external(raw, external, &mut read) },
            napi_status::napi_ok
        );
        assert_eq!(read, pointer);

        // An ordinary wrapped object is refused by that door: the addon never
        // put a pointer there through `create_external`.
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };
        // SAFETY: handles from the open scope.
        unsafe {
            wrap::napi_wrap(
                raw,
                object,
                pointer,
                None,
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        // SAFETY: same.
        assert_eq!(
            unsafe { wrap::napi_get_value_external(raw, object, &mut read) },
            napi_status::napi_invalid_arg
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn destroying_the_environment_runs_a_wrap_s_finalizer() {
    // The other trigger. An addon that unloads without removing its wraps is
    // the common case, and P6 — the collector telling anyone — is the third.
    in_a_program(|| {
        // SAFETY: single-threaded test.
        unsafe { FINALIZED = 0 };
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };
        let mut owned = 99u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        // SAFETY: a handle from the open scope.
        unsafe {
            wrap::napi_wrap(
                raw,
                object,
                pointer,
                Some(note_finalized),
                core::ptr::null_mut(),
                core::ptr::null_mut::<napi_ref>(),
            )
        };
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
        // SAFETY: single-threaded test.
        assert_eq!(unsafe { FINALIZED }, pointer as usize);
    });
}
