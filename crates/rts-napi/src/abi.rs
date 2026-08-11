//! The types the ABI dictates.
//!
//! Every layout, every discriminant and every name in this file comes from
//! `js_native_api_types.h`. A compiled `.node` knows them by heart, so none of
//! it is ours to improve — rule 1 of this crate's README, and the reason the
//! identifiers are snake case.
//!
//! **Never reorder an enum.** The numeric value is the contract: an addon that
//! compares against `napi_ok` compares against `0`, and one that switches on
//! `napi_string_expected` switches on `3`. Inserting in the middle renumbers
//! everything after it, which no compiler here can see and no addon survives.

use core::ffi::c_void;

/// An opaque value handle.
///
/// Pointer-shaped because the ABI says so, and a pointer to a SLOT rather than
/// an encoded value — see [`crate::handles`] and rule 2 of the README. The
/// addon never dereferences it.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct napi_value(pub *mut c_void);

/// The opaque context every entry point takes.
///
/// One per addon instance, pointing at a [`crate::env::Env`]. Not valid on
/// another thread and not valid after the addon is unloaded.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_env(pub *mut c_void);

/// An opaque handle scope. See [`crate::handles`].
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_handle_scope(pub *mut c_void);

/// An opaque escapable handle scope.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_escapable_handle_scope(pub *mut c_void);

/// A reference that outlives the scope that made it. P4.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_ref(pub *mut c_void);

/// What a callback is told about its call: argc, argv, `this`, data. P3.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_callback_info(pub *mut c_void);

/// What every entry point answers. **The order is the ABI.**
///
/// The variants carry no doc comments and `missing_docs` is waived for this
/// enum alone: each name is a sentence already (`napi_string_expected` says a
/// string was expected), and 24 comments restating 24 names is the kind of
/// documentation this repository's rule 7 calls worthless. What is worth
/// saying is said above.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_status {
    napi_ok = 0,
    napi_invalid_arg,
    napi_object_expected,
    napi_string_expected,
    napi_name_expected,
    napi_function_expected,
    napi_number_expected,
    napi_boolean_expected,
    napi_array_expected,
    napi_generic_failure,
    napi_pending_exception,
    napi_cancelled,
    napi_escape_called_twice,
    napi_handle_scope_mismatch,
    napi_callback_scope_mismatch,
    napi_queue_full,
    napi_closing,
    napi_bigint_expected,
    napi_date_expected,
    napi_arraybuffer_expected,
    napi_detachable_arraybuffer_expected,
    napi_would_deadlock,
    napi_no_external_buffers_allowed,
    napi_cannot_run_js,
}

/// What `napi_typeof` answers. **The order is the ABI.**
///
/// Variants undocumented for the reason [`napi_status`] states.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_valuetype {
    napi_undefined = 0,
    napi_null,
    napi_boolean,
    napi_number,
    napi_string,
    napi_symbol,
    napi_object,
    napi_function,
    napi_external,
    napi_bigint,
}

/// "Measure it yourself" for a string length, spelled `(size_t)-1`.
pub const NAPI_AUTO_LENGTH: usize = usize::MAX;

/// A native function an addon registers. P3.
pub type napi_callback =
    Option<unsafe extern "C" fn(env: napi_env, info: napi_callback_info) -> napi_value>;

/// What runs when a wrapped value is collected. P6.
pub type napi_finalize = Option<
    unsafe extern "C" fn(env: napi_env, finalize_data: *mut c_void, finalize_hint: *mut c_void),
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The discriminants are the contract, so they are asserted rather than
    /// trusted to the order of the lines above — a merge that reorders one line
    /// is otherwise invisible until an addon takes the wrong branch.
    #[test]
    fn the_status_numbering_is_the_one_the_header_publishes() {
        assert_eq!(napi_status::napi_ok as i32, 0);
        assert_eq!(napi_status::napi_invalid_arg as i32, 1);
        assert_eq!(napi_status::napi_string_expected as i32, 3);
        assert_eq!(napi_status::napi_generic_failure as i32, 9);
        assert_eq!(napi_status::napi_pending_exception as i32, 10);
        assert_eq!(napi_status::napi_cannot_run_js as i32, 23);
    }

    #[test]
    fn the_valuetype_numbering_is_the_one_the_header_publishes() {
        assert_eq!(napi_valuetype::napi_undefined as i32, 0);
        assert_eq!(napi_valuetype::napi_number as i32, 3);
        assert_eq!(napi_valuetype::napi_object as i32, 6);
        assert_eq!(napi_valuetype::napi_bigint as i32, 9);
    }

    #[test]
    fn a_handle_is_exactly_a_pointer_wide() {
        // `#[repr(transparent)]` over a pointer, because the addon passes these
        // by value in registers the C ABI assigns by size.
        assert_eq!(
            core::mem::size_of::<napi_value>(),
            core::mem::size_of::<*mut c_void>()
        );
        assert_eq!(
            core::mem::size_of::<napi_env>(),
            core::mem::size_of::<*mut c_void>()
        );
    }
}
