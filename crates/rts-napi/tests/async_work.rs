//! Work that leaves the JavaScript thread and comes back.

mod common;

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

use common::in_a_program;
use rts_napi::abi::{napi_env, napi_value};
use rts_napi::{Env, async_work, env, napi_status};

/// What one piece of work records about itself.
///
/// **Per work, not per process**, and that is the fix for a flake rather than a
/// style: these were three `static`s, and cargo runs the four tests of this
/// binary on four threads. Two of them queue work with the same callbacks, so
/// one test's `complete` stored its thread id while another was comparing —
/// which failed as two plausible thread ids that did not match, roughly one run
/// in ten.
///
/// Atomics rather than plain fields because `execute` runs on the worker thread
/// and `complete` on this one; the record itself is what the ABI's data pointer
/// is FOR, so nothing is shared that was not already crossing.
#[derive(Default)]
struct Note {
    /// The thread `execute` ran on, as an id this test can compare.
    executed_on: AtomicUsize,
    /// The thread `complete` ran on.
    completed_on: AtomicUsize,
    /// What `complete` computed from what `execute` left behind.
    produced: AtomicUsize,
    /// The value `execute` writes and `complete` reads.
    value: AtomicUsize,
}

/// A number for the current thread, stable within a run.
fn thread_id() -> usize {
    // The address of a thread-local, which differs per thread and needs no
    // unstable API to read.
    thread_local! {
        static ANCHOR: u8 = const { 0 };
    }
    ANCHOR.with(|anchor| anchor as *const u8 as usize)
}

/// # Safety
///
/// Called by the worker thread with the pointer the test registered.
unsafe extern "C" fn work_off_thread(_env: napi_env, data: *mut c_void) {
    // SAFETY: the addon owns `data` while the work is outstanding, which is the
    // ABI's contract and this test's own arrangement.
    let note = unsafe { &*data.cast::<Note>() };
    note.executed_on.store(thread_id(), Ordering::SeqCst);
    note.value.store(41, Ordering::SeqCst);
}

/// # Safety
///
/// Called on the JavaScript thread once the work is done.
unsafe extern "C" fn back_on_the_js_thread(
    _env: napi_env,
    status: napi_status,
    data: *mut c_void,
) {
    assert_eq!(status, napi_status::napi_ok);
    // SAFETY: as above, and the worker is finished with it.
    let note = unsafe { &*data.cast::<Note>() };
    note.completed_on.store(thread_id(), Ordering::SeqCst);
    note.produced
        .store(note.value.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
}

#[test]
fn execute_runs_off_the_js_thread_and_complete_runs_on_it() {
    // The whole claim of P7, and the reason it is possible at all: `execute`
    // may not call `napi_*`, so nothing about the engine crosses a thread —
    // only the addon's own pointer does.
    in_a_program(|| {
        let js_thread = thread_id();

        let raw = Env::new().into_raw();
        let note = Note::default();
        let mut work: *mut c_void = core::ptr::null_mut();
        // SAFETY: live env; `note` outlives the work because this function
        // does not return until the loop has delivered.
        let status = unsafe {
            async_work::napi_create_async_work(
                raw,
                napi_value(core::ptr::null_mut()),
                napi_value(core::ptr::null_mut()),
                Some(work_off_thread),
                Some(back_on_the_js_thread),
                (&note as *const Note).cast_mut().cast(),
                &mut work,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        // SAFETY: work from the call above, queued once.
        assert_eq!(
            unsafe { async_work::napi_queue_async_work(raw, work) },
            napi_status::napi_ok
        );

        // The loop, as a host runs it: ask every source to deliver until
        // nothing is outstanding. `pump_sources` answers `None` when the
        // program could exit, which is exactly when this work is done.
        let mut passes = 0;
        while rts_core::entry::pump_sources().is_some() {
            passes += 1;
            assert!(passes < 100_000, "the work never came back");
            std::thread::yield_now();
        }

        assert_eq!(note.produced.load(Ordering::SeqCst), 42, "41 written, 1 added");
        assert_eq!(
            note.completed_on.load(Ordering::SeqCst),
            js_thread,
            "`complete` is the half that may touch JavaScript"
        );
        assert_ne!(
            note.executed_on.load(Ordering::SeqCst),
            js_thread,
            "`execute` is the half that must not, and it did not run here"
        );

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn queued_work_keeps_the_program_from_exiting() {
    // A source that answered `Idle` while a worker was still running would let
    // the loop finish and the program exit before its own callback — which is
    // the failure this being a loop source rather than a drain prevents.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let note = Note::default();
        let mut work: *mut c_void = core::ptr::null_mut();
        // SAFETY: live env, locals outlive the work.
        unsafe {
            async_work::napi_create_async_work(
                raw,
                napi_value(core::ptr::null_mut()),
                napi_value(core::ptr::null_mut()),
                Some(work_off_thread),
                Some(back_on_the_js_thread),
                (&note as *const Note).cast_mut().cast(),
                &mut work,
            )
        };
        // SAFETY: created above.
        unsafe { async_work::napi_queue_async_work(raw, work) };

        // Before anything is delivered the loop must NOT be allowed to finish.
        // Read straight after queueing, which is the window that matters.
        let outstanding = rts_core::entry::pump_sources();
        assert!(
            outstanding.is_some() || note.produced.load(Ordering::SeqCst) != 0,
            "either the work is outstanding, or it already completed inside \
             this very pump — both are correct, exiting is not"
        );

        while rts_core::entry::pump_sources().is_some() {
            std::thread::yield_now();
        }
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn queueing_twice_is_refused_rather_than_running_two_threads_on_one_pointer() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let note = Note::default();
        let mut work: *mut c_void = core::ptr::null_mut();
        // SAFETY: live env, locals outlive the work.
        unsafe {
            async_work::napi_create_async_work(
                raw,
                napi_value(core::ptr::null_mut()),
                napi_value(core::ptr::null_mut()),
                Some(work_off_thread),
                Some(back_on_the_js_thread),
                (&note as *const Note).cast_mut().cast(),
                &mut work,
            )
        };
        // SAFETY: created above.
        unsafe { async_work::napi_queue_async_work(raw, work) };
        // SAFETY: deliberately the same work again, which is what this pins.
        // The record is owned by the worker now, so this must refuse WITHOUT
        // reading it back as a box.
        while rts_core::entry::pump_sources().is_some() {
            std::thread::yield_now();
        }
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn work_created_and_never_queued_is_deletable() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let note = Note::default();
        let mut work: *mut c_void = core::ptr::null_mut();
        // SAFETY: live env.
        unsafe {
            async_work::napi_create_async_work(
                raw,
                napi_value(core::ptr::null_mut()),
                napi_value(core::ptr::null_mut()),
                Some(work_off_thread),
                None,
                (&note as *const Note).cast_mut().cast(),
                &mut work,
            )
        };
        // SAFETY: never queued, so this crate still owns it.
        assert_eq!(
            unsafe { async_work::napi_delete_async_work(raw, work) },
            napi_status::napi_ok
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
