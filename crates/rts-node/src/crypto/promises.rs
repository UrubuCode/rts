//! node:crypto has no promise sub-API implemented in this slice yet (Node's
//! real `crypto.webcrypto`/`crypto/promises` surface needs the same
//! `Buffer`/stateful-object machinery deferred in `symbols.rs`). This file
//! exists only for the uniform module layout (symbols.rs / promises.rs /
//! mod.rs); it declares no symbols.
