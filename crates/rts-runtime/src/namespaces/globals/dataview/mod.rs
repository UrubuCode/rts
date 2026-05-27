//! `ArrayBuffer` e `DataView` globais.
//!
//! Backing: `Entry::Buffer(Vec<u8>)` via o namespace `buffer`. `ArrayBuffer(n)`
//! aloca um buffer zerado; `DataView(buf)` eh uma view sobre esse handle
//! (byteOffset = 0). Os getters/setters de DataView seguem a spec JS
//! (big-endian por padrao). As impls extern "C" vivem em
//! `crate::namespaces::buffer::ops` (`__RTS_FN_GL_DATAVIEW_*`).

pub mod abi;
