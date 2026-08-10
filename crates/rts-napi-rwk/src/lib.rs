//! N-API — the symbols a `.node` addon resolves out of this process.
//!
//! Read `README.md` before changing anything here: the ABI is not ours to
//! design, a `napi_value` is a slot rather than a value, and a failure is a
//! status rather than a panic. `PLAN.md` has the phases and what each one is
//! waiting on.
//!
//! # What works today
//!
//! P1: handle scopes, and the value surface — numbers, booleans, `undefined`,
//! `null`, UTF-8 strings both ways, `napi_typeof`. Everything else in the ABI
//! is absent rather than stubbed; an absent symbol fails to link loudly, which
//! is the answer an addon can act on.
//!
//! # Why the names look like that
//!
//! `napi_status`, `napi_create_double`, `NAPI_AUTO_LENGTH` — snake case, no
//! Rust convention, no attribute deriving them. They are a foreign C interface
//! whose spelling IS the contract, which `CLAUDE.md` names as the one permanent
//! exception to "never hand-write a symbol name".

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![deny(missing_docs)]
#![deny(dead_code)]

pub mod abi;
pub mod env;
pub mod handles;
pub mod values;

pub use abi::{napi_env, napi_status, napi_value, napi_valuetype};
pub use env::Env;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::NAPI_AUTO_LENGTH;
    use rts_core::entry::{Context, with_context};
    use rts_core::value::{Kinds, Singletons};

    /// Runs `body` with a runtime installed, the way a host does before an
    /// addon can be called at all.
    fn in_a_program<T>(body: impl FnOnce() -> T) -> T {
        let context = Context::new(
            Singletons {
                undefined: 0,
                null: 1,
                hole: 2,
            },
            // The numbering `ValueModel::declare` produces for the first program
            // in a process. `Kinds::in_declaration_order` says the same thing and is
            // `#[cfg(test)]` inside its own crate, so it is not reachable from here.
            Kinds {
                symbol: 4,
                bigint: 5,
            },
        );
        let (_, answer) = with_context(context, body);
        answer
    }

    #[test]
    fn a_number_survives_the_round_trip_the_addon_makes() {
        in_a_program(|| {
            let env = Env::new().into_raw();
            let mut handle = handles::none();
            // SAFETY: `env` is live and `handle` is a local.
            let status = unsafe { values::napi_create_double(env, 6.5, &mut handle) };
            assert_eq!(status, napi_status::napi_ok);

            let mut read = 0.0f64;
            // SAFETY: same.
            let status = unsafe { values::napi_get_value_double(env, handle, &mut read) };
            assert_eq!(status, napi_status::napi_ok);
            assert_eq!(read, 6.5);
            // SAFETY: the pointer came from `into_raw` and is dropped once.
            drop(unsafe { Env::from_raw(env) });
        });
    }

    #[test]
    fn a_string_crosses_as_utf8_and_comes_back_as_the_same_text() {
        in_a_program(|| {
            let env = Env::new().into_raw();
            let text = c"olá, mundo";
            let mut handle = handles::none();
            // SAFETY: a NUL-terminated literal, and auto length.
            let status = unsafe {
                values::napi_create_string_utf8(env, text.as_ptr(), NAPI_AUTO_LENGTH, &mut handle)
            };
            assert_eq!(status, napi_status::napi_ok);

            // SAFETY: a handle from the open scope.
            let word = unsafe { handles::value_of(handle) }.expect("a handle names a slot");
            assert_eq!(
                rts_core::entry::text_of(word).as_deref(),
                Some("olá, mundo"),
                "multi-byte UTF-8 has to survive, not just ASCII"
            );
            // SAFETY: as above.
            drop(unsafe { Env::from_raw(env) });
        });
    }

    #[test]
    fn bytes_that_are_not_utf8_are_refused_rather_than_replaced() {
        in_a_program(|| {
            let env = Env::new().into_raw();
            let bytes: [u8; 3] = [0xff, 0xfe, 0x00];
            let mut handle = handles::none();
            // SAFETY: three readable bytes, length given explicitly.
            let status = unsafe {
                values::napi_create_string_utf8(
                    env,
                    bytes.as_ptr().cast(),
                    2,
                    &mut handle,
                )
            };
            assert_eq!(
                status,
                napi_status::napi_string_expected,
                "a string of replacement characters would carry the addon's bug \
                 into the program as data"
            );
            // SAFETY: as above.
            drop(unsafe { Env::from_raw(env) });
        });
    }

    #[test]
    fn typeof_answers_null_where_the_language_says_object() {
        in_a_program(|| {
            let env = Env::new().into_raw();
            let mut handle = handles::none();
            // SAFETY: live env, local out-parameter.
            unsafe { values::napi_get_null(env, &mut handle) };

            let mut kind = napi_valuetype::napi_undefined;
            // SAFETY: same.
            let status = unsafe { values::napi_typeof(env, handle, &mut kind) };
            assert_eq!(status, napi_status::napi_ok);
            assert_eq!(
                kind,
                napi_valuetype::napi_null,
                "the ABI is finer than `typeof` here, and an addon branches on it"
            );
            // SAFETY: as above.
            drop(unsafe { Env::from_raw(env) });
        });
    }

    #[test]
    fn a_null_out_parameter_is_a_status_and_not_a_fault() {
        in_a_program(|| {
            let env = Env::new().into_raw();
            // SAFETY: deliberately a null out-parameter, which is what this
            // pins: the ABI's way of reporting it, not a segfault inside the
            // addon's process.
            let status =
                unsafe { values::napi_create_double(env, 1.0, core::ptr::null_mut()) };
            assert_eq!(status, napi_status::napi_invalid_arg);
            // SAFETY: as above.
            drop(unsafe { Env::from_raw(env) });
        });
    }

    #[test]
    fn every_handle_takes_a_root_and_closing_the_scope_gives_it_back() {
        // The reason `napi_value` is a slot and not a word, checked from the
        // side this crate owns: that a handle IS a root.
        //
        // That an external root survives a collection is proven where the
        // collector is — `rts-core`'s
        // `collect_cycle::a_value_held_from_outside_the_heap_survives_and_stops_when_released`
        // — rather than duplicated here, which would mean exporting the
        // collector's entry point for a test and nothing else.
        in_a_program(|| {
            let mut env = Env::new();
            assert!(env.current().is_empty());
            let _ = env.current().handle(rts_core::entry::make_number(1.0));
            let _ = env.current().handle(rts_core::entry::make_number(2.0));
            assert_eq!(env.current().len(), 2, "one hold per handle");

            env.open();
            assert_eq!(env.depth(), 2);
            let _ = env.current().handle(rts_core::entry::make_number(3.0));
            assert!(env.close(), "an inner scope closes");
            assert_eq!(env.depth(), 1);
            assert!(
                !env.close(),
                "the scope the ABI gave the addon is not the addon's to close"
            );
        });
    }
}
