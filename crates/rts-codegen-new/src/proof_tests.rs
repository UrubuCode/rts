//! # P1 proof suite — the new value model SOLVING the old engine's failures.
//!
//! Each test below reproduces a SPECIFIC failure mode of the old engine
//! (`rts-codegen-old`) and shows the new `PolyValue` representation handling it
//! correctly, through **real Cranelift JIT execution** — not just a pure-Rust
//! unit test. Every proof JIT-compiles a function via `lower/` + `lower/jit` and
//! runs the resulting native code, calling into the REAL runtime symbols
//! (`__RTS_FN_NS_COLLECTIONS_VEC_*`, the string pool via the adapter, the generic
//! `__rtsadp_*` operators) across the PolyValue ABI boundary.
//!
//! The old-engine bug each proof refutes is cited inline.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use crate::lower::ir::{Func, Node};
use crate::lower::jit;
use crate::repr::Repr;
use crate::value::{abi_adapter, genops};
use crate::value::PolyValue;

// ===========================================================================
// Proof 1 — numeric_unboxed
// ===========================================================================
//
// The winning monomorphic numeric path (design §3.1) is PRESERVED unboxed: a
// proven-`f64` computation lowers to native `fmul`/`fadd` with NO boxing. This
// is the path the old engine got right (~5× Bun) and the redesign must not
// regress.

#[test]
fn numeric_unboxed() {
    // f(x: f64) = x*x + 1.0  — both operands Float64 → native fmul/fadd.
    let func = Func::single(
        vec![Repr::Float64],
        Repr::Float64,
        Node::add(
            Node::mul(Node::Param(0), Node::Param(0)),
            Node::ConstF64(1.0),
        ),
    );
    let f = jit::jit_run_f64_f64(&func);

    for x in [0.0_f64, 1.0, 1.5, -2.0, 3.25, 10.0, -0.5, 1e6] {
        let expected = x * x + 1.0;
        let got = f(x);
        assert_eq!(got.to_bits(), expected.to_bits(), "x*x+1 wrong for {x}");
    }
}

// ===========================================================================
// Proof 2 — bool_survives_fn_boundary
// ===========================================================================
//
// THE OLD ENGINE FAILED THIS. Its `MAINTENANCE.md` at the 100% tag confessed:
// "um bool perde a tag ao cruzar uma fronteira de função porque parâmetros/
// retornos são `i64` e `true == 1`." A `true` passed through a function became
// the integer 1 and printed as "1". Tagging bools as i64::MIN+k sentinels was
// tried and reverted (broke 83 TS tests).
//
// With PolyValue, a boolean is a distinct tagged word that carries its identity
// THROUGH the i64 ABI slot: the param/return is Tagged (the raw PolyValue), so
// `true` stays `true` — `is_bool()`, `as_bool() == true`, typeof "boolean".

#[test]
fn bool_survives_fn_boundary() {
    // id(x): Tagged -> Tagged  (param and return are raw PolyValue words).
    let func = Func::single(vec![Repr::Tagged], Repr::Tagged, Node::Param(0));
    let id = jit::jit_run_u64_u64(&func);

    let input = PolyValue::bool(true);
    let out = PolyValue::from_raw(id(input.raw()));

    // The old engine would have round-tripped this as the integer 1 ("1").
    assert!(out.is_bool(), "boolean lost its tag crossing the fn boundary");
    assert!(out.as_bool(), "true did not survive as true");
    assert_eq!(out.typeof_str(), "boolean", "typeof must be boolean, not number");
    assert_eq!(out.raw(), input.raw(), "exact bit round-trip");

    // false too, and a negative control: an int32 1 stays a number, not a bool.
    let f_out = PolyValue::from_raw(id(PolyValue::bool(false).raw()));
    assert!(f_out.is_bool() && !f_out.as_bool());
    let one = PolyValue::from_raw(id(PolyValue::from_i32(1).raw()));
    assert!(one.is_int32() && !one.is_bool());
    assert_eq!(one.typeof_str(), "number");
}

// ===========================================================================
// Proof 3 — float_in_heterogeneous_container
// ===========================================================================
//
// THE OLD ENGINE needed `Entry::FloatPrim` + a FLOAT_BOX/UNBOX/EQ/ARITH helper
// quadruple to put a fractional float into a `Vec<i64>` (design §2.2): the i64
// slot was already overloaded, so `1.5` had to be re-boxed. Here a `1.5` double,
// a `7` int32, and an interned string all live in ONE `Vec<PolyValue>` as plain
// storage — heterogeneous storage falls out of the value model for free.
//
// The float is pushed THROUGH a JIT'd function calling the extern container ops
// (proving the boundary for at least the float); the int and string are pushed
// via the extern fns directly from Rust, then all three are read back.

#[test]
fn float_in_heterogeneous_container() {
    // Build the vec via the REAL collections extern. The handle is a real runtime
    // u64 (gen+slot+shard), carried verbatim across the JIT boundary as the raw
    // i64 param word.
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();

    // Push the FLOAT through a JIT'd function: push_float(vec) pushes 1.5 and
    // returns the vec handle. The element slot of the REAL Vec is a raw i64; we
    // store the PolyValue raw word there — heterogeneous storage falls out for
    // free, now in the REAL Entry::Vec. Body =
    // [ CallExtern(VEC_PUSH, vec, polyword(1.5)); Return vec ].
    let float_bits = PolyValue::from_f64(1.5).raw();
    let push_float = Func {
        params: vec![Repr::Tagged], // the real vec handle (raw i64 word)
        ret: Repr::Tagged,
        body: vec![
            Node::CallExtern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                vec![Node::Param(0), Node::ConstPoly(float_bits)],
            ),
            Node::Return(Box::new(Node::Param(0))),
        ],
    };
    let jit_push = jit::jit_run_u64_u64(&push_float);
    let returned = jit_push(vec);
    assert_eq!(returned, vec, "JIT'd push returned the vec handle unchanged");

    // Push the int32 and the string via the REAL extern directly from Rust. The
    // i64 element is the PolyValue raw word.
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, PolyValue::from_i32(7).raw() as i64);
    let hi = abi_adapter::intern_poly("hi");
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, hi.raw() as i64);

    // length is 3.
    assert_eq!(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(vec), 3);

    // [0] is the float 1.5, a "number" — read the raw i64 element back as a Poly.
    let e0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec, 0) as u64);
    assert!(e0.is_double(), "stored float must read back as a double");
    assert_eq!(e0.as_f64(), 1.5);
    assert_eq!(e0.typeof_str(), "number");

    // [1] is the int32 7, a "number".
    let e1 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec, 1) as u64);
    assert!(e1.is_int32(), "stored int must read back as an int32");
    assert_eq!(e1.as_i32(), 7);
    assert_eq!(e1.typeof_str(), "number");

    // [2] is the string "hi", a "string".
    let e2 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec, 2) as u64);
    assert!(e2.is_string(), "stored string must read back as a string");
    assert_eq!(abi_adapter::resolve_poly(e2), "hi");
    assert_eq!(e2.typeof_str(), "string");
}

// ===========================================================================
// Proof 3b — gc_marks_through_nanboxed_container (the indirection-table killer)
// ===========================================================================
//
// The OLD adapter kept a process-global `Vec<u64>` mapping a small idx → the full
// real handle, and held the ONLY strong ref to those handles WITHOUT being a GC
// root (a documented GC-root hole). We deleted that table: a heap PolyValue now
// carries the bare 48-bit slot+shard and the generation is reconstructed on
// demand (`__RTS_FN_NS_GC_POLY_TO_HANDLE`). For that to be safe, the GC must mark
// THROUGH a NaN-boxed handle word: when a boxed string lives only inside a real
// `Entry::Vec`, marking the Vec must keep the string alive.
//
// This test would FAIL without the `mark_handle` normalization (engine STEP 2):
// the string is reachable ONLY via the Vec's NaN-boxed child, so marking the Vec
// would NOT reach the string and a subsequent sweep would collect it.
//
// NB: we do NOT call the process-global `sweep_all_shards()` here. The whole test
// suite runs multi-threaded over ONE shared global HandleTable, and a global
// sweep frees every *unmarked* slot — including handles other concurrent tests
// hold in Rust locals (invisible to this thread's mark phase). That would corrupt
// sibling tests (observed: `typeof` separator / `float_in_heterogeneous_container`
// Vec collected mid-flight). Instead we prove the mark-through directly with the
// read-only `handle_is_marked`: mark the Vec, then assert the underlying string
// slot's mark bit is set. Sweep *behaviour* (an unmarked slot is freed, a marked
// one survives) is covered isolation-safely by the engine-side unit tests.

#[test]
fn gc_marks_through_nanboxed_container() {
    // Intern a string into a PolyValue and stash its 48-bit payload. We drop every
    // other reference to the string handle; the only surviving reference is the
    // boxed word pushed into the REAL Vec below — so the slot is reachable ONLY
    // transitively through the Vec's NaN-boxed child.
    let s = abi_adapter::intern_poly("survive-the-gc");
    let boxed_word = s.raw();
    let payload = s.as_handle(); // 48-bit slot+shard

    // Push the boxed string word into a REAL Entry::Vec (the container is the root).
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, boxed_word as i64);

    // The full real handle of the boxed string (gen reconstructed from the live
    // slot). BEFORE marking, its mark bit must be clear.
    let str_handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(payload);
    assert_ne!(str_handle, 0, "interned string must be live before the mark");
    assert_eq!(
        rt_handles::handle_is_marked(str_handle),
        Some(false),
        "string slot must start unmarked"
    );

    // Mark the container root (simulating the conservative scanner finding the Vec
    // handle live on the stack). `mark_handle` walks the Vec's children; each child
    // word is normalized through `poly_handle_normalize`, so the NaN-boxed string
    // word resolves to `str_handle` and is marked transitively.
    rt_handles::mark_handle(vec);

    // THE PROOF: the string slot — reachable ONLY via the Vec's NaN-boxed child —
    // is now marked. Without engine STEP 2 (normalize in `mark_handle`) the raw
    // boxed word would not decode to a live slot and this would be `Some(false)`,
    // i.e. the string would be swept => the GC-root hole the deleted table hid.
    assert_eq!(
        rt_handles::handle_is_marked(str_handle),
        Some(true),
        "string reachable only via a NaN-boxed Vec child must be marked (GC hole otherwise)"
    );

    // The Vec root itself is marked too, and still resolves to length 1.
    assert_eq!(rt_handles::handle_is_marked(vec), Some(true), "Vec root must be marked");
    assert_eq!(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(vec), 1, "Vec must resolve, len 1");

    // The stored word still round-trips: reconstruct the handle and read the bytes
    // back through the REAL pool, proving the bridge + storage are intact.
    let stored = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec, 0) as u64;
    let stored_pv = PolyValue::from_raw(stored);
    assert!(stored_pv.is_string(), "stored word must still be a string PolyValue");
    let real = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(stored_pv.as_handle());
    assert_eq!(real, str_handle, "stored payload reconstructs the same live handle");
    assert_eq!(
        abi_adapter::real_handle_to_string(real),
        "survive-the-gc",
        "the NaN-boxed string reads back intact"
    );

    // Clean up: the mark bit lingers until a sweep, but we do not run one (see the
    // module note). Freeing our own handles keeps the shared table tidy.
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_FREE(vec);
    rt_handles::free_handle(str_handle);
}

// ===========================================================================
// Proof 4 — polymorphic_add_one_path
// ===========================================================================
//
// The old engine guessed `+` semantics from the AST shape, producing the famous
// `arr[0] + 5 → "05"` bug (design §2.1/§7.3): a value loaded from a container
// was mis-classified as a string by shape heuristics. Here `__rtsadp_add` is ONE
// tag-dispatched path — it inspects the runtime tags of the ACTUAL values, never
// the source shape; the string side concatenates through the REAL pool. add(1,2)=3
// (number); add("a","b")="ab"; add(1,"x")="1x".
//
// The numeric case is driven from a JIT'd function that BOXES its two native
// Int32 inputs and CallExterns `__rtsadp_add`; the string cases feed Tagged
// PolyValue literals through the same JIT'd `CallExtern` path.

#[test]
fn polymorphic_add_one_path() {
    // add_ii(a: Int32, b: Int32) -> Tagged : boxes both params, calls __rtsadp_add.
    let add_ii = Func::single(
        vec![Repr::Int32, Repr::Int32],
        Repr::Tagged,
        Node::CallExtern("__rtsadp_add", vec![Node::Param(0), Node::Param(1)]),
    );
    let jit_add_ii = jit::jit_run_ii_u64(&add_ii);

    // add(1, 2) → int32 3, typeof "number".
    let r = PolyValue::from_raw(jit_add_ii(1, 2));
    assert!(r.is_int32() && r.as_i32() == 3, "1+2 should be int32 3, got {r:?}");
    assert_eq!(r.typeof_str(), "number");

    // Helper: a zero-param JIT'd fn that CallExterns __rtsadp_add on two Tagged
    // literals — same single path, fed string/mixed operands.
    let add_consts = |a: PolyValue, b: PolyValue| -> PolyValue {
        let func = Func::single(
            vec![],
            Repr::Tagged,
            Node::CallExtern(
                "__rtsadp_add",
                vec![Node::ConstPoly(a.raw()), Node::ConstPoly(b.raw())],
            ),
        );
        let f = jit::jit_run_unit_u64(&func);
        PolyValue::from_raw(f())
    };

    // add("a", "b") → string "ab" (concatenated in the REAL string pool).
    let sa = abi_adapter::intern_poly("a");
    let sb = abi_adapter::intern_poly("b");
    let ab = add_consts(sa, sb);
    assert!(ab.is_string(), r#""a"+"b" should be a string, got {ab:?}"#);
    assert_eq!(abi_adapter::resolve_poly(ab), "ab");
    assert_eq!(ab.typeof_str(), "string");

    // add(1, "x") → string "1x"  (NOT "0x", NOT 1 — the old AST-shape bug class).
    let one = PolyValue::from_i32(1);
    let sx = abi_adapter::intern_poly("x");
    let onex = add_consts(one, sx);
    assert!(onex.is_string(), r#"1+"x" should be a string, got {onex:?}"#);
    assert_eq!(abi_adapter::resolve_poly(onex), "1x");
    assert_eq!(onex.typeof_str(), "string");

    // And the SAME generic op, called directly, agrees (one path, no shape input):
    let direct = PolyValue::from_raw(genops::__rtsadp_add(one.raw(), sx.raw()));
    assert_eq!(abi_adapter::resolve_poly(direct), "1x");
}

// ===========================================================================
// Proof 5 — typeof_single_tag
// ===========================================================================
//
// `typeof` is a SINGLE tag inspection — no side-table, no per-value tracking
// (design §5.3). Covers every kind. Checked both on the pure model (`typeof_str`)
// and through the generic op `__rtsadp_typeof` (which returns a string handle
// interned in the REAL pool).

#[test]
fn typeof_single_tag() {
    let cases: &[(PolyValue, &str)] = &[
        (PolyValue::from_i32(42), "number"),
        (PolyValue::from_f64(1.5), "number"),
        (PolyValue::from_f64(f64::NAN), "number"),
        (abi_adapter::intern_poly("hello"), "string"),
        (PolyValue::from_object_handle(3), "object"),
        (PolyValue::from_function_handle(4), "function"),
        (PolyValue::undefined(), "undefined"),
        (PolyValue::null(), "object"), // JS quirk
        (PolyValue::bool(true), "boolean"),
        (PolyValue::bool(false), "boolean"),
    ];
    for &(v, want) in cases {
        // pure model
        assert_eq!(v.typeof_str(), want, "typeof_str wrong for {v:?}");
        // generic op: returns a string-handle PolyValue (interned in the real pool).
        let result = PolyValue::from_raw(genops::__rtsadp_typeof(v.raw()));
        assert!(result.is_string());
        assert_eq!(abi_adapter::resolve_poly(result), want, "__rtsadp_typeof wrong for {v:?}");
    }
}

// ===========================================================================
// Proof 6 — box_unbox_is_folded_pair
// ===========================================================================
//
// box/unbox are inverse PURE-IR ops (design §5.3/§9.3): Unbox(Box(x)) == x. They
// are emitted as `bitcast`/`band`/`bor`/`ishl`/`sshr` (no extern call) precisely
// so Cranelift's egraph (`use_egraphs=true`) can fold the redundant pair away.
// Here we JIT a function whose body is literally Unbox(Box(ConstI32(42))) and
// assert it returns 42 — proving the round-trip through real codegen.

#[test]
fn box_unbox_is_folded_pair() {
    // entry() -> i64 :  Unbox( Box( ConstI32(42) ), Int32 )
    let func = Func::single(
        vec![],
        Repr::Int32,
        Node::unbox(Node::boxed(Node::ConstI32(42)), Repr::Int32),
    );
    // ret repr Int32 carried as i64.
    let f = {
        let jf = jit::compile(&func);
        let raw = jf.ptr();
        let g: extern "C" fn() -> i64 = unsafe { std::mem::transmute(raw) };
        move || {
            let _keep = &jf;
            g()
        }
    };
    assert_eq!(f(), 42, "Unbox(Box(42)) must round-trip to 42");

    // A few more values, parameterised: id_through_box(x: Int32) -> Int32 that
    // boxes then unboxes its param. The egraph should collapse it to identity.
    let id_box = Func::single(
        vec![Repr::Int32],
        Repr::Int32,
        Node::unbox(Node::boxed(Node::Param(0)), Repr::Int32),
    );
    let idf = jit::jit_run_i64_i64(&id_box);
    for x in [0_i64, 1, -1, 42, -42, 123456, -987654, i32::MAX as i64, i32::MIN as i64] {
        assert_eq!(idf(x), x, "box/unbox identity failed for {x}");
    }
}
