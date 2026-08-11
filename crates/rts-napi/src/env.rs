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
    /// The pointer the addon keeps per environment. See [`crate::instance`].
    pub instance: Option<crate::instance::Instance>,
    /// Hooks to run at teardown. See [`crate::cleanup`].
    pub cleanup: Vec<crate::cleanup::napi_async_cleanup_hook_handle>,
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
            instance: None,
            cleanup: Vec::new(),
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

    /// Puts a value in the scope BELOW the innermost, and answers its handle.
    ///
    /// What `napi_escape_handle` is: a value made inside a scope that is about
    /// to close has to live somewhere the close will not take it, and the only
    /// such place is the scope that will still be open afterwards.
    ///
    /// `None` when the innermost is the only one — there is nowhere below, and
    /// escaping from the scope the ABI gave the addon is meaningless rather
    /// than merely unsupported.
    pub fn handle_below(&mut self, value: u64) -> Option<crate::abi::napi_value> {
        let below = self.scopes.len().checked_sub(2)?;
        Some(self.scopes[below].handle(value))
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
    // The addon's own teardown runs FIRST, while everything it may reach is
    // still standing: a cleanup hook is entitled to call back in, and one that
    // ran after the scopes were dropped would be handed handles to nothing.
    // SAFETY: the caller's contract — a live environment.
    if let Some(held) = unsafe { crate::handles::env_of(env) } {
        let hooks = core::mem::take(&mut held.cleanup);
        // SAFETY: every handle in that list is one `crate::cleanup` leaked and
        // has not been removed, which is the invariant that module keeps.
        unsafe { crate::cleanup::run(hooks) };
        if let Some(instance) = held.instance.take()
            && let Some(finalize) = instance.finalize
        {
            // SAFETY: the addon's own function, with the two words it supplied.
            unsafe { finalize(env, instance.data, instance.hint) };
        }
    }
    crate::functions::forget(env);
    crate::finalizers::forget(env);
    crate::references::forget(env);
    crate::wrap::forget(env);
    crate::threadsafe::forget(env);
    // SAFETY: the caller's contract.
    drop(unsafe { Env::from_raw(env) });
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}
