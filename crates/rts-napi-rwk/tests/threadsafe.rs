//! Calling a JavaScript function from a thread that has none.

mod common;

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

use common::in_a_program;
use rts_napi_rwk::abi::{napi_callback_info, napi_env, napi_value};
use rts_napi_rwk::threadsafe::{
    napi_threadsafe_function, napi_threadsafe_function_call_mode,
    napi_threadsafe_function_release_mode,
};
use rts_napi_rwk::{Env, env, functions, handles, napi_status, threadsafe, values};

/// How many times the JavaScript-side callback ran.
static CALLED: AtomicUsize = AtomicUsize::new(0);
/// The sum of the `data` words it was handed.
static SUM: AtomicUsize = AtomicUsize::new(0);
/// The thread the JavaScript-side callback ran on.
static CALLED_ON: AtomicUsize = AtomicUsize::new(0);

fn thread_id() -> usize {
    thread_local! {
        static ANCHOR: u8 = const { 0 };
    }
    ANCHOR.with(|anchor| anchor as *const u8 as usize)
}

/// The addon's JavaScript-side bridge: it is handed the function and the data,
/// and decides what to call with what.
///
/// # Safety
///
/// Called on the JavaScript thread by this crate.
unsafe extern "C" fn bridge(
    env: napi_env,
    js_callback: napi_value,
    _context: *mut c_void,
    data: *mut c_void,
) {
    CALLED.fetch_add(1, Ordering::SeqCst);
    SUM.fetch_add(data as usize, Ordering::SeqCst);
    CALLED_ON.store(thread_id(), Ordering::SeqCst);

    // And it really is a callable handle: call it, the way an addon would.
    let mut answer = handles::none();
    // SAFETY: a handle from the call's own scope, live env.
    unsafe {
        functions::napi_call_function(
            env,
            handles::none(),
            js_callback,
            0,
            core::ptr::null(),
            &mut answer,
        )
    };
}

/// A JavaScript function, as an addon-registered native.
///
/// # Safety
///
/// Called by the engine.
unsafe extern "C" fn the_js_function(env: napi_env, _info: napi_callback_info) -> napi_value {
    RAN_JS.fetch_add(1, Ordering::SeqCst);
    let mut answer = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe { values::napi_get_undefined(env, &mut answer) };
    answer
}

/// How many times the JavaScript function itself ran.
static RAN_JS: AtomicUsize = AtomicUsize::new(0);

/// Creates a threadsafe function over a fresh native callable.
///
/// # Safety
///
/// `env` must be live.
unsafe fn make(env: napi_env) -> napi_threadsafe_function {
    let mut function = handles::none();
    // SAFETY: the caller's contract.
    unsafe {
        functions::napi_create_function(
            env,
            core::ptr::null(),
            0,
            Some(the_js_function),
            core::ptr::null_mut(),
            &mut function,
        )
    };
    let mut tsfn = napi_threadsafe_function(core::ptr::null_mut());
    // SAFETY: a handle from the open scope.
    let status = unsafe {
        threadsafe::napi_create_threadsafe_function(
            env,
            function,
            napi_value(core::ptr::null_mut()),
            napi_value(core::ptr::null_mut()),
            0,
            1,
            core::ptr::null_mut(),
            None,
            core::ptr::null_mut(),
            Some(bridge),
            &mut tsfn,
        )
    };
    assert_eq!(status, napi_status::napi_ok);
    tsfn
}

#[test]
fn another_thread_asks_and_the_js_thread_calls() {
    // The claim: the call is MADE on the JavaScript thread, however far away it
    // was asked for. Two worker threads ask three times between them.
    in_a_program(|| {
        CALLED.store(0, Ordering::SeqCst);
        SUM.store(0, Ordering::SeqCst);
        RAN_JS.store(0, Ordering::SeqCst);
        let js_thread = thread_id();

        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let tsfn = unsafe { make(raw) };

        let workers: Vec<_> = [10usize, 20, 30]
            .into_iter()
            .map(|word| {
                let handle = tsfn.0 as usize;
                std::thread::spawn(move || {
                    let tsfn = napi_threadsafe_function(handle as *mut c_void);
                    // SAFETY: the function is live for the whole test, and the
                    // ABI's own contract is that this is callable from any
                    // thread — which is the thing being tested.
                    unsafe {
                        threadsafe::napi_acquire_threadsafe_function(tsfn);
                        threadsafe::napi_call_threadsafe_function(
                            tsfn,
                            word as *mut c_void,
                            napi_threadsafe_function_call_mode::napi_tsfn_nonblocking,
                        );
                        threadsafe::napi_release_threadsafe_function(
                            tsfn,
                            napi_threadsafe_function_release_mode::napi_tsfn_release,
                        );
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("a worker thread");
        }

        // The loop, as a host runs it.
        let mut passes = 0;
        while CALLED.load(Ordering::SeqCst) < 3 {
            rts_core::entry::pump_sources();
            passes += 1;
            assert!(passes < 100_000, "the calls never arrived");
            std::thread::yield_now();
        }

        assert_eq!(SUM.load(Ordering::SeqCst), 60, "10 + 20 + 30");
        assert_eq!(
            RAN_JS.load(Ordering::SeqCst),
            3,
            "and the JavaScript function itself ran, three times"
        );
        assert_eq!(
            CALLED_ON.load(Ordering::SeqCst),
            js_thread,
            "on the thread that has a Context, never on the caller's"
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_call_after_the_last_release_is_refused() {
    // `napi_closing` is the ABI's word for it, and an addon branches on it —
    // answering `ok` would promise a call that will never happen.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let tsfn = unsafe { make(raw) };
        // SAFETY: the initial acquisition, released once.
        unsafe {
            threadsafe::napi_release_threadsafe_function(
                tsfn,
                napi_threadsafe_function_release_mode::napi_tsfn_release,
            )
        };
        // SAFETY: deliberately after the last release.
        let status = unsafe {
            threadsafe::napi_call_threadsafe_function(
                tsfn,
                core::ptr::null_mut(),
                napi_threadsafe_function_call_mode::napi_tsfn_nonblocking,
            )
        };
        assert_eq!(status, napi_status::napi_closing);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn an_unreffed_function_does_not_hold_the_program_open() {
    // The difference between the two counts. A thread still holds this one, so
    // it is not finished — but the addon has said it must not keep the process
    // alive, and a loop that ignored that would hang forever.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        let tsfn = unsafe { make(raw) };

        // SAFETY: on the JavaScript thread, as the ABI requires.
        unsafe { threadsafe::napi_unref_threadsafe_function(raw, tsfn) };
        assert!(
            rts_core::entry::pump_sources().is_none(),
            "unreffed, so nothing here keeps the loop running"
        );

        // SAFETY: same.
        unsafe { threadsafe::napi_ref_threadsafe_function(raw, tsfn) };
        assert!(
            rts_core::entry::pump_sources().is_some(),
            "and reffing it back is what an addon does before it expects a call"
        );

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn the_context_pointer_comes_back_unchanged() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut function = handles::none();
        // SAFETY: live env.
        unsafe {
            functions::napi_create_function(
                raw,
                core::ptr::null(),
                0,
                Some(the_js_function),
                core::ptr::null_mut(),
                &mut function,
            )
        };
        let mut owned = 5u64;
        let context = (&mut owned as *mut u64).cast::<c_void>();
        let mut tsfn = napi_threadsafe_function(core::ptr::null_mut());
        // SAFETY: a handle from the open scope.
        unsafe {
            threadsafe::napi_create_threadsafe_function(
                raw,
                function,
                napi_value(core::ptr::null_mut()),
                napi_value(core::ptr::null_mut()),
                0,
                1,
                core::ptr::null_mut(),
                None,
                context,
                Some(bridge),
                &mut tsfn,
            )
        };

        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: a live function.
        assert_eq!(
            unsafe { threadsafe::napi_get_threadsafe_function_context(tsfn, &mut read) },
            napi_status::napi_ok
        );
        assert_eq!(read, context);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
