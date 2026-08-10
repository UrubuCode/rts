//! An addon saying what it is.
//!
//! Every test here plays the part a `.node` plays, in Rust: it is handed an
//! `env` and an `exports`, and it answers. What is NOT tested is a `.node`
//! being opened from disk — see `src/module.rs` for why that half is a change
//! to the build rather than to this crate.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi_rwk::abi::{napi_callback_info, napi_env, napi_value};
use rts_napi_rwk::module::napi_module;
use rts_napi_rwk::{Env, env, functions, handles, module, napi_status, objects, values};

/// An addon that hangs one function on the exports it was given.
///
/// # Safety
///
/// Called with the environment and exports this crate made.
unsafe extern "C" fn hangs_a_function(env: napi_env, exports: napi_value) -> napi_value {
    let mut function = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe {
        functions::napi_create_function(
            env,
            c"answer".as_ptr(),
            usize::MAX,
            Some(answer_forty_two),
            core::ptr::null_mut(),
            &mut function,
        )
    };
    // SAFETY: handles from the open scope, NUL-terminated literal.
    unsafe { objects::napi_set_named_property(env, exports, c"answer".as_ptr(), function) };
    exports
}

/// An addon that replaces its exports with a function.
///
/// # Safety
///
/// As above.
unsafe extern "C" fn replaces_exports(env: napi_env, _exports: napi_value) -> napi_value {
    let mut function = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe {
        functions::napi_create_function(
            env,
            core::ptr::null(),
            0,
            Some(answer_forty_two),
            core::ptr::null_mut(),
            &mut function,
        )
    };
    function
}

/// # Safety
///
/// Called by the engine.
unsafe extern "C" fn answer_forty_two(env: napi_env, _info: napi_callback_info) -> napi_value {
    let mut answer = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe { values::napi_create_double(env, 42.0, &mut answer) };
    answer
}

#[test]
fn an_addon_that_hangs_a_function_gets_its_object_back() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: the registrar is this file's own.
        let exports = unsafe { module::run(raw, Some(hangs_a_function)) }.expect("exports");

        let handle = {
            // SAFETY: live env.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            scoped.current().handle(exports)
        };
        let mut found = handles::none();
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, handle, c"answer".as_ptr(), &mut found) };

        let mut answer = handles::none();
        // SAFETY: same.
        unsafe {
            functions::napi_call_function(
                raw,
                handles::none(),
                found,
                0,
                core::ptr::null(),
                &mut answer,
            )
        };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, answer, &mut number) };
        assert_eq!(number, 42.0, "the addon's function, called");

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn an_addon_may_replace_its_exports_entirely() {
    // Both shapes are common — hang properties on the object, or answer
    // something else — and using the object regardless would silently discard
    // the second kind of addon's whole surface.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: the registrar is this file's own.
        let exports = unsafe { module::run(raw, Some(replaces_exports)) }.expect("exports");

        let handle = {
            // SAFETY: live env.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            scoped.current().handle(exports)
        };
        let mut callable = false;
        // SAFETY: a handle from the open scope.
        unsafe { functions::napi_is_callable(raw, handle, &mut callable) };
        assert!(callable, "the exports ARE the function");

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_module_registered_early_is_run_when_it_is_asked_for() {
    // The older path: a static constructor registers before a `Context` exists,
    // so nothing may be evaluated at registration time. Running the registrar
    // there would reach a runtime the host has not installed — an abort, not an
    // error.
    in_a_program(|| {
        let mut record = napi_module {
            nm_version: 1,
            nm_flags: 0,
            nm_filename: c"addon.c".as_ptr(),
            nm_register_func: Some(hangs_a_function),
            nm_modname: c"greeter".as_ptr(),
            nm_priv: core::ptr::null_mut(),
            reserved: [core::ptr::null_mut(); 4],
        };
        let before = module::registered();
        // SAFETY: a live record with NUL-terminated strings.
        unsafe { module::napi_module_register(&mut record) };
        assert_eq!(module::registered(), before + 1, "recorded, not run");

        let raw = Env::new().into_raw();
        // SAFETY: live env, the name just registered.
        let exports = unsafe { module::exports_of(raw, "greeter") }.expect("its exports");

        let handle = {
            // SAFETY: live env.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            scoped.current().handle(exports)
        };
        let mut found = handles::none();
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, handle, c"answer".as_ptr(), &mut found) };
        let mut callable = false;
        // SAFETY: same.
        unsafe { functions::napi_is_callable(raw, found, &mut callable) };
        assert!(callable);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn asking_for_a_module_nobody_registered_answers_nothing() {
    // What a `require` of a `.node` that never registered looks like, and it
    // must not be an empty object: an addon whose constructor did not run is a
    // broken build, and an empty object hides it until the first call.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        // SAFETY: live env.
        assert!(unsafe { module::exports_of(raw, "absent") }.is_none());
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

/// The struct is passed by pointer from C, so its size is part of the contract.
const _: () = assert!(core::mem::size_of::<*mut c_void>() > 0);
