//! The context every entry point is handed.
//!
//! # What it holds, and what it deliberately does not
//!
//! A stack of open handle scopes, and nothing else yet. It does NOT hold a
//! pointer to the runtime's `Context`: that is reached through the thread-local
//! every entry point in `rts-core` reaches through, for the reason stated in
//! `entry/mod.rs` — a context that could be passed is a context a caller can
//! pass the wrong one of.
//!
//! So an `Env` is a per-addon bookkeeping record, not a handle to the engine.
//! The engine is ambient; this is the part the ABI needs a pointer to.

use crate::abi::napi_env;
use crate::handles::Scope;

/// What a `napi_env` points at.
pub struct Env {
    /// Open handle scopes, innermost last.
    ///
    /// There is always at least one while a call from an addon is in progress —
    /// the ABI guarantees the addon a scope it did not open — so
    /// [`Self::current`] never has to answer "no scope" to an addon that
    /// followed the rules.
    scopes: Vec<Scope>,
}

impl Env {
    /// An environment with one open scope: the one the ABI promises an addon.
    pub fn new() -> Self {
        Env {
            scopes: vec![Scope::new()],
        }
    }

    /// The innermost open scope.
    pub fn current(&mut self) -> &mut Scope {
        self.scopes
            .last_mut()
            .expect("an environment always has the scope its constructor opened")
    }

    /// Opens a scope and answers how deep it now is.
    pub fn open(&mut self) -> usize {
        self.scopes.push(Scope::new());
        self.scopes.len()
    }

    /// Closes the innermost scope, releasing its handles.
    ///
    /// Refuses to close the outermost one: it is the scope the ABI gave the
    /// addon, and closing it would leave the next call with nowhere to put a
    /// handle. An addon that unbalances its scopes gets a status, not a crash.
    pub fn close(&mut self) -> bool {
        match self.scopes.len() > 1 {
            true => {
                self.scopes.pop();
                true
            }
            false => false,
        }
    }

    /// How many scopes are open.
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Hands ownership to the ABI as an opaque pointer.
    ///
    /// Leaked on purpose: the addon holds this for as long as it is loaded, and
    /// the only thing that may free it is [`Self::from_raw`].
    pub fn into_raw(self) -> napi_env {
        napi_env(Box::into_raw(Box::new(self)).cast())
    }

    /// Takes ownership back, dropping every open scope with it.
    ///
    /// # Safety
    ///
    /// `env` must be a pointer [`Self::into_raw`] produced, not yet passed
    /// here.
    pub unsafe fn from_raw(env: napi_env) -> Option<Box<Env>> {
        match env.0.is_null() {
            true => None,
            // SAFETY: the caller's contract, above.
            false => Some(unsafe { Box::from_raw(env.0.cast::<Env>()) }),
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}
