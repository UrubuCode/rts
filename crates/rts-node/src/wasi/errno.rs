//! WASI preview1 errno — the numbers the spec itself assigns, alphabetical
//! by name. Not independently checked against Node's C++ binding source;
//! see the parent module doc's "Not implemented, by name" closing note.

#![allow(dead_code)]

pub const SUCCESS: i32 = 0;
pub const EBADF: i32 = 8;
pub const EINVAL: i32 = 28;
pub const EIO: i32 = 29;
pub const ENOSYS: i32 = 52;
pub const ENOTCAPABLE: i32 = 76;
