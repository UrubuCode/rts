//! ABI type aliases for the `#[rts_namespace]` / `#[rts_fn]` macros.
//!
//! These are plain Rust repr aliases — `Handle` and `U64` are both `u64`, but
//! the *written token* drives the derived `AbiType` (and thus the TS type).
//! Author intent travels in the name: write `Handle` for a runtime handle,
//! `U64` for a raw count/pointer, even though both lower to `u64`.
//!
//! The macro string-matches the last path segment of each parameter/return
//! type against this set:
//!
//! | token            | `AbiType` | TS        | repr  |
//! |------------------|-----------|-----------|-------|
//! | `Handle`         | Handle    | number    | u64   |
//! | `U64`            | U64       | number    | u64   |
//! | `I64` / `i64`    | I64       | number    | i64   |
//! | `I32` / `i32`    | I32       | number    | i32   |
//! | `F64` / `f64`    | F64       | number    | f64   |
//! | `Bool` / `bool`  | Bool      | boolean   | i8    |
//! | `Str`            | StrPtr    | string    | ptr+len (expanded by macro) |
//! | `()` / none      | Void      | void      | —     |

/// Runtime resource handle (`HandleTable` u64: gen + slot).
pub type Handle = u64;
/// Raw unsigned 64-bit value (counts, sizes, opaque pointers).
pub type U64 = u64;
/// Native signed 64-bit integer.
pub type I64 = i64;
/// Native signed 32-bit integer.
pub type I32 = i32;
/// Native f64.
pub type F64 = f64;
/// Boolean at the extern "C" boundary (0/1 in an i8).
pub type Bool = i8;
