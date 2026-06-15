//! Unit tests for [`PolyValue`](super::PolyValue): the pure bit-model plus two
//! Cranelift JIT roundtrips proving the emitted IR matches the model.

use super::*;

// -- helpers --

/// Doubles representative of every interesting class — none of which may be
/// classified as boxed (NaN is handled separately because of canonicalization).
fn representative_doubles() -> Vec<f64> {
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        1.5,
        -2.25,
        3.141592653589793,
        f64::MIN,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE / 2.0, // subnormal
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e308,
        -1e308,
        123456789.0,
    ]
}

// ----------------------------------------------------------------
// 1. Pure-model unit tests
// ----------------------------------------------------------------

#[test]
fn doubles_roundtrip_and_classify() {
    for &d in &representative_doubles() {
        let v = PolyValue::from_f64(d);
        assert!(v.is_double(), "{d} should classify as a double");
        assert!(!v.is_boxed(), "{d} must not be in boxed space");
        assert!(!v.is_int32());
        assert!(!v.is_string());
        assert!(!v.is_object());
        assert!(!v.is_function());
        // exact round-trip
        let back = v.as_f64();
        assert_eq!(
            back.to_bits(),
            d.to_bits(),
            "{d} did not round-trip exactly"
        );
        assert_eq!(v.typeof_str(), "number");
    }
}

#[test]
fn signed_zero_is_preserved() {
    let pos = PolyValue::from_f64(0.0);
    let neg = PolyValue::from_f64(-0.0);
    assert!(pos.is_double() && neg.is_double());
    // +0.0 and -0.0 are bit-distinct and the sign must survive.
    assert_eq!(pos.as_f64().to_bits(), 0u64);
    assert_eq!(neg.as_f64().to_bits(), 0x8000_0000_0000_0000u64);
    assert!(pos.as_f64() == 0.0 && neg.as_f64() == 0.0);
    assert!(pos.as_f64().is_sign_positive());
    assert!(neg.as_f64().is_sign_negative());
    // is_truthy: both zeroes are falsy.
    assert!(!pos.is_truthy());
    assert!(!neg.is_truthy());
}

#[test]
fn neg_infinity_is_a_double_not_boxed() {
    // -Infinity = 0xFFF0_0000_0000_0000. Bit 51 is 0, so it is NOT boxed.
    let v = PolyValue::from_f64(f64::NEG_INFINITY);
    assert_eq!(v.raw(), 0xFFF0_0000_0000_0000);
    assert!(v.is_double(), "-Infinity must classify as a double");
    assert!(!v.is_boxed(), "-Infinity must not be in boxed space");
    assert_eq!(v.as_f64(), f64::NEG_INFINITY);
    assert!(v.is_truthy()); // -Infinity is truthy in JS
    assert_eq!(v.typeof_str(), "number");

    // +Infinity for symmetry.
    let p = PolyValue::from_f64(f64::INFINITY);
    assert!(p.is_double() && !p.is_boxed());
    assert_eq!(p.as_f64(), f64::INFINITY);
}

#[test]
fn nan_is_canonicalized_and_classifies_as_double() {
    let v = PolyValue::from_f64(f64::NAN);
    // Stored as the positive canonical qNaN, NOT in boxed space.
    assert_eq!(v.raw(), CANONICAL_NAN);
    assert!(v.is_double(), "canonical NaN must classify as a double");
    assert!(!v.is_boxed(), "canonical NaN must NOT be in boxed space");
    assert!(v.as_f64().is_nan(), "must round-trip to a NaN");
    assert!(!v.is_truthy(), "NaN is falsy");
    assert_eq!(v.typeof_str(), "number");

    // A *negative* qNaN input is also canonicalized to the positive one, so
    // it never lands in the boxed space (this is the soundness guarantee).
    let neg_qnan = f64::from_bits(0xFFF8_0000_0000_0001);
    assert!(neg_qnan.is_nan());
    let v2 = PolyValue::from_f64(neg_qnan);
    assert_eq!(v2.raw(), CANONICAL_NAN);
    assert!(v2.is_double() && !v2.is_boxed());
}

#[test]
fn int32_roundtrip() {
    for &i in &[
        0i32,
        1,
        -1,
        42,
        -42,
        i32::MIN,
        i32::MAX,
        0x7FFF_FFFF,
        -0x8000_0000,
    ] {
        let v = PolyValue::from_i32(i);
        assert!(v.is_int32(), "{i} should be int32");
        assert!(v.is_boxed());
        assert!(!v.is_double(), "{i} must not classify as a double");
        assert_eq!(v.as_i32(), i, "{i} did not round-trip");
        assert_eq!(v.tag(), TAG_INT32);
        assert_eq!(v.typeof_str(), "number");
    }
    // truthiness: 0 falsy, everything else truthy.
    assert!(!PolyValue::from_i32(0).is_truthy());
    assert!(PolyValue::from_i32(1).is_truthy());
    assert!(PolyValue::from_i32(-1).is_truthy());
}

#[test]
fn handles_roundtrip() {
    let max48: u64 = PAYLOAD_MASK; // 0xFFFF_FFFF_FFFF
    for &slot in &[0u64, 1, 0xDEAD_BEEF, 0xFFFF_FFFF_FFFF, max48] {
        // string
        let s = PolyValue::from_str_handle(slot);
        assert!(s.is_string());
        assert!(!s.is_object() && !s.is_function() && !s.is_double());
        assert_eq!(s.as_handle(), slot);
        assert_eq!(s.typeof_str(), "string");
        assert_eq!(s.tag(), TAG_STR);

        // object
        let o = PolyValue::from_object_handle(slot);
        assert!(o.is_object());
        assert!(!o.is_string() && !o.is_function() && !o.is_double());
        assert_eq!(o.as_handle(), slot);
        assert_eq!(o.typeof_str(), "object");
        assert_eq!(o.tag(), TAG_OBJECT);

        // function
        let fnh = PolyValue::from_function_handle(slot);
        assert!(fnh.is_function());
        assert!(!fnh.is_string() && !fnh.is_object() && !fnh.is_double());
        assert_eq!(fnh.as_handle(), slot);
        assert_eq!(fnh.typeof_str(), "function");
        assert_eq!(fnh.tag(), TAG_FUNCTION);

        // all three with the same slot must be DISTINCT words (tag differs).
        assert_ne!(s.raw(), o.raw());
        assert_ne!(o.raw(), fnh.raw());
        assert_ne!(s.raw(), fnh.raw());

        // objects/functions/strings are truthy (string-emptiness aside).
        assert!(o.is_truthy());
        assert!(fnh.is_truthy());
        assert!(s.is_truthy());
    }
}

#[test]
fn singletons_distinct_and_correct() {
    let undef = PolyValue::undefined();
    let null = PolyValue::null();
    let t = PolyValue::bool(true);
    let f = PolyValue::bool(false);
    let hole = PolyValue::hole();
    let empty = PolyValue::empty();

    // predicates
    assert!(undef.is_undefined());
    assert!(null.is_null());
    assert!(t.is_bool() && f.is_bool());
    assert!(hole.is_hole());
    assert!(empty.is_empty());

    // none of these is a double / number / heap kind
    for s in [undef, null, t, f, hole, empty] {
        assert!(!s.is_double(), "{s:?} must not be a double");
        assert!(s.is_boxed());
        assert!(!s.is_int32() && !s.is_string() && !s.is_object() && !s.is_function());
        assert_eq!(s.tag(), TAG_SINGLETON);
    }

    // typeof
    assert_eq!(undef.typeof_str(), "undefined");
    assert_eq!(null.typeof_str(), "object"); // JS quirk
    assert_eq!(t.typeof_str(), "boolean");
    assert_eq!(f.typeof_str(), "boolean");

    // truthiness
    assert!(!undef.is_truthy());
    assert!(!null.is_truthy());
    assert!(t.is_truthy());
    assert!(!f.is_truthy());
    assert!(!hole.is_truthy());
    assert!(!empty.is_truthy());

    // all six singletons are pairwise bit-distinct
    let all = [undef, null, t, f, hole, empty];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(
                all[i].raw(),
                all[j].raw(),
                "singletons {i} and {j} share raw bits"
            );
        }
    }
}

#[test]
fn disjointness_no_double_is_boxed() {
    // Every non-double constructor output must NOT be a double.
    let mut non_doubles: Vec<PolyValue> = vec![
        PolyValue::undefined(),
        PolyValue::null(),
        PolyValue::bool(true),
        PolyValue::bool(false),
        PolyValue::hole(),
        PolyValue::empty(),
    ];
    for &i in &[0i32, -1, i32::MIN, i32::MAX, 12345] {
        non_doubles.push(PolyValue::from_i32(i));
    }
    for &slot in &[0u64, 1, PAYLOAD_MASK] {
        non_doubles.push(PolyValue::from_str_handle(slot));
        non_doubles.push(PolyValue::from_object_handle(slot));
        non_doubles.push(PolyValue::from_function_handle(slot));
    }
    for v in &non_doubles {
        assert!(!v.is_double(), "{v:?} must not be a double");
        assert!(v.is_boxed(), "{v:?} must be boxed");
    }

    // And a sweep of doubles must all be is_double and NOT mis-tagged.
    let mut doubles = representative_doubles();
    doubles.push(f64::NAN);
    for &d in &doubles {
        let v = PolyValue::from_f64(d);
        assert!(v.is_double(), "{d} must be a double");
        assert!(!v.is_int32());
        assert!(!v.is_string());
        assert!(!v.is_object());
        assert!(!v.is_function());
        assert!(!v.is_undefined() && !v.is_null() && !v.is_bool());
    }
}

#[test]
fn raw_roundtrips_through_from_raw() {
    let samples = [
        PolyValue::from_f64(1.5),
        PolyValue::from_i32(-7),
        PolyValue::from_str_handle(99),
        PolyValue::from_object_handle(0xABCD),
        PolyValue::from_function_handle(0xFFFF_FFFF_FFFF),
        PolyValue::undefined(),
        PolyValue::null(),
        PolyValue::bool(true),
    ];
    for v in samples {
        assert_eq!(PolyValue::from_raw(v.raw()), v);
    }
}

#[test]
fn encode_header_constants_are_what_the_docs_claim() {
    // BOX_BASE has the top 13 bits set and nothing else.
    assert_eq!(BOX_BASE, 0xFFF8_0000_0000_0000);
    assert_eq!(BOX_BASE >> 51, 0x1FFF); // 13 ones
    assert_eq!(BOX_BASE & !(0x1FFFu64 << 51), 0);
    // int32 header = BOX_BASE | (1<<48)
    assert_eq!(encode(TAG_INT32, 0), 0xFFF9_0000_0000_0000);
    // singleton header = BOX_BASE | (2<<48)
    assert_eq!(encode(TAG_SINGLETON, 0), 0xFFFA_0000_0000_0000);
    // CANONICAL_NAN is a double (positive qNaN), not boxed.
    assert_eq!(CANONICAL_NAN, 0x7FF8_0000_0000_0000);
    assert!(PolyValue::from_raw(CANONICAL_NAN).is_double());
}

// ----------------------------------------------------------------
// 2. Cranelift JIT roundtrip — proves the emitted IR matches the model.
// ----------------------------------------------------------------

#[test]
fn jit_unbox_int32_matches_model() {
    use cranelift_codegen::ir::types;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
    use cranelift_codegen::settings::{self, Configurable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, Module};

    // Build an ISA for the host.
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish isa");

    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    let mut module = JITModule::new(builder);

    // Signature: extern "C" fn(i64) -> i64. Takes a raw PolyValue word
    // (assumed tagged int32), returns the sign-extended i32 (as i64).
    let mut sig = Signature::new(module.isa().default_call_conv());
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    let func_id = module
        .declare_function("test_unbox_int32", Linkage::Local, &sig)
        .expect("declare");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);

        let arg = fb.block_params(entry)[0];
        let result = emit_unbox_int32(&mut fb, arg);
        fb.ins().return_(&[result]);
        fb.finalize();
    }

    module.define_function(func_id, &mut ctx).expect("define");
    module.clear_context(&mut ctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(func_id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(code) };

    for &i in &[0i32, 1, -1, 42, -42, i32::MIN, i32::MAX, 123456, -987654] {
        let boxed = PolyValue::from_i32(i);
        let model = PolyValue::from_raw(boxed.raw()).as_i32() as i64;
        let jitted = f(boxed.raw() as i64);
        assert_eq!(jitted, model, "JIT unbox_int32 mismatch for {i}");
        assert_eq!(jitted, i as i64, "JIT unbox_int32 wrong value for {i}");
    }
}

#[test]
fn jit_is_boxed_matches_model() {
    use cranelift_codegen::ir::types;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
    use cranelift_codegen::settings::{self, Configurable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, Module};

    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish isa");

    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    let mut module = JITModule::new(builder);

    // fn(i64) -> i64 returning 1 if boxed else 0.
    let mut sig = Signature::new(module.isa().default_call_conv());
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    let func_id = module
        .declare_function("test_is_boxed", Linkage::Local, &sig)
        .expect("declare");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);

        let arg = fb.block_params(entry)[0];
        let is_boxed_i8 = emit_is_boxed(&mut fb, arg);
        // widen the i8 bool to i64 for the return.
        let widened = fb.ins().uextend(types::I64, is_boxed_i8);
        fb.ins().return_(&[widened]);
        fb.finalize();
    }

    module.define_function(func_id, &mut ctx).expect("define");
    module.clear_context(&mut ctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(func_id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(code) };

    let cases: Vec<PolyValue> = vec![
        PolyValue::from_f64(0.0),
        PolyValue::from_f64(-0.0),
        PolyValue::from_f64(1.5),
        PolyValue::from_f64(f64::INFINITY),
        PolyValue::from_f64(f64::NEG_INFINITY),
        PolyValue::from_f64(f64::NAN),
        PolyValue::from_i32(7),
        PolyValue::from_i32(-7),
        PolyValue::from_str_handle(42),
        PolyValue::from_object_handle(0xABCD),
        PolyValue::from_function_handle(1),
        PolyValue::undefined(),
        PolyValue::null(),
        PolyValue::bool(true),
        PolyValue::bool(false),
        PolyValue::hole(),
        PolyValue::empty(),
    ];
    for v in cases {
        let model = if v.is_boxed() { 1i64 } else { 0 };
        let jitted = f(v.raw() as i64);
        assert_eq!(jitted, model, "JIT is_boxed mismatch for {v:?}");
    }
}
