//! What every one of these tests needs before it can call anything.
//!
//! In `tests/common/` rather than repeated per file: cargo builds each test
//! file as its own binary, so a helper written three times would be three
//! helpers that can disagree about what "a program is running" means.

#![allow(dead_code)]

use rts_core::entry::{Context, with_context};
use rts_core::value::{Kinds, Singletons};
use rts_napi_rwk::abi::{NAPI_AUTO_LENGTH, napi_env, napi_value};
use rts_napi_rwk::{handles, napi_status, values};


/// Runs `body` with a runtime installed, the way a host does before an
/// addon can be called at all.
pub fn in_a_program<T>(body: impl FnOnce() -> T) -> T {
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

/// A handle holding `text`, in `env`'s innermost scope.
///
/// The shape an addon writes constantly, so the tests below write it once.
///
/// # Safety
///
/// `env` must be live.
pub unsafe fn string(env: napi_env, text: &core::ffi::CStr) -> napi_value {
    let mut handle = handles::none();
    // SAFETY: the caller's contract, and a NUL-terminated literal.
    let status = unsafe {
        values::napi_create_string_utf8(env, text.as_ptr(), NAPI_AUTO_LENGTH, &mut handle)
    };
    assert_eq!(status, napi_status::napi_ok);
    handle
}

