//! How an addon says what it is.
//!
//! A `.node` file exports one function — `napi_register_module_v1(env,
//! exports)` — and answers the object the `require` of it produces. Older
//! addons instead call `napi_module_register` from a static constructor, which
//! runs before anything asks. Both are here; both end in the same place.
//!
//! # What this half does and what it does not
//!
//! It runs a registrar and produces its exports. It does NOT load a `.node`
//! from disk, and the reason is worth stating precisely rather than as a
//! to-do: an addon resolves `napi_create_double` and friends **out of the host
//! process**, by name, at load time. That works when the process EXPORTS those
//! symbols — `-rdynamic` on ELF, an export table entry on COFF — and this
//! binary exports none of them today. Adding `dlopen` before that would produce
//! a loader that opens a file and fails on the first undefined symbol, which
//! looks like a bug in the addon.
//!
//! So the split is: registration is here and tested; the export table is a
//! change to the build, and `PLAN.md` says what it involves.
//!
//! # Why `exports` is an argument and not a return
//!
//! Node hands the registrar an object and takes back whatever it answers,
//! which is usually that same object with properties on it. An addon that
//! answers something else — a function, a class — replaces the exports
//! entirely, and both shapes are common. So this passes one in and uses what
//! comes back.

use core::ffi::c_void;

use crate::abi::{napi_env, napi_value};
use crate::handles::value_of;

/// What a registrar looks like.
pub type Register =
    Option<unsafe extern "C" fn(env: napi_env, exports: napi_value) -> napi_value>;

/// The record an older addon hands to [`napi_module_register`].
///
/// **The layout is the ABI's**, down to the two reserved words: an addon builds
/// this as a C aggregate in its own translation unit.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct napi_module {
    /// The N-API version the addon was built against.
    pub nm_version: i32,
    /// Flags. Unused by the ABI today.
    pub nm_flags: u32,
    /// The file the addon was compiled from, for diagnostics.
    pub nm_filename: *const core::ffi::c_char,
    /// The registrar.
    pub nm_register_func: Register,
    /// The name the module registers under.
    pub nm_modname: *const core::ffi::c_char,
    /// The addon's own pointer, handed back nowhere by this ABI.
    pub nm_priv: *mut c_void,
    /// Reserved. Four words, and they are part of the layout.
    pub reserved: [*mut c_void; 4],
}

/// A module an addon registered before anything asked for it.
struct Registered {
    name: String,
    register: Register,
}

thread_local! {
    /// Modules registered by a static constructor, waiting to be asked for.
    static PENDING: core::cell::RefCell<Vec<Registered>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// `napi_module_register` — the older path, called from a static constructor.
///
/// It runs before a `Context` exists, which is why nothing is evaluated here:
/// the registrar is recorded and [`exports_of`] runs it when the module is
/// actually asked for. Calling it early would reach a thread-local runtime that
/// the host has not installed yet, and that is an abort rather than an error.
///
/// # Safety
///
/// `module` must point at a live `napi_module` whose strings are
/// NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_module_register(module: *mut napi_module) {
    if module.is_null() {
        return;
    }
    // SAFETY: the caller's contract.
    let module = unsafe { &*module };
    let name = match module.nm_modname.is_null() {
        true => String::new(),
        // SAFETY: the caller's contract.
        false => unsafe { core::ffi::CStr::from_ptr(module.nm_modname) }
            .to_string_lossy()
            .into_owned(),
    };
    PENDING.with_borrow_mut(|pending| {
        pending.push(Registered {
            name,
            register: module.nm_register_func,
        })
    });
}

/// Runs a registrar and answers what the module exports.
///
/// The environment is this crate's, made here and handed to the addon for the
/// whole of its life — an addon caches the `napi_env` it was given and uses it
/// on every later call, so it must outlive the registration.
///
/// # Safety
///
/// `register` must be the addon's own entry point.
pub unsafe fn run(env: napi_env, register: Register) -> Option<u64> {
    let register = register?;
    let exports = rts_core::entry::with_runtime(rts_core::entry::make_object);
    // SAFETY: the caller's contract.
    let handle = unsafe { crate::handles::env_of(env) }?
        .current()
        .handle(exports);
    // SAFETY: the addon's own function, with the environment it will keep.
    let answered = unsafe { register(env, handle) };
    // SAFETY: a handle this crate just made, or one the addon answered from
    // the same scope.
    match unsafe { value_of(answered) } {
        // An addon that answers nothing keeps the object it was given, which is
        // the common shape: it hung properties on it and returned it.
        None => Some(exports),
        Some(word) => Some(word),
    }
}

/// Runs a module registered earlier by name, and answers its exports.
///
/// `None` when nothing registered under that name — which is what a `require`
/// of a `.node` that never called `napi_module_register` looks like.
///
/// # Safety
///
/// `env` must be live for as long as the addon is.
pub unsafe fn exports_of(env: napi_env, name: &str) -> Option<u64> {
    let register = PENDING.with_borrow(|pending| {
        pending
            .iter()
            .find(|module| module.name == name)
            .map(|module| module.register)
    })?;
    // SAFETY: the caller's contract.
    unsafe { run(env, register) }
}

/// The name of the module registered most recently.
///
/// What [`crate::loader`] reads after mapping a library: an older addon
/// registers from a static constructor while it is being MAPPED, so the loader
/// learns its name by watching this list grow rather than by asking the library
/// anything — there is nothing to ask, which is why that path exists at all.
pub fn last_registered() -> Option<String> {
    PENDING.with_borrow(|pending| pending.last().map(|module| module.name.clone()))
}

/// How many modules have registered and not yet been asked for.
///
/// For a host deciding whether a library it loaded registered anything at all —
/// an addon that neither exports `napi_register_module_v1` nor calls
/// `napi_module_register` has told nobody it exists, and saying so beats
/// answering an empty object.
pub fn registered() -> usize {
    PENDING.with_borrow(Vec::len)
}
