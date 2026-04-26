//! `gc` namespace — runtime value storage.
//!
//! Owns the handle table that backs dynamically allocated strings (and,
//! eventually, objects, arrays, buffers). Every handle-returning function
//! across the codebase ultimately calls into this module's tables.

pub mod abi;
pub mod env;
pub mod error;
pub mod handles;
pub mod instance;
pub mod string_pool;
