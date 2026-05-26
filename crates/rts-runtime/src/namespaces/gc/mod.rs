//! `gc` namespace — runtime value storage.
//!
//! Owns the handle table that backs dynamically allocated strings (and,
//! eventually, objects, arrays, buffers). Every handle-returning function
//! across the codebase ultimately calls into this module's tables.

pub mod abi;
pub mod class_registry;
pub mod closure;
pub mod collector;
pub mod debug;
pub mod env;
pub mod error;
pub mod generator;
pub mod global_roots;
pub mod handles;
pub mod instance;
pub mod promise_slot;
pub mod stack;
pub mod stack_map_registry;
pub mod string_pool;
pub mod tagged_raw;
pub mod this_slot;
pub mod thread_registry;
