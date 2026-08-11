//! What runs when the environment goes, and what an object's death runs.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi_rwk::abi::napi_env;
use rts_napi_rwk::cleanup::napi_async_cleanup_hook_handle;
use rts_napi_rwk::{Env, cleanup, env, finalizers, handles, napi_status, objects};

thread_local! {
    /// Every hook that ran, in order. Thread-local for the reason `wrap.rs`
    /// states: cargo runs these on several threads.
    static RAN: core::cell::RefCell<Vec<usize>> = const { core::cell::RefCell::new(Vec::new()) };
}

/// Records that it ran, by the word it was given.
///
/// # Safety
///
/// Called by this crate with the handle and data the test registered.
unsafe extern "C" fn note(_handle: napi_async_cleanup_hook_handle, data: *mut c_void) {
    RAN.with_borrow_mut(|ran| ran.push(data as usize));
}

/// Records that it ran, and then removes itself — which is what the ABI asks
/// an async hook to do, and the ordering that would double-free a naive
/// implementation.
///
/// # Safety
///
/// As [`note`].
unsafe extern "C" fn note_and_remove(
    handle: napi_async_cleanup_hook_handle,
    data: *mut c_void,
) {
    RAN.with_borrow_mut(|ran| ran.push(data as usize));
    // SAFETY: the handle this hook was handed, which is its own.
    unsafe { cleanup::napi_remove_async_cleanup_hook(handle) };
}

/// A finalizer that records the pointer it was handed.
///
/// # Safety
///
/// Called by this crate with the words the test registered.
unsafe extern "C" fn note_finalized(_env: napi_env, data: *mut c_void, _hint: *mut c_void) {
    RAN.with_borrow_mut(|ran| ran.push(data as usize));
}

#[test]
fn a_cleanup_hook_runs_when_the_environment_is_torn_down() {
    in_a_program(|| {
        RAN.with_borrow_mut(|ran| ran.clear());
        let raw = Env::new().into_raw();
        // SAFETY: live env, a local out-parameter.
        let status = unsafe {
            cleanup::napi_add_async_cleanup_hook(raw, Some(note), 11 as *mut c_void, core::ptr::null_mut())
        };
        assert_eq!(status, napi_status::napi_ok);
        assert!(RAN.with_borrow(Vec::is_empty));

        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        assert_eq!(RAN.with_borrow(|ran| ran.clone()), vec![11]);
    });
}

#[test]
fn a_removed_hook_does_not_run() {
    in_a_program(|| {
        RAN.with_borrow_mut(|ran| ran.clear());
        let raw = Env::new().into_raw();
        let mut handle = napi_async_cleanup_hook_handle(core::ptr::null_mut());
        // SAFETY: live env, a local out-parameter.
        unsafe {
            cleanup::napi_add_async_cleanup_hook(raw, Some(note), 22 as *mut c_void, &mut handle)
        };
        // SAFETY: the handle just produced, not yet removed.
        let status = unsafe { cleanup::napi_remove_async_cleanup_hook(handle) };
        assert_eq!(status, napi_status::napi_ok);

        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        assert!(RAN.with_borrow(Vec::is_empty));
    });
}

#[test]
fn a_hook_that_removes_itself_while_running_is_not_freed_twice() {
    in_a_program(|| {
        // The ordering the ABI positively invites — the hook is handed its own
        // handle so it can report completion — and the one that turns two
        // plausible `Box::from_raw` calls into a double free. It runs to a
        // conclusion or the process dies; there is no third outcome to assert.
        RAN.with_borrow_mut(|ran| ran.clear());
        let raw = Env::new().into_raw();
        // SAFETY: live env, no out-parameter wanted.
        unsafe {
            cleanup::napi_add_async_cleanup_hook(
                raw,
                Some(note_and_remove),
                33 as *mut c_void,
                core::ptr::null_mut(),
            )
        };
        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        assert_eq!(RAN.with_borrow(|ran| ran.clone()), vec![33]);
    });
}

#[test]
fn hooks_run_in_the_order_they_were_added() {
    in_a_program(|| {
        RAN.with_borrow_mut(|ran| ran.clear());
        let raw = Env::new().into_raw();
        // SAFETY: live env, no out-parameters wanted.
        unsafe {
            cleanup::napi_add_async_cleanup_hook(raw, Some(note), 1 as *mut c_void, core::ptr::null_mut());
            cleanup::napi_add_async_cleanup_hook(raw, Some(note), 2 as *mut c_void, core::ptr::null_mut());
        }
        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        assert_eq!(RAN.with_borrow(|ran| ran.clone()), vec![1, 2]);
    });
}

#[test]
fn an_added_finalizer_runs_at_teardown_and_does_not_claim_the_object() {
    in_a_program(|| {
        RAN.with_borrow_mut(|ran| ran.clear());
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, a local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };

        // Two finalizers on ONE object, which `napi_wrap` refuses and this must
        // not: an addon adds one per resource it hung on the same instance.
        // SAFETY: a handle from the open scope, no `napi_ref` wanted.
        let status = unsafe {
            finalizers::napi_add_finalizer(
                raw,
                object,
                44 as *mut c_void,
                Some(note_finalized),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        // SAFETY: as above.
        unsafe {
            finalizers::napi_add_finalizer(
                raw,
                object,
                55 as *mut c_void,
                Some(note_finalized),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };

        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        let ran = RAN.with_borrow(|ran| ran.clone());
        assert!(ran.contains(&44) && ran.contains(&55), "{ran:?}");
    });
}
