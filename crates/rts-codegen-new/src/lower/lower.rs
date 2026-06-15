//! Lower a [`Func`] to a Cranelift `Function` — the single HIR→Cranelift path,
//! scaled to the P1 proofs.
//!
//! Representation rules (design pilar 2 + 5):
//! - `Float64` lives in an `f64` register, `Int32` in an `i64` register holding
//!   the sign-extended int, `Tagged` in an `i64` register holding the raw
//!   `PolyValue` word.
//! - `Add`/`Mul` are repr-aware: matching native reprs use `fadd`/`iadd` (the
//!   winning numeric fast path); anything else BOXes both operands and calls the
//!   generic `__rtsadp_add` — ONE path, never AST-shape guessing.
//! - `Box`/`Unbox` are PURE Cranelift IR (via [`crate::value`] emit helpers), so
//!   Cranelift's egraph can fold a redundant `unbox(box(x))` to nothing.

use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

use crate::repr::Repr;
use crate::value;
use crate::value::abi_sig::sig_of;
use crate::value::emit_marshal;

use super::ir::{Func, Node};

/// The Cranelift type a value of `repr` is carried in.
pub fn cl_type(repr: Repr) -> types::Type {
    match repr {
        Repr::Float64 => types::F64,
        // Int32 and Bool live sign/zero-extended in an i64 register; Ref/Tagged
        // are raw i64 PolyValue words.
        _ => types::I64,
    }
}

/// An SSA value paired with the representation it carries.
#[derive(Clone, Copy)]
struct Val {
    v: Value,
    repr: Repr,
}

/// Build the Cranelift `Signature` for a `Func` under the host call convention.
pub fn signature_for(func: &Func, module: &dyn Module) -> Signature {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &p in &func.params {
        sig.params.push(AbiParam::new(cl_type(p)));
    }
    sig.returns.push(AbiParam::new(cl_type(func.ret)));
    sig
}

/// Lower `func` into `builder` (whose `Function` already has the right
/// signature). `module` is used to declare/import the runtime externs that
/// `CallExtern` nodes reference. Returns nothing — on completion the builder
/// holds a complete, finalizable function.
pub fn lower_func(module: &mut dyn Module, builder: &mut FunctionBuilder, func: &Func) {
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    // Snapshot the block params (the function parameters) with their reprs.
    let params: Vec<Val> = func
        .params
        .iter()
        .enumerate()
        .map(|(i, &repr)| Val {
            v: builder.block_params(entry)[i],
            repr,
        })
        .collect();

    for node in &func.body {
        if let Node::Return(inner) = node {
            let r = lower_node(module, builder, &params, inner);
            // Coerce the returned value to the function's declared return repr.
            let coerced = coerce(builder, r, func.ret);
            builder.ins().return_(&[coerced]);
        } else {
            // P1 bodies are a single Return; evaluate any leading node for its
            // side effects (none today) but keep the path total.
            let _ = lower_node(module, builder, &params, node);
        }
    }
    // NOTE: `builder.finalize()` is intentionally NOT called here — it takes the
    // builder by value, and the harness owns it (it finalizes after this returns).
}

/// Lower one node, returning its SSA value + repr.
fn lower_node(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    params: &[Val],
    node: &Node,
) -> Val {
    match node {
        Node::ConstF64(f) => {
            let v = builder.ins().f64const(*f);
            Val {
                v,
                repr: Repr::Float64,
            }
        }
        Node::ConstI32(i) => {
            // carry the int sign-extended in an i64 register.
            let v = builder.ins().iconst(types::I64, *i as i64);
            Val {
                v,
                repr: Repr::Int32,
            }
        }
        Node::ConstPoly(bits) => {
            let v = builder.ins().iconst(types::I64, *bits as i64);
            Val {
                v,
                repr: Repr::Tagged,
            }
        }
        Node::Param(n) => params[*n],

        Node::Add(a, b) => lower_arith(module, builder, params, a, b, ArithOp::Add),
        Node::Mul(a, b) => lower_arith(module, builder, params, a, b, ArithOp::Mul),

        Node::Box(inner) => {
            let iv = lower_node(module, builder, params, inner);
            let boxed = box_value(builder, iv);
            Val {
                v: boxed,
                repr: Repr::Tagged,
            }
        }
        Node::Unbox(inner, to) => {
            let iv = lower_node(module, builder, params, inner);
            debug_assert_eq!(iv.repr, Repr::Tagged, "Unbox input must be Tagged");
            let v = unbox_value(builder, iv.v, *to);
            Val { v, repr: *to }
        }

        Node::CallExtern(name, args) => {
            let lowered_args: Vec<Value> = args
                .iter()
                .map(|a| {
                    let av = lower_node(module, builder, params, a);
                    // All extern args are Tagged i64 PolyValue words.
                    coerce(builder, av, Repr::Tagged)
                })
                .collect();
            let ret = call_extern(module, builder, name, &lowered_args);
            Val {
                v: ret,
                repr: Repr::Tagged,
            }
        }

        Node::Return(inner) => {
            // Returning is normally handled in lower_func; if a Return appears
            // nested, lower its inner value (shouldn't happen in P1 IR).
            lower_node(module, builder, params, inner)
        }
    }
}

#[derive(Clone, Copy)]
enum ArithOp {
    Add,
    Mul,
}

/// Lower `a <op> b`. Native fast path when both operands share `Float64` or
/// `Int32`; otherwise (Add only) BOX both and call the generic `__rtsadp_add`.
fn lower_arith(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    params: &[Val],
    a: &Node,
    b: &Node,
    op: ArithOp,
) -> Val {
    let av = lower_node(module, builder, params, a);
    let bv = lower_node(module, builder, params, b);

    match (av.repr, bv.repr) {
        (Repr::Float64, Repr::Float64) => {
            let v = match op {
                ArithOp::Add => builder.ins().fadd(av.v, bv.v),
                ArithOp::Mul => builder.ins().fmul(av.v, bv.v),
            };
            Val {
                v,
                repr: Repr::Float64,
            }
        }
        (Repr::Int32, Repr::Int32) => {
            let v = match op {
                ArithOp::Add => builder.ins().iadd(av.v, bv.v),
                ArithOp::Mul => builder.ins().imul(av.v, bv.v),
            };
            Val {
                v,
                repr: Repr::Int32,
            }
        }
        _ => {
            // Generic path: BOX both, CallExtern __rtsadp_add. (Mul has no generic
            // P1 path; it is only used on proven-numeric operands.)
            debug_assert!(
                matches!(op, ArithOp::Add),
                "generic path only for Add in P1"
            );
            let boxed_a = box_value(builder, av);
            let boxed_b = box_value(builder, bv);
            let ret = call_extern(module, builder, "__rtsadp_add", &[boxed_a, boxed_b]);
            Val {
                v: ret,
                repr: Repr::Tagged,
            }
        }
    }
}

/// Coerce `val` to `target` repr, inserting box/unbox as needed. The only
/// coercions P1 needs: identity, native→Tagged (box), Tagged→native (unbox).
fn coerce(builder: &mut FunctionBuilder, val: Val, target: Repr) -> Value {
    if val.repr == target {
        return val.v;
    }
    match (val.repr, target) {
        (Repr::Int32, Repr::Tagged) | (Repr::Float64, Repr::Tagged) => box_value(builder, val),
        (Repr::Tagged, Repr::Int32) | (Repr::Tagged, Repr::Float64) => {
            unbox_value(builder, val.v, target)
        }
        // Ref/Bool coercions are not exercised by P1.
        (from, to) => panic!("unsupported coercion {from:?} -> {to:?} in P1 lowering"),
    }
}

/// BOX an unboxed value to a Tagged PolyValue word (pure IR).
fn box_value(builder: &mut FunctionBuilder, val: Val) -> Value {
    match val.repr {
        Repr::Int32 => {
            // The int lives sign-extended in i64; narrow to i32 for the box helper.
            let i32v = builder.ins().ireduce(types::I32, val.v);
            value::emit_box_int32(builder, i32v)
        }
        Repr::Float64 => value::emit_box_double(builder, val.v),
        Repr::Tagged => val.v, // already boxed
        other => panic!("cannot box repr {other:?} in P1"),
    }
}

/// UNBOX a Tagged PolyValue word to the requested native repr (pure IR).
fn unbox_value(builder: &mut FunctionBuilder, tagged: Value, to: Repr) -> Value {
    match to {
        // emit_unbox_int32 already returns the sign-extended value as i64.
        Repr::Int32 => value::emit_unbox_int32(builder, tagged),
        Repr::Float64 => value::emit_unbox_double(builder, tagged),
        other => panic!("cannot unbox to repr {other:?} in P1"),
    }
}

/// Declare-import a REAL runtime symbol (or a `__rtsadp_*` adapter trampoline)
/// into the current function and emit the call, with the Cranelift signature
/// derived EXACTLY from the real-symbol descriptor ([`crate::value::abi_sig`]).
/// For void symbols returns a placeholder `undefined` PolyValue so the caller
/// always has a value (it is never used for void calls).
fn call_extern(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    name: &'static str,
    args: &[Value],
) -> Value {
    let sig = sig_of(name).unwrap_or_else(|| panic!("unknown runtime symbol {name}"));
    debug_assert_eq!(
        sig.param_slot_count(),
        args.len(),
        "symbol {name} arity mismatch"
    );
    match emit_marshal::emit_call(module, builder, name, args) {
        Some(v) => v,
        None => {
            // void: synthesize an undefined PolyValue word as the (unused) result.
            builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64)
        }
    }
}
