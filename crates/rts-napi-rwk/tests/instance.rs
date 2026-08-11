//! The pointer an addon keeps per environment, and its teardown.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi_rwk::abi::napi_env;
use rts_napi_rwk::{Env, env, instance, napi_status};

thread_local! {
    /// What [`note_freed`] saw. Thread-local for the reason `wrap.rs` states:
    /// cargo runs these on several threads and a shared cell reads another
    /// test's pointer.
    static FREED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Records the pointer it was handed.
///
/// # Safety
///
/// Called by this crate with the words the test registered.
unsafe extern "C" fn note_freed(_env: napi_env, data: *mut c_void, _hint: *mut c_void) {
    FREED.set(data as usize);
}

#[test]
fn the_pointer_set_is_the_pointer_returned() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut owned = 99u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        // SAFETY: live env; `owned` outlives the environment.
        let status =
            unsafe { instance::napi_set_instance_data(raw, pointer, None, core::ptr::null_mut()) };
        assert_eq!(status, napi_status::napi_ok);

        let mut back = core::ptr::null_mut();
        // SAFETY: live env, local out-parameter.
        let status = unsafe { instance::napi_get_instance_data(raw, &mut back) };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(back, pointer);
        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn two_environments_do_not_share_one_pointer() {
    in_a_program(|| {
        // The failure this pins is the old crate's: a process-wide cell, which
        // answers correctly for one addon and hands the second the first's
        // pointer. Two environments in one process is the ordinary case.
        let first = Env::new().into_raw();
        let second = Env::new().into_raw();
        let mut one = 1u64;
        let mut two = 2u64;
        let a = (&mut one as *mut u64).cast::<c_void>();
        let b = (&mut two as *mut u64).cast::<c_void>();
        // SAFETY: two live environments; both locals outlive them.
        unsafe {
            instance::napi_set_instance_data(first, a, None, core::ptr::null_mut());
            instance::napi_set_instance_data(second, b, None, core::ptr::null_mut());
        }

        let mut back = core::ptr::null_mut();
        // SAFETY: live env, local out-parameter.
        unsafe { instance::napi_get_instance_data(first, &mut back) };
        assert_eq!(back, a);
        // SAFETY: live env, local out-parameter.
        unsafe { instance::napi_get_instance_data(second, &mut back) };
        assert_eq!(back, b);
        // SAFETY: environments this test made and has not destroyed.
        unsafe {
            env::destroy(first);
            env::destroy(second);
        }
    });
}

#[test]
fn an_environment_with_nothing_set_answers_null_and_not_a_failure() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut back = (&mut 0u8 as *mut u8).cast::<c_void>();
        // SAFETY: live env, local out-parameter.
        let status = unsafe { instance::napi_get_instance_data(raw, &mut back) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(back.is_null());
        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn tearing_the_environment_down_runs_the_finalizer_once() {
    in_a_program(|| {
        FREED.set(0);
        let raw = Env::new().into_raw();
        let mut owned = 7u64;
        let pointer = (&mut owned as *mut u64).cast::<c_void>();
        // SAFETY: live env; `owned` outlives the environment.
        unsafe {
            instance::napi_set_instance_data(
                raw,
                pointer,
                Some(note_freed),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(FREED.get(), 0);
        // SAFETY: an environment this test made and has not destroyed.
        unsafe { env::destroy(raw) };
        assert_eq!(FREED.get(), pointer as usize);
    });
}
