//! `Object.*` statics reached as first-class FUNCTION VALUES.
//!
//! The CALLED form (`Object.keys(o)`) is lowered by the engine at the call site
//! (`rts-codegen-new/src/front/run/objstatic.rs`), which is why it has always
//! worked; this module covers the OTHER form — the static read as a value
//! (`const dp = Object.defineProperty`, `arr.map(Object.freeze)`) — by handing
//! back a real callable whose env slot names the static. Minified bundles hold
//! these in a variable constantly (`var d = Object.defineProperty`), and before
//! this the bare `Object` identifier reached nothing and threw
//! `ReferenceError: Object is not defined` at the READ.
//!
//! Both forms end in the SAME `__rtsadp_*` runtime authority, so the value form
//! cannot compute something different from the called form.
//!
//! `Object` is PRIMORDIAL, so naming its members here is allowed by the
//! doctrine. Modelled on [`super::mathfn`], including the reserved-0 trap it
//! already paid for (see [`OBJECT_FN_OPS`]).

use rts_runtime::namespaces::gc::handles::{Entry, FunctionData, alloc_entry};

use super::PolyValue;
use super::mathfn::uniform_args;

/// Every `Object` STATIC reachable as a first-class function VALUE, in a FIXED
/// order. This is the SINGLE source both sides read: the engine's member-read
/// path resolves a property name to its position here, and [`object_static_thunk`]
/// dispatches on the same entry — so a reordering cannot desynchronize them.
///
/// The op-code encoding lives in [`super::opcode`], including why `0` is
/// reserved and why the FIRST entry (`assign`) is the one that needs a test.
pub const OBJECT_FN_OPS: &[&str] = &[
    "assign",
    "create",
    "defineProperties",
    "defineProperty",
    "entries",
    "freeze",
    "fromEntries",
    "getOwnPropertyDescriptor",
    "getOwnPropertyDescriptors",
    "getOwnPropertyNames",
    "getOwnPropertySymbols",
    "getPrototypeOf",
    "groupBy",
    "hasOwn",
    "is",
    "isExtensible",
    "isFrozen",
    "isSealed",
    "keys",
    "preventExtensions",
    "seal",
    "setPrototypeOf",
    "values",
];

/// The env-slot op code for an `Object` static's name, or `None` when the name is
/// not one. The encoding (and the reserved `0`) belongs to [`super::opcode`] —
/// do not restate the off-by-one here.
pub fn object_fn_op_code(name: &str) -> Option<i64> {
    super::opcode::encode(OBJECT_FN_OPS, name)
}

/// The `Object` static an env-slot op code denotes, or `None` when the code is
/// out of range (the reserved `0` included).
fn object_fn_op_name(op: u64) -> Option<&'static str> {
    super::opcode::decode(OBJECT_FN_OPS, op)
}

/// The spec `length` of an `Object` static — what `Object.keys.length` must read.
fn object_fn_arity(name: &str) -> u8 {
    match name {
        "assign" | "create" | "defineProperties" | "getOwnPropertyDescriptor" | "groupBy"
        | "hasOwn" | "is" | "setPrototypeOf" => 2,
        "defineProperty" => 3,
        _ => 1,
    }
}

/// Apply the `Object` static named `name` (an entry of [`OBJECT_FN_OPS`]) to the
/// already-collected argument WORDS. Every arm calls the SAME `__rtsadp_*`
/// trampoline the engine emits for the called form — nothing is re-implemented
/// here. A missing argument reads as `undefined`, which is what the called form
/// with too few arguments passes too.
fn object_apply(name: &str, args: &[u64]) -> u64 {
    use super::genops::__rtsadp_same_value;
    use super::iterops::{__rtsadp_obj_keys, __rtsadp_obj_own_names, __rtsadp_obj_own_symbols};
    use super::objops::{
        __rtsadp_freeze, __rtsadp_has_own, __rtsadp_is_extensible, __rtsadp_is_frozen,
        __rtsadp_is_sealed, __rtsadp_obj_assign, __rtsadp_obj_define_properties,
        __rtsadp_obj_define_property, __rtsadp_obj_entries, __rtsadp_obj_from_entries,
        __rtsadp_obj_get_own_property_descriptor, __rtsadp_obj_get_own_property_descriptors,
        __rtsadp_obj_group_by, __rtsadp_obj_values, __rtsadp_prevent_ext, __rtsadp_seal,
    };
    use super::protos::{__rtsadp_obj_create, __rtsadp_obj_proto_of, __rtsadp_obj_set_proto};

    let undef = PolyValue::undefined().raw();
    let arg = |i: usize| args.get(i).copied().unwrap_or(undef);

    match name {
        // ---- variadic ----
        // `Object.assign(target, ...sources)`: fold the sources left-to-right
        // through the same per-source trampoline the call site chains.
        "assign" => {
            let mut target = arg(0);
            for s in args.iter().skip(1) {
                target = __rtsadp_obj_assign(target, *s);
            }
            target
        }
        // ---- object producers ----
        // `Object.create(proto[, descriptors])` — the descriptor bag goes through
        // the single defineProperty authority, exactly like the call site.
        "create" => {
            let o = __rtsadp_obj_create(arg(0));
            if args.len() > 1 {
                __rtsadp_obj_define_properties(o, arg(1))
            } else {
                o
            }
        }
        "defineProperties" => __rtsadp_obj_define_properties(arg(0), arg(1)),
        "defineProperty" => __rtsadp_obj_define_property(arg(0), arg(1), arg(2)),
        "fromEntries" => __rtsadp_obj_from_entries(arg(0)),
        "groupBy" => __rtsadp_obj_group_by(arg(0), arg(1)),
        // ---- enumeration ----
        "entries" => __rtsadp_obj_entries(arg(0)),
        "keys" => __rtsadp_obj_keys(arg(0)),
        "values" => __rtsadp_obj_values(arg(0)),
        "getOwnPropertyNames" => __rtsadp_obj_own_names(arg(0)),
        "getOwnPropertySymbols" => __rtsadp_obj_own_symbols(arg(0)),
        // ---- descriptors ----
        "getOwnPropertyDescriptor" => __rtsadp_obj_get_own_property_descriptor(arg(0), arg(1)),
        "getOwnPropertyDescriptors" => __rtsadp_obj_get_own_property_descriptors(arg(0)),
        // ---- prototypes ----
        "getPrototypeOf" => __rtsadp_obj_proto_of(arg(0)),
        "setPrototypeOf" => __rtsadp_obj_set_proto(arg(0), arg(1)),
        // ---- integrity: the trampolines return a FLAG, JS returns the object ----
        "freeze" => {
            __rtsadp_freeze(arg(0));
            arg(0)
        }
        "seal" => {
            __rtsadp_seal(arg(0));
            arg(0)
        }
        "preventExtensions" => {
            __rtsadp_prevent_ext(arg(0));
            arg(0)
        }
        // ---- predicates: an `i64` 0/1 flag becomes a real boolean word ----
        "isExtensible" => PolyValue::bool(__rtsadp_is_extensible(arg(0)) != 0).raw(),
        "isFrozen" => PolyValue::bool(__rtsadp_is_frozen(arg(0)) != 0).raw(),
        "isSealed" => PolyValue::bool(__rtsadp_is_sealed(arg(0)) != 0).raw(),
        // these two already hand back a boolean WORD.
        "hasOwn" => __rtsadp_has_own(arg(0), arg(1)),
        "is" => __rtsadp_same_value(arg(0), arg(1)),
        // `OBJECT_FN_OPS` is the only producer of `name`, so this is unreachable.
        // `undefined` is the honest answer for an unknown static, never a wrong
        // object.
        _ => undef,
    }
}

/// Uniform-ABI thunk for an `Object.<static>` FUNCTION VALUE. `env` carries the
/// index into [`OBJECT_FN_OPS`]; the args follow the ≤4 + overflow-array
/// convention.
extern "C" fn object_static_thunk(env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64 {
    let Some(name) = object_fn_op_name(env) else {
        return PolyValue::undefined().raw();
    };
    let args = uniform_args(a0, a1, a2, a3, rest);
    object_apply(name, &args)
}

/// An `Object` static read as a first-class FUNCTION VALUE (`const dp =
/// Object.defineProperty`, `arr.map(Object.freeze)` …): a real callable whose env
/// slot carries the [`OBJECT_FN_OPS`] index.
#[rtse::abi]
pub fn rtsadp_object_fn_value(op: u64) -> u64 {
    let name = object_fn_op_name(op).unwrap_or("");
    let data = FunctionData {
        fn_ptr: object_static_thunk as *const () as usize as u64,
        arity: object_fn_arity(name),
        name: Box::<str>::from(name),
        bound_this: op as i64,
        has_bound_this: true,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    };
    let h = alloc_entry(Entry::Function(Box::new(data)));
    PolyValue::from_function_handle(h & super::PAYLOAD_MASK).raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two ends of the op code must agree EXACTLY. A one-off makes every
    /// static silently become its neighbour — `Object.keys` running
    /// `Object.isSealed` and still returning a value, so nothing crashes.
    #[test]
    fn op_code_round_trips_for_every_member() {
        for &name in OBJECT_FN_OPS {
            let code = object_fn_op_code(name).expect("every table entry has a code");
            assert_eq!(
                object_fn_op_name(code as u64),
                Some(name),
                "op code {code} did not round-trip back to `{name}`"
            );
        }
    }

    /// 0 is RESERVED: `__rtsadp_fn_invoke` uses `env == 0` as the "captures
    /// nothing" sentinel and rewrites it before the thunk sees it, so codes must
    /// start at 1 or the FIRST entry breaks while `typeof` still reads
    /// `"function"`.
    #[test]
    fn zero_is_reserved_and_codes_start_at_one() {
        assert_eq!(object_fn_op_name(0), None, "0 must decode to nothing");
        assert_eq!(
            object_fn_op_code(OBJECT_FN_OPS[0]),
            Some(1),
            "first entry is 1"
        );
        assert_eq!(object_fn_op_name(1), Some(OBJECT_FN_OPS[0]));
    }

    /// A name that is not an `Object` static has no code — the engine's
    /// member-read path relies on `None` to fall through to its honest bail.
    #[test]
    fn unknown_name_has_no_code() {
        assert_eq!(object_fn_op_code("nopeNotAStatic"), None);
        assert_eq!(object_fn_op_name(OBJECT_FN_OPS.len() as u64 + 1), None);
    }

    /// A duplicate entry makes `position()` return the FIRST index, so the second
    /// copy is unreachable and its code decodes to the wrong static.
    #[test]
    fn table_has_no_duplicates() {
        let mut seen: Vec<&str> = OBJECT_FN_OPS.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "OBJECT_FN_OPS contains a duplicate");
    }

    /// Every entry must have a dispatch arm. `object_apply` falls through to
    /// `undefined` for an unknown name, which is exactly the silent-wrong-value
    /// shape a missing arm would take, so the table and the match are checked
    /// against each other here rather than by review.
    #[test]
    fn every_entry_has_an_arity() {
        for &name in OBJECT_FN_OPS {
            assert!(
                (1..=3).contains(&object_fn_arity(name)),
                "`{name}` has an out-of-range declared length"
            );
        }
    }
}
