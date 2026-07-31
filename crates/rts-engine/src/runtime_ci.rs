//! Runtime class-instance method table — `(class, method, arity) -> native fn`.
//!
//! The engine resolves an instance-method call `recv.m(a)` at COMPILE time when
//! it can prove `recv`'s Registry class (a `new C()`, a `let h = createHash()`
//! whose spec return type names the class, a chained call). When it CANNOT — the
//! receiver is a plain array element, a function parameter, or any value whose
//! class the front-end never tracked — the lowering falls to the generic dynamic
//! trampoline (`__rtsadp_dyn_method_call`), which reads the method off the
//! receiver's prototype chain. Object-backed Registry classes (an `Entry::Map`
//! tagged `__rts_class`, e.g. `Hash`/`Stats`/`StringDecoder`) publish NO methods
//! on a prototype — their methods are native `extern "C"` functions in the
//! Registry — so that path throws `TypeError: m is not a function`.
//!
//! This table closes that gap WITHOUT the engine naming any non-primordial class:
//! it is pure data harvested from the Registry (every `InstanceMethod` member of
//! every class → its `fn_ptr` + ABI signature), keyed by the class name that the
//! VALUE ITSELF carries in its `__rts_class` tag at runtime. The dynamic
//! trampoline, before throwing, reads that tag and looks the method up here, then
//! marshals the boxed args to the native ABI and calls the real fn — the same
//! `fn_ptr` the proven compile-time path would have called. Fully polymorphic
//! (the class comes from the value, never hardcoded), fully data-driven.

use crate::abi::AbiType;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// One resolved native instance method: its raw fn pointer plus the ABI
/// signature the marshaller needs to build the call.
#[derive(Clone)]
pub struct CiMethod {
    /// The native `extern "C"` function pointer (receiver-first args).
    pub fn_ptr: *const u8,
    /// Full argument ABI, INCLUDING the leading receiver `Handle`.
    pub args: Vec<AbiType>,
    /// Return ABI.
    pub ret: AbiType,
    /// A property GETTER (read yields the value) vs a callable METHOD (read yields
    /// a bound function value). Distinguishes `s["length"]` (getter → number) from
    /// `s["toUpperCase"]` (method → callable) on a primitive-string `obj_get`.
    pub is_getter: bool,
}

// SAFETY: `fn_ptr` is a `'static` code address (a Rust `extern "C"` fn), never
// dereferenced as data and valid for the whole process; sharing it across
// threads is sound.
unsafe impl Send for CiMethod {}
unsafe impl Sync for CiMethod {}

/// The arity overloads of ONE `(class, method)` pair. Tiny in practice (a method
/// declares one signature, at most a handful of arity variants), so the `_ge`
/// searches scan it directly instead of paying an index.
type Overloads = Vec<(usize, Arc<CiMethod>)>;

/// `class -> method -> [(arity, method)]`.
///
/// NESTED on purpose. The flat `HashMap<(String, String, usize), _>` this
/// replaces could not be probed without allocating: a `(String, String, usize)`
/// key does not `Borrow` from a `(&str, &str, usize)`, so every lookup built two
/// `String`s just to hash them — and every miss (the common case for `_ge`, whose
/// whole job is to NOT match the exact arity) fell back to scanning the entire
/// process-global table: every method of every registered class, on every dynamic
/// property read and method call. Nesting restores `Borrow<str>`, so a lookup is
/// two allocation-free hash probes plus a scan of that one method's overloads.
type Table = HashMap<String, HashMap<String, Overloads>>;

fn table() -> &'static RwLock<Table> {
    static TABLE: OnceLock<RwLock<Table>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register one class instance-method. `arity` is the FULL sig arity (receiver
/// included), matching `sig.args.len()`. Idempotent — re-registering the same
/// key (a rebuilt engine between runs) just overwrites with the same data.
#[allow(clippy::too_many_arguments)]
pub fn register_ci(
    class: &str,
    method: &str,
    arity: usize,
    fn_ptr: *const u8,
    args: Vec<AbiType>,
    ret: AbiType,
    is_getter: bool,
) {
    let ci = Arc::new(CiMethod { fn_ptr, args, ret, is_getter });
    let mut t = table().write().unwrap();
    let overloads = t
        .entry(class.to_string())
        .or_default()
        .entry(method.to_string())
        .or_default();
    match overloads.iter_mut().find(|(a, _)| *a == arity) {
        Some(slot) => slot.1 = ci,
        None => overloads.push((arity, ci)),
    }
}

/// Resolve `(class, method)` to its arity overloads, tolerant of OPTIONAL
/// trailing params: when no method is registered at exactly `min_arity`, return
/// the one with the SMALLEST arity `>= min_arity` (a value-class method with
/// defaulted trailing args — `indexOf(needle, position?)` called with one arg).
/// The caller pads the omitted arg slots with `undefined` (which the body reads
/// as its own default). `None` when no overload admits that arg count.
pub fn lookup_ci_ge(class: &str, method: &str, min_arity: usize) -> Option<Arc<CiMethod>> {
    let t = table().read().unwrap();
    let overloads = t.get(class)?.get(method)?;
    if let Some((_, ci)) = overloads.iter().find(|(a, _)| *a == min_arity) {
        return Some(ci.clone());
    }
    if let Some((_, ci)) = overloads
        .iter()
        .filter(|(a, ci)| *a >= min_arity && !ci.is_getter)
        .min_by_key(|(a, _)| *a)
    {
        return Some(ci.clone());
    }
    // EXTRA args are IGNORED in JS: a call with more arguments than the method
    // declares is not an error, the surplus is dropped. So when no overload
    // admits `min_arity`, take the LARGEST one below it (`date.toJSON(key)` —
    // the spec passes the property key to a 0-arg `Date.prototype.toJSON`; the
    // whole JSON `toJSON` hook threw `TypeError: toJSON is not a function`
    // because nothing accepted 2 slots). The caller's surplus arg words are
    // simply not marshalled.
    overloads
        .iter()
        .filter(|(a, ci)| *a < min_arity && !ci.is_getter)
        .max_by_key(|(a, _)| *a)
        .map(|(_, ci)| ci.clone())
}

/// Whether `class` has an instance METHOD (not a getter) named `method` at ANY
/// arity — the read-time check for `obj[name]` on a primitive: a hit means the
/// read yields a BOUND method value (`s["toUpperCase"]`), a miss falls through to
/// the normal property lookup. Getters are excluded (they read as a value).
pub fn has_instance_method(class: &str, method: &str) -> bool {
    let t = table().read().unwrap();
    t.get(class)
        .and_then(|methods| methods.get(method))
        .is_some_and(|overloads| overloads.iter().any(|(_, ci)| !ci.is_getter))
}
