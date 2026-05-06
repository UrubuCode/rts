//! Namespace implementations — re-exported from rts-runtime.
//!
//! Only `runtime` is declared locally to preserve the `eval_jit` submodule
//! (which depends on the compiler pipeline and cannot live in rts-runtime).

pub use rts_runtime::namespaces::{
    alloc, atomic, bigfloat, buffer, collections, crypto, date, env, events,
    ffi, fmt, fs, gc, globals, hash, hint, http_server, io, json, math, mem,
    net, num, os, parallel, path, process, promise, ptr, regex, sync, test,
    thread, time, tls, trace, ui,
};

pub mod runtime {
    pub use rts_runtime::namespaces::runtime::*;
    pub mod eval_jit;
}
