//! An addon's own pointer, behind a JavaScript object.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi::abi::{napi_env, napi_ref};
use rts_napi::{Env, env, handles, napi_status, napi_valuetype, objects, values, wrap};

/// Set by [`note_finalized`] so a test can see that it ran.
///
/// Thread-local, not a `static`: cargo runs the tests of one binary on several
/// threads, and two of these register finalizers with different pointers. A
/// shared cell made them read each other's — which failed as a mismatch of two
/// plausible addresses, the least readable kind of flake.
thread_local! {
    static FINALIZED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// A finalizer that records the pointer it was handed.
///
/// # Safety
///
/// Called by this crate with the two pointers the test registered.
unsafe extern "C" fn note_finalized(_env: napi_env, data: *mut c_void, _hint: *mut c_void) {
    FINALIZED.set(data as usize);
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
        FINALIZED.set(0);
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
        assert_eq!(FINALIZED.get(), pointer as usize, "the finalizer ran");

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
        FINALIZED.set(0);
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
        assert_eq!(FINALIZED.get(), pointer as usize);
    });
}

#[test]
fn the_collector_runs_a_wrap_s_finalizer_at_the_next_drain() {
    // The third trigger, end to end: a real cycle frees the object, the sweep
    // QUEUES rather than calling, and the drain calls. Assembled here rather
    // than trusted from `rts-core`'s two halves, because the piece between them
    // is this crate's — the trampoline that recovers an environment from the
    // two words a registration carries.
    in_a_program(|| {
        FINALIZED.set(0);
        let raw = Env::new().into_raw();
        let mut owned = 4242u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();

        {
            // A scope of its own, so the only root on the object is the handle —
            // and closing it takes that away.
            // SAFETY: the pointer came from `into_raw` and is live.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            scoped.open();
        }
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };
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
        {
            // SAFETY: as above.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            assert!(scoped.close(), "the last root on the object");
        }

        // A zeroed buffer as the stack range, for the reason `rts-core`'s own
        // collector tests give: this thread's real stack holds words that can
        // decode as a reference to the cell just allocated, which is sound
        // (conservative retention) but makes the assertion flaky.
        let buffer = [0u64; 4];
        let low = buffer.as_ptr() as usize;
        rts_core::entry::with_runtime(|context| {
            context.stack_high = Some(low + core::mem::size_of_val(&buffer));
        });
        rts_core::entry::collect_now(low);

        assert_eq!(
            FINALIZED.get(),
            0,
            "the sweep must not call a finalizer — it holds the borrow"
        );
        assert_eq!(rts_core::entry::drain_finalizers(), 1, "and the drain does");
        assert_eq!(FINALIZED.get(), pointer as usize);

        // SAFETY: from `into_raw`, destroyed once — and the finalizer must NOT
        // run a second time, which is what the wrap being gone already says.
        unsafe { env::destroy(raw) };
    });
}
