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
pub fn register_ci(
    class: &str,
    method: &str,
    arity: usize,
    fn_ptr: *const u8,
    args: Vec<AbiType>,
    ret: AbiType,
) {
    table().write().unwrap().insert(
        (class.to_string(), method.to_string(), arity),
        CiMethod { fn_ptr, args, ret },
    );
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
