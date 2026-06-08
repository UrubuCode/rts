//! `bigfloat` namespace — i128 decimal fixed-point (scale ≤ 36).
//!
//! Handle-based API backed by `FixedDecimal` (`fixed` submodule). Values live in
//! the shared GC handle table as `Entry::BigFixed`; callers allocate with
//! zero/from_*, operate via named methods, and eventually `free` the handle.
//! Algorithms like Machin's π compose these primitives in user-space TS.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

pub mod fixed;

use rts_abi::ty::{F64, Handle, I64, U64};
use rts_macro::rts_namespace;

use fixed::FixedDecimal;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn alloc(value: FixedDecimal) -> u64 {
    alloc_entry(Entry::BigFixed(Box::new(value)))
}

fn clone_of(handle: u64) -> Option<FixedDecimal> {
    with_entry(handle, |entry| match entry {
        Some(Entry::BigFixed(b)) => Some(b.as_ref().clone()),
        _ => None,
    })
}

/// i128 decimal fixed-point arithmetic (scale ≤ 36), handle-based.
#[rts_namespace(bigfloat)]
impl BigFloatNs {
    /// Zero with `precision` decimal digits (clamped 1..=36). Handle.
    #[rts_fn]
    pub fn zero(precision: I64) -> U64 {
        let scale = precision.max(1).min(36) as u32;
        alloc(FixedDecimal::zero(scale))
    }

    /// From an f64 with `precision` decimal digits. Handle.
    #[rts_fn]
    pub fn from_f64(x: F64, precision: I64) -> U64 {
        let scale = precision.max(1).min(36) as u32;
        alloc(FixedDecimal::from_f64(x, scale))
    }

    /// Parse a decimal string with `precision` digits. Handle, 0 on error.
    #[rts_fn]
    pub fn from_str(s: Str, precision: I64) -> U64 {
        let scale = precision.max(1).min(36) as u32;
        match FixedDecimal::from_str(s, scale) {
            Some(v) => alloc(v),
            None => 0,
        }
    }

    /// From an i64 with `precision` decimal digits. Handle.
    #[rts_fn]
    pub fn from_i64(x: I64, precision: I64) -> U64 {
        let scale = precision.max(1).min(36) as u32;
        alloc(FixedDecimal::from_i64(x, scale))
    }

    /// Convert to f64 (NaN if handle invalid).
    #[rts_fn]
    pub fn to_f64(h: U64) -> F64 {
        clone_of(h).map(|v| v.to_f64()).unwrap_or(f64::NAN)
    }

    /// Decimal string (string handle; "NaN" if handle invalid).
    #[rts_fn(ts = "to_string(h: number): string")]
    pub fn to_string(h: U64) -> Handle {
        let s = clone_of(h)
            .map(|v| v.to_string_decimal())
            .unwrap_or_else(|| "NaN".to_string());
        unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
    }

    /// a + b. New handle, 0 if either invalid.
    #[rts_fn]
    pub fn add(a: U64, b: U64) -> U64 {
        let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
            return 0;
        };
        alloc(l.add(&r))
    }

    /// a - b. New handle, 0 if either invalid.
    #[rts_fn]
    pub fn sub(a: U64, b: U64) -> U64 {
        let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
            return 0;
        };
        alloc(l.sub(&r))
    }

    /// a * b. New handle, 0 if either invalid.
    #[rts_fn]
    pub fn mul(a: U64, b: U64) -> U64 {
        let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
            return 0;
        };
        alloc(l.mul(&r))
    }

    /// a / b. New handle, 0 if either invalid or division fails.
    #[rts_fn]
    pub fn div(a: U64, b: U64) -> U64 {
        let (Some(l), Some(r)) = (clone_of(a), clone_of(b)) else {
            return 0;
        };
        match l.div(&r) {
            Some(v) => alloc(v),
            None => 0,
        }
    }

    /// -a. New handle, 0 if invalid.
    #[rts_fn]
    pub fn neg(a: U64) -> U64 {
        let Some(v) = clone_of(a) else { return 0 };
        alloc(v.neg())
    }

    /// sqrt(a). New handle, 0 if invalid or negative.
    #[rts_fn]
    pub fn sqrt(a: U64) -> U64 {
        let Some(v) = clone_of(a) else { return 0 };
        match v.sqrt() {
            Some(r) => alloc(r),
            None => 0,
        }
    }

    /// Releases the handle.
    #[rts_fn]
    pub fn free(h: U64) {
        free_handle(h);
    }
}
