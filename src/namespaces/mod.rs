//! Namespace implementations exposed through the new ABI.
//!
//! Each submodule registers an `abi::SPEC` consumed by codegen via
//! [`crate::abi::SPECS`]. No legacy dispatch path remains: every callee is
//! resolved to a canonical `__RTS_*` extern "C" symbol and called directly.

pub mod bigfloat;
pub mod buffer;
pub mod env;
pub mod fs;
pub mod gc;
pub mod io;
pub mod math;
pub mod os;
pub mod path;
pub mod process;
pub mod string;
pub mod time;
