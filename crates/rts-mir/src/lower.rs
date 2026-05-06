//! HIR → MIR lowering.
//!
//! Converts a `HirFunc` into a `MirFunc` with SSA-like basic blocks. This
//! is the initial implementation covering the linear path: arithmetic,
//! comparisons, if/else, while, let bindings, return. Constructs that
//! aren't yet supported lower to `Terminator::Trap { code: User(_) }` so
//! `rts-codegen` consuming the MIR can decide to fall back to the AST path.
//!
//! Not yet covered (will be follow-up sub-stages):
//! - for/forof/forin loops
//! - try/catch/finally + throw
//! - switch
//! - method calls + member access
//! - array/object literals
//! - extern calls (rts namespace dispatch) — needs ABI integration
//! - destructuring, spread, ternary nesting beyond simple cases

use std::collections::HashMap;

use rts_hir::ir::{
    HirBinOp, HirExpr, HirExprKind, HirFunc, HirLit, HirStmt, HirType, HirUnOp,
};
use rts_hir::scope::Scope;

use crate::ir::*;

/// Unsupported-construct trap code. Picked to stay distinct so debugging the
/// generated MIR can pinpoint which HIR feature triggered the fallback.
const TRAP_UNSUPPORTED_STMT: u16 = 0x10;

/// Resolves a `(namespace, member)` pair (e.g. `("math", "sqrt")`) to a
/// concrete extern call signature. Returns `(symbol, param_tys, ret_ty)`
/// when the namespace member exists, or `None` to leave the MethodCall
/// unresolved. Provided by the caller to keep `rts-mir` free of the runtime
/// SPECS table.
pub type ExternResolver<'r> = &'r dyn Fn(&str, &str) -> Option<(String, Vec<HirType>, HirType)>;

/// Public entry: lower a fully-typed `HirFunc` into a `MirFunc`.
///
/// Without a `Scope`, calls to other user fns lower to a placeholder zero
/// (the MIR loses callee info). Use `lower_func_with_scope` when you have
/// a populated scope and want `Inst::CallUser` emitted.
pub fn lower_func(func: &HirFunc) -> MirFunc {
    let scope = Scope::new();
    lower_func_with_scope(func, &scope)
}

/// Lower with access to a populated scope so identifier callees can be
/// resolved to typed `Inst::CallUser` instructions.
pub fn lower_func_with_scope(func: &HirFunc, scope: &Scope) -> MirFunc {
    lower_func_full(func, scope, None)
}

/// Most general lowering entry: takes both a scope (for user-fn calls) and
/// an optional extern resolver (for namespace method calls like
/// `math.sqrt(x)`).
pub fn lower_func_full(
    func: &HirFunc,
    scope: &Scope,
    extern_resolver: Option<ExternResolver<'_>>,
) -> MirFunc {
    let mut mir = MirFunc::new(
        &func.name,
        // Tail conv matches `rts-codegen`'s default for user fns
        CallConvHint::Tail,
        func.ret.clone(),
    );

    // Allocate one ValueId per parameter and record it as a function param.
    let mut env: HashMap<String, ValueId> = HashMap::new();
    for p in &func.params {
        let v = mir.new_value(p.ty.clone());
        mir.params.push((v, p.ty.clone()));
        env.insert(p.name.clone(), v);
    }

    // Entry block carries the function params as block params (Cranelift idiom).
    let entry = mir.new_block();
    for (vid, ty) in mir.params.clone() {
        mir.blocks[entry as usize].params.push((vid, ty));
    }

    // Snapshot the scope's function signatures so we don't have to keep a
    // live reference to it through the lowering walk.
    let mut fn_sigs: HashMap<String, (Vec<HirType>, HirType)> = HashMap::new();
    // Self — the function being lowered is callable from within (recursion).
    fn_sigs.insert(
        func.name.clone(),
        (
            func.params.iter().map(|p| p.ty.clone()).collect(),
            func.ret.clone(),
        ),
    );
    // Pull every (name, params, ret) the scope already knows about.
    for (name, params, ret) in scope.iter_fn_sigs() {
        fn_sigs.insert(name.to_string(), (params.to_vec(), ret));
    }
    let mut ctx = LowerCtx {
        cursor: entry,
        env,
        terminated: false,
        loop_stack: Vec::new(),
        fn_sigs,
        extern_resolver,
    };

    lower_stmts(&func.body, &mut mir, &mut ctx);

    // If control falls off the end without an explicit return, append one.
    if !ctx.terminated {
        let trap = if matches!(func.ret, HirType::Void) {
            Terminator::Return(vec![])
        } else {
            // Returning the zero value of the declared type keeps the IR
            // well-formed even when the source forgot to return.
            let zero = emit_zero_const(&func.ret, &mut mir, &mut ctx);
            Terminator::Return(vec![zero])
        };
        set_term(&mut mir, ctx.cursor, trap);
    }

    mir
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx<'r> {
    cursor: BlockId,
    env: HashMap<String, ValueId>,
    /// True once the current block has emitted a terminator. Statements past
    /// that point are dead and lowered into a fresh unreachable block.
    terminated: bool,
    /// Loop stack: (continue_target, break_target). `break` jumps to the top
    /// of the stack's break target; `continue` to its continue target.
    loop_stack: Vec<(BlockId, BlockId)>,
    /// User fn signatures (name → (param_tys, ret_ty)) snapshotted from the
    /// HIR scope so `lower_expr` can emit `Inst::CallUser` with full type info.
    fn_sigs: HashMap<String, (Vec<HirType>, HirType)>,
    /// Optional resolver for `(namespace, method)` pairs to extern symbols.
    extern_resolver: Option<ExternResolver<'r>>,
}

impl LowerCtx<'_> {
    fn push_inst(&self, mir: &mut MirFunc, inst: Inst) {
        mir.blocks[self.cursor as usize].insts.push(inst);
    }
}

fn set_term(mir: &mut MirFunc, b: BlockId, term: Terminator) {
    mir.blocks[b as usize].term = term;
}

// ---------------------------------------------------------------------------
// Statement lowering
// ---------------------------------------------------------------------------

fn lower_stmts(stmts: &[HirStmt], mir: &mut MirFunc, ctx: &mut LowerCtx) {
    for stmt in stmts {
        if ctx.terminated {
            // Drop dead statements after a terminator — cleaner IR.
            break;
        }
        lower_stmt(stmt, mir, ctx);
    }
}

fn lower_stmt(stmt: &HirStmt, mir: &mut MirFunc, ctx: &mut LowerCtx) {
    match stmt {
        HirStmt::Expr(e) => {
            // Side-effecting expression statement: lower & discard.
            let _ = lower_expr(e, mir, ctx);
        }

        HirStmt::Return(opt) => {
            let term = match opt {
                Some(e) => {
                    let v = lower_expr(e, mir, ctx);
                    Terminator::Return(vec![v])
                }
                None => Terminator::Return(vec![]),
            };
            set_term(mir, ctx.cursor, term);
            ctx.terminated = true;
        }

        HirStmt::Let { name, ty, init } => {
            let v = match init {
                Some(e) => lower_expr(e, mir, ctx),
                None => emit_zero_const(ty, mir, ctx),
            };
            ctx.env.insert(name.clone(), v);
        }

        HirStmt::Const { name, init, .. } => {
            let v = lower_expr(init, mir, ctx);
            ctx.env.insert(name.clone(), v);
        }

        HirStmt::If { cond, then, else_ } => lower_if(cond, then, else_.as_deref(), mir, ctx),

        HirStmt::While { cond, body } => lower_while(cond, body, mir, ctx),

        HirStmt::DoWhile { body, cond } => lower_do_while(body, cond, mir, ctx),

        HirStmt::For { init, cond, update, body } => {
            lower_for(init.as_deref(), cond.as_ref(), update.as_ref(), body, mir, ctx)
        }

        HirStmt::Break(_label) => {
            // Labeled break/continue not yet supported — uses innermost loop.
            if let Some(&(_, brk)) = ctx.loop_stack.last() {
                set_term(mir, ctx.cursor, Terminator::Jump { target: brk, args: vec![] });
                ctx.terminated = true;
            } else {
                // break outside loop — fall through (codegen ast handles)
                set_term(mir, ctx.cursor, Terminator::Trap {
                    code: TrapHint::User(TRAP_UNSUPPORTED_STMT),
                });
                ctx.terminated = true;
            }
        }

        HirStmt::Continue(_label) => {
            if let Some(&(cont, _)) = ctx.loop_stack.last() {
                set_term(mir, ctx.cursor, Terminator::Jump { target: cont, args: vec![] });
                ctx.terminated = true;
            } else {
                set_term(mir, ctx.cursor, Terminator::Trap {
                    code: TrapHint::User(TRAP_UNSUPPORTED_STMT),
                });
                ctx.terminated = true;
            }
        }

        HirStmt::Block(body) => lower_stmts(body, mir, ctx),

        HirStmt::Switch { discriminant, cases } => {
            lower_switch(discriminant, cases, mir, ctx);
        }

        HirStmt::Throw(_e) => {
            // Throw maps to a trap in MIR for now — the AST codegen path
            // handles real exception unwind via thread-local error slot.
            // The MIR layer simply marks the block unreachable.
            set_term(
                mir,
                ctx.cursor,
                Terminator::Trap {
                    code: TrapHint::User(TRAP_UNSUPPORTED_STMT),
                },
            );
            ctx.terminated = true;
        }

        HirStmt::Try { body, catch: _, finally } => {
            // Phase 1: ignore catch (no unwind in MIR yet) — lower body.
            // After body, lower the `finally` block if present (runs on
            // normal completion). Real exception semantics need a landing
            // pad model, postponed to Phase 2.
            lower_stmts(body, mir, ctx);
            if let Some(fin) = finally {
                if !ctx.terminated {
                    lower_stmts(fin, mir, ctx);
                }
            }
        }

        // Constructs not yet supported — emit trap, mark terminated so caller
        // doesn't keep emitting into a dead block.
        _ => {
            set_term(
                mir,
                ctx.cursor,
                Terminator::Trap {
                    code: TrapHint::User(TRAP_UNSUPPORTED_STMT),
                },
            );
            ctx.terminated = true;
        }
    }
}

fn lower_if(
    cond: &HirExpr,
    then: &[HirStmt],
    else_: Option<&[HirStmt]>,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) {
    let cond_v = lower_expr(cond, mir, ctx);

    let then_b = mir.new_block();
    let else_b = mir.new_block();
    let join_b = mir.new_block();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: then_b,
            then_args: vec![],
            else_block: else_b,
            else_args: vec![],
        },
    );

    // then branch
    ctx.cursor = then_b;
    ctx.terminated = false;
    lower_stmts(then, mir, ctx);
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: join_b,
                args: vec![],
            },
        );
    }

    // else branch
    ctx.cursor = else_b;
    ctx.terminated = false;
    if let Some(stmts) = else_ {
        lower_stmts(stmts, mir, ctx);
    }
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: join_b,
                args: vec![],
            },
        );
    }

    // continue at join
    ctx.cursor = join_b;
    ctx.terminated = false;
}

fn lower_while(cond: &HirExpr, body: &[HirStmt], mir: &mut MirFunc, ctx: &mut LowerCtx) {
    let header_b = mir.new_block();
    let body_b = mir.new_block();
    let exit_b = mir.new_block();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Jump {
            target: header_b,
            args: vec![],
        },
    );

    // header: test condition
    ctx.cursor = header_b;
    ctx.terminated = false;
    let cond_v = lower_expr(cond, mir, ctx);
    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: vec![],
        },
    );

    // body: jump back to header at end. Push loop frame so break/continue work.
    ctx.cursor = body_b;
    ctx.terminated = false;
    ctx.loop_stack.push((header_b, exit_b));
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: header_b,
                args: vec![],
            },
        );
    }

    ctx.cursor = exit_b;
    ctx.terminated = false;
}

fn lower_do_while(body: &[HirStmt], cond: &HirExpr, mir: &mut MirFunc, ctx: &mut LowerCtx) {
    let body_b = mir.new_block();
    let header_b = mir.new_block();
    let exit_b = mir.new_block();

    set_term(mir, ctx.cursor, Terminator::Jump { target: body_b, args: vec![] });

    // body: runs at least once. continue jumps to header (cond test).
    ctx.cursor = body_b;
    ctx.terminated = false;
    ctx.loop_stack.push((header_b, exit_b));
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        set_term(mir, ctx.cursor, Terminator::Jump { target: header_b, args: vec![] });
    }

    // header: test cond, brif → body / exit
    ctx.cursor = header_b;
    ctx.terminated = false;
    let cond_v = lower_expr(cond, mir, ctx);
    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: vec![],
        },
    );

    ctx.cursor = exit_b;
    ctx.terminated = false;
}

fn lower_switch(
    discriminant: &HirExpr,
    cases: &[rts_hir::ir::HirSwitchCase],
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) {
    let disc = lower_expr(discriminant, mir, ctx);

    // Allocate one entry block per case + a join/exit block.
    let case_blocks: Vec<BlockId> = cases.iter().map(|_| mir.new_block()).collect();
    let exit_b = mir.new_block();

    // Build the (key, BlockId) pairs for non-default cases. Default block
    // points to either the explicit `default` case (test=None) or to exit
    // when no default is present.
    let mut switch_cases: Vec<(u64, BlockId)> = Vec::new();
    let mut default_block: Option<BlockId> = None;

    for (i, case) in cases.iter().enumerate() {
        match &case.test {
            None => {
                default_block = Some(case_blocks[i]);
            }
            Some(test_expr) => {
                if let Some(key) = extract_int_lit(test_expr) {
                    switch_cases.push((key as u64, case_blocks[i]));
                }
                // Non-literal case keys: codegen AST handles them; the MIR
                // path just won't match — falls through to default. This
                // matches JS semantics for keys that aren't compile-time
                // constants: we treat them as `if/else` chained outside
                // the Switch terminator (future enhancement).
            }
        }
    }

    let default = default_block.unwrap_or(exit_b);

    set_term(
        mir,
        ctx.cursor,
        Terminator::Switch {
            index: disc,
            default,
            cases: switch_cases,
        },
    );

    // Lower each case body. JS semantics: fall through to next case on
    // implicit completion; explicit `break` jumps to exit_b. We push a
    // loop frame with continue=exit_b/break=exit_b so naked `break` works
    // (continue inside switch is forwarded to enclosing loop, but that's
    // not modeled here).
    ctx.loop_stack.push((exit_b, exit_b));
    for (i, case) in cases.iter().enumerate() {
        ctx.cursor = case_blocks[i];
        ctx.terminated = false;
        lower_stmts(&case.body, mir, ctx);
        if !ctx.terminated {
            // Fall through to next case (or exit if last).
            let target = if i + 1 < case_blocks.len() {
                case_blocks[i + 1]
            } else {
                exit_b
            };
            set_term(
                mir,
                ctx.cursor,
                Terminator::Jump {
                    target,
                    args: vec![],
                },
            );
        }
    }
    ctx.loop_stack.pop();

    ctx.cursor = exit_b;
    ctx.terminated = false;
}

/// If `expr` is an integer literal (or one wrapped in a numeric literal that
/// fits as int), return the value. Used by `lower_switch` to pull case keys.
fn extract_int_lit(expr: &HirExpr) -> Option<i64> {
    match &expr.kind {
        HirExprKind::Lit(HirLit::Int(n)) => Some(*n),
        HirExprKind::Lit(HirLit::Number(n)) if n.fract() == 0.0 && n.is_finite() => {
            Some(*n as i64)
        }
        HirExprKind::Lit(HirLit::Float(n)) if n.fract() == 0.0 && n.is_finite() => {
            Some(*n as i64)
        }
        _ => None,
    }
}

fn lower_for(
    init: Option<&HirStmt>,
    cond: Option<&HirExpr>,
    update: Option<&HirExpr>,
    body: &[HirStmt],
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) {
    // 1. init runs in current block
    if let Some(s) = init {
        lower_stmt(s, mir, ctx);
        if ctx.terminated {
            return;
        }
    }

    let header_b = mir.new_block();
    let body_b = mir.new_block();
    let update_b = mir.new_block();
    let exit_b = mir.new_block();

    set_term(mir, ctx.cursor, Terminator::Jump { target: header_b, args: vec![] });

    // header: test cond (if missing, treat as `true`)
    ctx.cursor = header_b;
    ctx.terminated = false;
    let cond_v = match cond {
        Some(e) => lower_expr(e, mir, ctx),
        None => {
            let v = mir.new_value(HirType::Bool);
            ctx.push_inst(mir, Inst::IConst { dst: v, ty: HirType::Bool, val: 1 });
            v
        }
    };
    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: vec![],
        },
    );

    // body: continue → update_b, break → exit_b
    ctx.cursor = body_b;
    ctx.terminated = false;
    ctx.loop_stack.push((update_b, exit_b));
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        set_term(mir, ctx.cursor, Terminator::Jump { target: update_b, args: vec![] });
    }

    // update: run update expr (if any), then back to header
    ctx.cursor = update_b;
    ctx.terminated = false;
    if let Some(e) = update {
        let _ = lower_expr(e, mir, ctx);
    }
    set_term(mir, ctx.cursor, Terminator::Jump { target: header_b, args: vec![] });

    ctx.cursor = exit_b;
    ctx.terminated = false;
}

// ---------------------------------------------------------------------------
// Expression lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &HirExpr, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    match &expr.kind {
        HirExprKind::Lit(lit) => lower_lit(lit, &expr.ty, mir, ctx),

        HirExprKind::Ident(name) => match ctx.env.get(name) {
            Some(&v) => v,
            None => {
                // Unknown ident — emit a placeholder zero of the expected type.
                emit_zero_const(&expr.ty, mir, ctx)
            }
        },

        HirExprKind::Bin { op, lhs, rhs } => {
            let lv = lower_expr(lhs, mir, ctx);
            let rv = lower_expr(rhs, mir, ctx);
            lower_bin(*op, lv, rv, &expr.ty, &lhs.ty, mir, ctx)
        }

        HirExprKind::Unary { op, operand } => {
            let v = lower_expr(operand, mir, ctx);
            lower_unary(*op, v, &expr.ty, mir, ctx)
        }

        HirExprKind::Cast { expr: inner, target } => {
            let v = lower_expr(inner, mir, ctx);
            lower_cast(v, &inner.ty, target, mir, ctx)
        }

        HirExprKind::MethodCall { object, method, args } => {
            // Try to resolve `<ns>.<method>` via the extern resolver when
            // `object` is a bare identifier matching a known namespace.
            if let HirExprKind::Ident(ns_name) = &object.kind {
                if let Some(resolver) = ctx.extern_resolver {
                    if let Some((sym, param_tys, ret_ty)) = resolver(ns_name, method) {
                        // Build extern args. Each `HirType::Str` parameter
                        // expands to two i64 slots (ptr, len) — string
                        // literals materialize via Inst::StrLit; non-literal
                        // string args fall back to zero/zero (codegen AST
                        // handles GC-backed string handles).
                        let mut arg_vals: Vec<ValueId> = Vec::with_capacity(args.len() * 2);
                        let mut effective_param_tys: Vec<HirType> =
                            Vec::with_capacity(param_tys.len() * 2);

                        for (i, arg_expr) in args.iter().enumerate() {
                            let expected = param_tys.get(i);
                            if matches!(expected, Some(HirType::Str)) {
                                if let HirExprKind::Lit(HirLit::Str(s)) = &arg_expr.kind {
                                    let dst_ptr = mir.new_value(HirType::I64);
                                    let dst_len = mir.new_value(HirType::I64);
                                    ctx.push_inst(
                                        mir,
                                        Inst::StrLit {
                                            dst_ptr,
                                            dst_len,
                                            value: s.clone(),
                                        },
                                    );
                                    arg_vals.push(dst_ptr);
                                    arg_vals.push(dst_len);
                                    effective_param_tys.push(HirType::I64);
                                    effective_param_tys.push(HirType::I64);
                                    continue;
                                }
                                // Non-literal string arg: emit a (0, 0) pair
                                // so the call shape is correct; the runtime
                                // sees a null/empty string. Codegen AST
                                // path handles dynamic strings via handles.
                                let zero_p = emit_zero_const(&HirType::I64, mir, ctx);
                                let zero_l = emit_zero_const(&HirType::I64, mir, ctx);
                                arg_vals.push(zero_p);
                                arg_vals.push(zero_l);
                                effective_param_tys.push(HirType::I64);
                                effective_param_tys.push(HirType::I64);
                                continue;
                            }
                            // Non-string parameter: lower normally.
                            let v = lower_expr(arg_expr, mir, ctx);
                            arg_vals.push(v);
                            effective_param_tys
                                .push(expected.cloned().unwrap_or(HirType::I64));
                        }

                        let dst = if matches!(ret_ty, HirType::Void) {
                            None
                        } else {
                            Some(mir.new_value(ret_ty.clone()))
                        };
                        ctx.push_inst(
                            mir,
                            Inst::CallExtern {
                                dst,
                                sym,
                                args: arg_vals,
                                ret_ty: ret_ty.clone(),
                                param_tys: effective_param_tys,
                            },
                        );
                        return dst.unwrap_or_else(|| emit_zero_const(&expr.ty, mir, ctx));
                    }
                }
            }
            // Unresolved method call → placeholder zero (codegen AST handles
            // member access for class methods, etc.).
            emit_zero_const(&expr.ty, mir, ctx)
        }

        HirExprKind::Call { callee, args } => {
            // Only Ident callees that we have a signature for produce CallUser.
            if let HirExprKind::Ident(name) = &callee.kind {
                if let Some((param_tys, ret_ty)) = ctx.fn_sigs.get(name).cloned() {
                    let arg_vals: Vec<ValueId> =
                        args.iter().map(|a| lower_expr(a, mir, ctx)).collect();
                    let dst = if matches!(ret_ty, HirType::Void) {
                        None
                    } else {
                        Some(mir.new_value(ret_ty.clone()))
                    };
                    ctx.push_inst(
                        mir,
                        Inst::CallUser {
                            dst,
                            name: name.clone(),
                            args: arg_vals,
                            ret_ty: ret_ty.clone(),
                            param_tys,
                        },
                    );
                    return dst.unwrap_or_else(|| emit_zero_const(&expr.ty, mir, ctx));
                }
            }
            // Fallback: unknown callee or non-ident; placeholder zero.
            emit_zero_const(&expr.ty, mir, ctx)
        }

        HirExprKind::Ternary { cond, then, else_ } => {
            // Branchless: `cond ? then : else` → Inst::Select.
            // Both branches evaluated unconditionally (matches semantics
            // when `then` and `else_` are pure; side effects in branches
            // would need real branching — caller should split into stmts).
            let c = lower_expr(cond, mir, ctx);
            let t = lower_expr(then, mir, ctx);
            let e = lower_expr(else_, mir, ctx);
            let dst = mir.new_value(expr.ty.clone());
            ctx.push_inst(mir, Inst::Select {
                dst,
                cond: c,
                on_true: t,
                on_false: e,
            });
            dst
        }

        // Unsupported: emit a placeholder of the expected type and trap when
        // we leave the block (handled by caller observing `ctx.terminated`
        // after reaching a stmt that uses it). For now, emit an iconst 0.
        _ => emit_zero_const(&expr.ty, mir, ctx),
    }
}

fn lower_lit(lit: &HirLit, ty: &HirType, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    match lit {
        HirLit::Int(n) => {
            let dst_ty = if matches!(ty, HirType::Unknown | HirType::Any) {
                HirType::I64
            } else {
                ty.clone()
            };
            let dst = mir.new_value(dst_ty.clone());
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: dst_ty,
                    val: *n,
                },
            );
            dst
        }
        HirLit::Float(f) => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::F64Const { dst, val: *f });
            dst
        }
        HirLit::Number(n) => {
            let dst = mir.new_value(HirType::Number);
            ctx.push_inst(mir, Inst::F64Const { dst, val: *n });
            dst
        }
        HirLit::Bool(b) => {
            let dst = mir.new_value(HirType::Bool);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::Bool,
                    val: if *b { 1 } else { 0 },
                },
            );
            dst
        }
        HirLit::Null | HirLit::Undefined => {
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::I64,
                    val: 0,
                },
            );
            dst
        }
        HirLit::Str(_) => {
            // String literals are codegen-time and need GC; emit a placeholder
            // i64 zero — codegen will fall back to AST path for string ops
            // until the extern-call lowering lands.
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: HirType::I64,
                    val: 0,
                },
            );
            dst
        }
    }
}

fn lower_bin(
    op: HirBinOp,
    lhs: ValueId,
    rhs: ValueId,
    res_ty: &HirType,
    operand_ty: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let is_float = operand_ty.is_float() || res_ty.is_float();
    let dst = mir.new_value(res_ty.clone());

    let inst = match (op, is_float) {
        // Arithmetic
        (HirBinOp::Add, false) => Inst::IAdd { dst, lhs, rhs },
        (HirBinOp::Add, true) => Inst::FAdd { dst, lhs, rhs },
        (HirBinOp::Sub, false) => Inst::ISub { dst, lhs, rhs },
        (HirBinOp::Sub, true) => Inst::FSub { dst, lhs, rhs },
        (HirBinOp::Mul, false) => Inst::IMul { dst, lhs, rhs },
        (HirBinOp::Mul, true) => Inst::FMul { dst, lhs, rhs },
        (HirBinOp::Div, false) => Inst::SDiv { dst, lhs, rhs },
        (HirBinOp::Div, true) => Inst::FDiv { dst, lhs, rhs },
        (HirBinOp::Rem, false) => Inst::SRem { dst, lhs, rhs },

        // Bitwise
        (HirBinOp::BitAnd, false) => Inst::BAnd { dst, lhs, rhs },
        (HirBinOp::BitOr, false) => Inst::BOr { dst, lhs, rhs },
        (HirBinOp::BitXor, false) => Inst::BXor { dst, lhs, rhs },
        (HirBinOp::Shl, false) => Inst::IShl { dst, lhs, rhs },
        (HirBinOp::Shr, false) => Inst::SShr { dst, lhs, rhs },
        (HirBinOp::UShr, false) => Inst::UShr { dst, lhs, rhs },

        // Comparisons
        (HirBinOp::Eq, false) => Inst::ICmp { dst, cond: IntCond::Eq, lhs, rhs },
        (HirBinOp::Eq, true) => Inst::FCmp { dst, cond: FloatCond::Eq, lhs, rhs },
        (HirBinOp::Ne, false) => Inst::ICmp { dst, cond: IntCond::Ne, lhs, rhs },
        (HirBinOp::Ne, true) => Inst::FCmp { dst, cond: FloatCond::Ne, lhs, rhs },
        (HirBinOp::Lt, false) => Inst::ICmp { dst, cond: IntCond::Slt, lhs, rhs },
        (HirBinOp::Lt, true) => Inst::FCmp { dst, cond: FloatCond::OLt, lhs, rhs },
        (HirBinOp::Le, false) => Inst::ICmp { dst, cond: IntCond::Sle, lhs, rhs },
        (HirBinOp::Le, true) => Inst::FCmp { dst, cond: FloatCond::OLe, lhs, rhs },
        (HirBinOp::Gt, false) => Inst::ICmp { dst, cond: IntCond::Sgt, lhs, rhs },
        (HirBinOp::Gt, true) => Inst::FCmp { dst, cond: FloatCond::OGt, lhs, rhs },
        (HirBinOp::Ge, false) => Inst::ICmp { dst, cond: IntCond::Sge, lhs, rhs },
        (HirBinOp::Ge, true) => Inst::FCmp { dst, cond: FloatCond::OGe, lhs, rhs },

        // Logical (short-circuit not yet — treat as bitwise on Bool for now)
        (HirBinOp::LogAnd, _) => Inst::BAnd { dst, lhs, rhs },
        (HirBinOp::LogOr, _) => Inst::BOr { dst, lhs, rhs },

        // Unsupported pair — placeholder zero
        _ => {
            // No instruction emitted; rebind dst to a fresh zero const.
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: res_ty.clone(),
                    val: 0,
                },
            );
            return dst;
        }
    };
    ctx.push_inst(mir, inst);
    dst
}

fn lower_unary(
    op: HirUnOp,
    src: ValueId,
    res_ty: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let dst = mir.new_value(res_ty.clone());
    let inst = match op {
        HirUnOp::Neg if res_ty.is_float() => Inst::FNeg { dst, src },
        HirUnOp::Neg => Inst::INeg { dst, src },
        HirUnOp::Not => {
            // !b => xor with 1 (bool)
            let one = mir.new_value(HirType::Bool);
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst: one,
                    ty: HirType::Bool,
                    val: 1,
                },
            );
            Inst::BXor { dst, lhs: src, rhs: one }
        }
        HirUnOp::BitNot => Inst::BNot { dst, src },
        _ => {
            ctx.push_inst(
                mir,
                Inst::IConst {
                    dst,
                    ty: res_ty.clone(),
                    val: 0,
                },
            );
            return dst;
        }
    };
    ctx.push_inst(mir, inst);
    dst
}

fn lower_cast(
    src: ValueId,
    src_ty: &HirType,
    target: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    let dst = mir.new_value(target.clone());

    let inst = match (src_ty, target) {
        // int → wider int (sign-extend)
        (HirType::I8 | HirType::I16 | HirType::I32, HirType::I64) => {
            Inst::SExtend { dst, src, to: target.clone() }
        }
        // unsigned narrow → wider (zero-extend)
        (HirType::U8 | HirType::U16 | HirType::U32, HirType::I64 | HirType::U64) => {
            Inst::UExtend { dst, src, to: target.clone() }
        }
        // wider int → narrower
        (HirType::I64 | HirType::U64, HirType::I8 | HirType::I16 | HirType::I32) => {
            Inst::IReduce { dst, src, to: target.clone() }
        }
        // int → float
        (a, HirType::F32 | HirType::F64 | HirType::Number) if a.is_integer() => {
            Inst::CvtFromSint { dst, src, to: target.clone() }
        }
        // float → int (saturating)
        (HirType::F32 | HirType::F64 | HirType::Number, b) if b.is_integer() => {
            Inst::CvtToSintSat { dst, src, to: target.clone() }
        }
        // f32 ↔ f64
        (HirType::F32, HirType::F64 | HirType::Number) => Inst::FPromote { dst, src },
        (HirType::F64 | HirType::Number, HirType::F32) => Inst::FDemote { dst, src },

        // Same kind or unsupported pair — bitcast as a fallback (preserves bits)
        _ => Inst::Bitcast { dst, src, to: target.clone() },
    };
    ctx.push_inst(mir, inst);
    dst
}

fn emit_zero_const(ty: &HirType, mir: &mut MirFunc, ctx: &mut LowerCtx) -> ValueId {
    let normalized = match ty {
        HirType::Unknown | HirType::Any => HirType::I64,
        other => other.clone(),
    };
    let dst = mir.new_value(normalized.clone());
    let inst = match normalized {
        HirType::F32 => Inst::F32Const { dst, val: 0.0 },
        HirType::F64 | HirType::Number => Inst::F64Const { dst, val: 0.0 },
        ref t => Inst::IConst {
            dst,
            ty: t.clone(),
            val: 0,
        },
    };
    ctx.push_inst(mir, inst);
    dst
}
