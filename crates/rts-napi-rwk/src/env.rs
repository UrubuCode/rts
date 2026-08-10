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

/// How many argument slots a call carries.
///
/// The engine's convention. `napi_get_cb_info` fills an array of exactly this
/// many, so it is named once here rather than written as a `4` in two files.
pub const ARGUMENTS: usize = 4;

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
    /// Not the whole teardown — see [`destroy`], which is what a caller wants.
    /// This half exists on its own because the scopes must be dropped while the
    /// runtime is still installed (they release external roots), and the
    /// registry must be cleared while this pointer is still valid (it is the
    /// key). Two steps, one order, stated in one place.
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

/// Tears an environment down: its registered functions, then itself.
///
/// The registry is cleared FIRST and it has to be: a slot is keyed by this
/// pointer, and clearing after the box is freed would compare against a pointer
/// that may already have been handed to something else — which would free the
/// wrong addon's callbacks, silently, and only when two addons are loaded.
///
/// # Safety
///
/// `env` must be a pointer [`Env::into_raw`] produced and not yet destroyed.
pub unsafe fn destroy(env: napi_env) {
    crate::functions::forget(env);
    // SAFETY: the caller's contract.
    drop(unsafe { Env::from_raw(env) });
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}
