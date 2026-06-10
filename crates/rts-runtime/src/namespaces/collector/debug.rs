//! GC debug logging — instrumentacao centralizada e desativada por padrao.
//!
//! Toda saida passa por [`is_enabled`] (cacheada em `OnceLock`) e so' emite
//! quando `RTS_GC_DEBUG=1`. Em release builds com a flag off, o overhead e'
//! uma leitura atomica + branch-not-taken.
//!
//! Convencoes do output:
//! - `[gc] <evento> ...` — uma linha por evento
//! - eventos relevantes: `mark_stack_roots`, `MARK`, `SWEEP`, safepoints
//!   registrados em compile-time
//! - bom para diagnosticar leak/liveness em servers de longa duracao
//!   (ex: HTTP) e para acompanhar pacing do GC periodico
//!
//! Uso:
//! ```ignore
//! use crate::namespaces::gc::debug;
//! if debug::is_enabled() {
//!     eprintln!("[gc] event ...");
//! }
//! ```

use std::sync::OnceLock;

/// `true` quando `RTS_GC_DEBUG=1` foi setado no startup. Cacheado pra
/// evitar releitura do env em hot paths (mark/sweep rodam a cada N allocs).
pub fn is_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var("RTS_GC_DEBUG").is_ok())
}
