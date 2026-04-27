//! Namespace implementations exposed through the new ABI.
//!
//! Each submodule registers an `abi::SPEC` consumed by codegen via
//! [`crate::abi::SPECS`]. No legacy dispatch path remains: every callee is
//! resolved to a canonical `__RTS_*` extern "C" symbol and called directly.

pub mod alloc;
pub mod atomic;
pub mod trace;
pub mod bigfloat;
pub mod buffer;
pub mod collections;
pub mod crypto;
pub mod env;
pub mod fmt;
pub mod ffi;
pub mod fs;
pub mod gc;
pub mod hash;
pub mod hint;
pub mod io;
pub mod math;
pub mod mem;
pub mod num;
pub mod os;
pub mod path;
pub mod process;
pub mod ptr;
pub mod regex;
pub mod parallel;
pub mod runtime;
pub mod string;
pub mod sync;
pub mod test;
pub mod thread;
pub mod time;
pub mod ui;
