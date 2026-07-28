//! Mutable-closure cells (#195).
//!
//! A "cell" is a 1-slot mutable heap box (reusing `Entry::Vec` with one
//! element) that backs a captured-AND-mutated local. The cell HANDLE is
//! captured by value into closures (the existing REIFY_CAPTURED machinery),
//! so every closure that captured the variable reads/writes the SAME cell —
//! mutations are shared, the classic env-record semantics. Reads/writes go
//! through CELL_GET / CELL_SET; the declaration becomes CELL_NEW(init). The
//! stored value is the raw i64 codegen uses for that variable (int / handle /
//! f64-bits), round-tripped verbatim.

use crate::heap::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CELL_NEW(value: i64) -> u64 {
    alloc_entry(Entry::Vec(Box::new(vec![value])))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CELL_GET(cell: u64) -> i64 {
    with_entry(cell, |e| match e {
        Some(Entry::Vec(v)) => v.first().copied().unwrap_or(0),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CELL_SET(cell: u64, value: i64) {
    with_entry_mut(cell, |e| {
        if let Some(Entry::Vec(v)) = e {
            if v.is_empty() {
                v.push(value);
            } else {
                v[0] = value;
            }
        }
    });
}
