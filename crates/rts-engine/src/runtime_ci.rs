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
use std::sync::{OnceLock, RwLock};

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

type Key = (String, String, usize);

fn table() -> &'static RwLock<HashMap<Key, CiMethod>> {
    static TABLE: OnceLock<RwLock<HashMap<Key, CiMethod>>> = OnceLock::new();
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
    table().write().unwrap().insert(
        (class.to_string(), method.to_string(), arity),
        CiMethod { fn_ptr, args, ret, is_getter },
    );
}

/// Like [`lookup_ci`] but tolerant of OPTIONAL trailing params: when no method is
/// registered at exactly `min_arity`, return the one with the SMALLEST arity `>=
/// min_arity` (a value-class method with defaulted trailing args — `indexOf(needle,
/// position?)` called with one arg). The caller pads the omitted arg slots with
/// `undefined` (which the body reads as its own default). `None` when no overload
/// admits that arg count.
pub fn lookup_ci_ge(class: &str, method: &str, min_arity: usize) -> Option<CiMethod> {
    let t = table().read().unwrap();
    if let Some(ci) = t.get(&(class.to_string(), method.to_string(), min_arity)) {
        return Some(ci.clone());
    }
    t.iter()
        .filter(|((c, m, a), ci)| c == class && m == method && *a >= min_arity && !ci.is_getter)
        .min_by_key(|((_, _, a), _)| *a)
        .map(|(_, ci)| ci.clone())
}

/// Whether `class` has an instance METHOD (not a getter) named `method` at ANY
/// arity — the read-time check for `obj[name]` on a primitive: a hit means the
/// read yields a BOUND method value (`s["toUpperCase"]`), a miss falls through to
/// the normal property lookup. Getters are excluded (they read as a value).
pub fn has_instance_method(class: &str, method: &str) -> bool {
    table()
        .read()
        .unwrap()
        .iter()
        .any(|((c, m, _), ci)| c == class && m == method && !ci.is_getter)
}

/// Look up a native instance method by the runtime class tag, method name, and
/// full arity (receiver + explicit args). `None` when the class does not carry
/// that method at that arity — the caller then falls back to its normal miss
/// behavior (a `TypeError`).
pub fn lookup_ci(class: &str, method: &str, arity: usize) -> Option<CiMethod> {
    table()
        .read()
        .unwrap()
        .get(&(class.to_string(), method.to_string(), arity))
        .cloned()
}
