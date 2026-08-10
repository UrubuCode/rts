//! Calling, in both directions.

mod common;

use common::in_a_program;
use rts_napi_rwk::abi::{napi_callback_info, napi_env, napi_value};
use rts_napi_rwk::{Env, env, functions, handles, napi_status, napi_valuetype, values};

unsafe extern "C" fn plus_data(env: napi_env, info: napi_callback_info) -> napi_value {
    let mut argc: usize = 4;
    let mut argv = [handles::none(); 4];
    let mut this = handles::none();
    let mut data: *mut core::ffi::c_void = core::ptr::null_mut();
    // SAFETY: the out-parameters are locals and `argc` says four.
    let status = unsafe {
        functions::napi_get_cb_info(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            &mut this,
            &mut data,
        )
    };
    assert_eq!(status, napi_status::napi_ok);
    assert_eq!(argc, 1, "one argument was passed, not four");

    let mut first = 0.0;
    // SAFETY: a handle from the call's own scope.
    unsafe { values::napi_get_value_double(env, argv[0], &mut first) };
    // SAFETY: `data` is the pointer the test registered, to a live `f64`.
    let added = unsafe { *data.cast::<f64>() };

    let mut answer = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe { values::napi_create_double(env, first + added, &mut answer) };
    answer
}

#[test]
fn a_program_calling_an_addon_function_gets_its_arguments_and_its_data() {
    // Both directions in one: the addon registers a function, the test
    // calls it the way a program would (`call_with_args`), and the callback
    // reads argc/argv/data and answers a value that comes back out.
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut added = 10.0f64;
        let mut function = handles::none();
        // SAFETY: live env; `added` outlives the call below.
        let status = unsafe {
            functions::napi_create_function(
                env,
                core::ptr::null(),
                0,
                Some(plus_data),
                (&mut added as *mut f64).cast(),
                &mut function,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut argument = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(env, 5.0, &mut argument) };
        let mut answer = handles::none();
        let argv = [argument];
        // SAFETY: one handle from the open scope.
        let status = unsafe {
            functions::napi_call_function(
                env,
                handles::none(),
                function,
                1,
                argv.as_ptr(),
                &mut answer,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut number = 0.0;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_get_value_double(env, answer, &mut number) };
        assert_eq!(number, 15.0, "5 from the argument, 10 from `data`");
        // SAFETY: from `into_raw`, dropped once.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn two_addon_functions_are_told_apart_by_their_environment() {
    // The P3 decision. One trampoline stands in for every addon function,
    // so if identity did not travel in the environment, the second
    // registration would answer the first one's `data`.
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut first_data = 1.0f64;
        let mut second_data = 100.0f64;
        let mut first = handles::none();
        let mut second = handles::none();
        // SAFETY: live env, locals outlive the calls.
        unsafe {
            functions::napi_create_function(
                env,
                core::ptr::null(),
                0,
                Some(plus_data),
                (&mut first_data as *mut f64).cast(),
                &mut first,
            );
            functions::napi_create_function(
                env,
                core::ptr::null(),
                0,
                Some(plus_data),
                (&mut second_data as *mut f64).cast(),
                &mut second,
            );
        }

        let mut zero = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(env, 0.0, &mut zero) };
        let argv = [zero];

        let mut answer = handles::none();
        // SAFETY: handles from the open scope.
        unsafe {
            functions::napi_call_function(
                env,
                handles::none(),
                second,
                1,
                argv.as_ptr(),
                &mut answer,
            )
        };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(env, answer, &mut number) };
        assert_eq!(number, 100.0, "the SECOND registration's data");
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn calling_something_that_is_not_a_function_is_refused_by_name() {
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(env, 1.0, &mut number) };
        let mut answer = handles::none();
        // SAFETY: handles from the open scope.
        let status = unsafe {
            functions::napi_call_function(
                env,
                handles::none(),
                number,
                0,
                core::ptr::null(),
                &mut answer,
            )
        };
        assert_eq!(
            status,
            napi_status::napi_function_expected,
            "the ABI has a status for this one too"
        );
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn an_addon_function_is_callable_and_says_so() {
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut data = 0.0f64;
        let mut function = handles::none();
        // SAFETY: live env, local outlives the call.
        unsafe {
            functions::napi_create_function(
                env,
                core::ptr::null(),
                0,
                Some(plus_data),
                (&mut data as *mut f64).cast(),
                &mut function,
            )
        };
        let mut callable = false;
        // SAFETY: a handle from the open scope.
        unsafe { functions::napi_is_callable(env, function, &mut callable) };
        assert!(callable);

        let mut kind = napi_valuetype::napi_undefined;
        // SAFETY: same.
        unsafe { values::napi_typeof(env, function, &mut kind) };
        assert_eq!(
            kind,
            napi_valuetype::napi_function,
            "and `typeof` agrees, which is what a program would see"
        );
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

