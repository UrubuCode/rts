//! `bigfloat` namespace — arbitrary-precision floating point.
//!
//! Handle-based API backed by `dashu::float::DBig`. Values live in the shared
//! GC handle table; callers allocate with `new` / `from_f64` / `from_str`,
//! operate via named methods, and eventually `free` the handle.

pub mod abi;
pub mod fixed;
pub mod ops;
