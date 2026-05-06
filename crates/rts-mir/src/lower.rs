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

use std::collections::{HashMap, HashSet};

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

/// Tag identifying an inlinable intrinsic operation. Mirrors `rts_abi::Intrinsic`
/// without depending on it directly so users without rts-abi can still drive
/// the lowering. The codegen translates each tag into native Cranelift IR
/// (sqrt, fabs, fmin, fmax, etc.) instead of a `call` instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicTag {
    Sqrt,
    AbsF64,
    MinF64,
    MaxF64,
    AbsI64,
    MinI64,
    MaxI64,
}

/// Resolver that decides whether a `(ns, member)` pair has an inlinable
/// intrinsic. When the resolver returns `Some(tag)` the MIR lowering emits
/// the corresponding `Inst::Sqrt`/`Inst::FAbs`/etc. directly instead of a
/// `CallExtern`.
pub type IntrinsicResolver<'r> = &'r dyn Fn(&str, &str) -> Option<IntrinsicTag>;

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
    lower_func_full_with_intrinsics(func, scope, extern_resolver, None)
}

/// Same as `lower_func_full` plus an optional `IntrinsicResolver` for
/// inlining hot extern calls (sqrt, fabs, fmin, etc.) into native Cranelift
/// IR instead of `CallExtern`.
pub fn lower_func_full_with_intrinsics(
    func: &HirFunc,
    scope: &Scope,
    extern_resolver: Option<ExternResolver<'_>>,
    intrinsic_resolver: Option<IntrinsicResolver<'_>>,
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
        intrinsic_resolver,
        had_placeholders: false,
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

    mir.had_placeholders = ctx.had_placeholders;

    // Pos-pass: marca cada handle vivo como GC root via DeclareGcValue.
    // Heuristica: cada `CallExtern` cujo `ret_ty` é `Handle(_)` produz um
    // valor que pode escapar para outras fns ou ficar vivo durante uma
    // chamada subsequente que dispare GC. Inserir DeclareGcValue logo
    // apos a call garante que o Cranelift `declare_value_needs_stack_map`
    // marque o slot do `dst` no stack map.
    insert_gc_declares(&mut mir);

    mir
}

/// Walks every block and inserts `Inst::DeclareGcValue` after each
/// `Inst::CallExtern` whose return type is a `Handle(_)` and produces a
/// `dst` value. The DeclareGcValue is a no-op at runtime — the codegen
/// translates it into `builder.declare_value_needs_stack_map(value)`.
fn insert_gc_declares(mir: &mut MirFunc) {
    for block in mir.blocks.iter_mut() {
        let mut new_insts: Vec<Inst> = Vec::with_capacity(block.insts.len() * 2);
        for inst in block.insts.drain(..) {
            let to_decl = match &inst {
                Inst::CallExtern { dst: Some(d), ret_ty, .. }
                    if matches!(ret_ty, HirType::Handle(_)) =>
                {
                    Some(*d)
                }
                _ => None,
            };
            new_insts.push(inst);
            if let Some(d) = to_decl {
                new_insts.push(Inst::DeclareGcValue { val: d });
            }
        }
        block.insts = new_insts;
    }
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
    /// Loop stack: cada frame carrega (continue_target, break_target,
    /// phi_names_in_order). Os phi names sao usados por `break`/`continue`
    /// para construir os Jump args lendo de `ctx.env`.
    loop_stack: Vec<LoopFrame>,
    /// User fn signatures (name → (param_tys, ret_ty)) snapshotted from the
    /// HIR scope so `lower_expr` can emit `Inst::CallUser` with full type info.
    fn_sigs: HashMap<String, (Vec<HirType>, HirType)>,
    /// Optional resolver for `(namespace, method)` pairs to extern symbols.
    extern_resolver: Option<ExternResolver<'r>>,
    /// Optional intrinsic tagger; when present and a (ns, member) maps to
    /// `Some(tag)`, the MethodCall lowers to native IR ops instead of a
    /// CallExtern.
    intrinsic_resolver: Option<IntrinsicResolver<'r>>,
    /// True when an expression silently fell back to a placeholder zero
    /// const (e.g. unresolved member access). Caller can inspect
    /// `MirFunc.had_placeholders` (set on lower output) to bail out of MIR
    /// routing instead of silently returning wrong results.
    had_placeholders: bool,
}

/// Frame de loop ativo. `continue_target` recebe args com os mesmos phis
/// do header; `break_target` (exit) recebe args com mesmos phis. Para
/// `do-while`, o frame do continue (header de cond) tambem usa os phis.
struct LoopFrame {
    continue_target: BlockId,
    break_target: BlockId,
    /// Nomes das vars phi na ordem dos block params dos targets.
    phi_names: Vec<String>,
}

impl LowerCtx<'_> {
    fn push_inst(&self, mir: &mut MirFunc, inst: Inst) {
        mir.blocks[self.cursor as usize].insts.push(inst);
    }

    /// Constroi os Jump args correntes para os phis do frame mais interno.
    fn current_phi_args(&self, names: &[String]) -> Vec<ValueId> {
        names
            .iter()
            .map(|n| {
                *self
                    .env
                    .get(n)
                    .expect("phi var deve estar em env durante o loop")
            })
            .collect()
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
            if let Some(frame) = ctx.loop_stack.last() {
                let brk = frame.break_target;
                let args = ctx.current_phi_args(&frame.phi_names.clone());
                set_term(mir, ctx.cursor, Terminator::Jump { target: brk, args });
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
            if let Some(frame) = ctx.loop_stack.last() {
                let cont = frame.continue_target;
                let args = ctx.current_phi_args(&frame.phi_names.clone());
                set_term(mir, ctx.cursor, Terminator::Jump { target: cont, args });
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
    // Detecta vars mutadas no body (e cond) e cria block params phi no header.
    let mut mutated = collect_mutated_vars(body);
    collect_mut_in_expr(cond, &mut mutated);
    let phis = collect_loop_phis(&mutated, mir, ctx);

    let header_b = mir.new_block();
    let body_b = mir.new_block();
    let exit_b = mir.new_block();

    // Cria os params do header (1 por phi) e registra-os em env enquanto
    // lowerizamos cond+body.
    let mut header_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    let mut entry_args: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_name, init_v, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[header_b as usize].params.push((p, ty.clone()));
        header_param_ids.push(p);
        entry_args.push(*init_v);
    }
    // Snapshot do env pre-loop para restaurar se necessario; o exit-block
    // tambem recebera params com os valores correntes ao sair.
    let env_before: HashMap<String, ValueId> = ctx.env.clone();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Jump {
            target: header_b,
            args: entry_args,
        },
    );

    // Dentro do loop, env[name] aponta para o param do header (phi).
    ctx.cursor = header_b;
    ctx.terminated = false;
    for ((name, _, _), pid) in phis.iter().zip(header_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }

    // header: test condition
    let cond_v = lower_expr(cond, mir, ctx);
    // Snapshot dos valores correntes apos lower_expr(cond) — eles sao os
    // args do salto para o exit_b (caso cond falhe, levam os valores de
    // header_params, ja que cond nao muta).
    let header_exit_args: Vec<ValueId> = phis
        .iter()
        .map(|(name, _, _)| *ctx.env.get(name).unwrap())
        .collect();

    // exit_b recebe params para que apos o loop o env aponte para o valor
    // final.
    let mut exit_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_, _, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[exit_b as usize].params.push((p, ty.clone()));
        exit_param_ids.push(p);
    }

    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: header_exit_args,
        },
    );

    // body: jump back to header at end. Push loop frame so break/continue work.
    ctx.cursor = body_b;
    ctx.terminated = false;
    let phi_names: Vec<String> = phis.iter().map(|(n, _, _)| n.clone()).collect();
    ctx.loop_stack.push(LoopFrame {
        continue_target: header_b,
        break_target: exit_b,
        phi_names: phi_names.clone(),
    });
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        let back_args = ctx.current_phi_args(&phi_names);
        set_term(
            mir,
            ctx.cursor,
            Terminator::Jump {
                target: header_b,
                args: back_args,
            },
        );
    }

    ctx.cursor = exit_b;
    ctx.terminated = false;
    // Restaura env para snapshot pre-loop, depois reaponta as vars de phi
    // para os exit-block params (valor pos-loop).
    ctx.env = env_before;
    for ((name, _, _), pid) in phis.iter().zip(exit_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
}

fn lower_do_while(body: &[HirStmt], cond: &HirExpr, mir: &mut MirFunc, ctx: &mut LowerCtx) {
    let mut mutated = collect_mutated_vars(body);
    collect_mut_in_expr(cond, &mut mutated);
    let phis = collect_loop_phis(&mutated, mir, ctx);

    let body_b = mir.new_block();
    let header_b = mir.new_block();
    let exit_b = mir.new_block();

    // body_b carrega params (1 por phi), pois eh o entry do loop.
    let mut body_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    let mut entry_args: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_name, init_v, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[body_b as usize].params.push((p, ty.clone()));
        body_param_ids.push(p);
        entry_args.push(*init_v);
    }
    // header_b tambem carrega params para repassar valores ao back-edge.
    let mut header_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_, _, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[header_b as usize].params.push((p, ty.clone()));
        header_param_ids.push(p);
    }
    let mut exit_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_, _, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[exit_b as usize].params.push((p, ty.clone()));
        exit_param_ids.push(p);
    }
    let env_before = ctx.env.clone();

    set_term(mir, ctx.cursor, Terminator::Jump { target: body_b, args: entry_args });

    // body: env aponta pra body_params. continue jumps to header (cond test).
    ctx.cursor = body_b;
    ctx.terminated = false;
    for ((name, _, _), pid) in phis.iter().zip(body_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
    let phi_names: Vec<String> = phis.iter().map(|(n, _, _)| n.clone()).collect();
    ctx.loop_stack.push(LoopFrame {
        continue_target: header_b,
        break_target: exit_b,
        phi_names: phi_names.clone(),
    });
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        let args = ctx.current_phi_args(&phi_names);
        set_term(mir, ctx.cursor, Terminator::Jump { target: header_b, args });
    }

    // header: env aponta para header_params; testa cond.
    ctx.cursor = header_b;
    ctx.terminated = false;
    for ((name, _, _), pid) in phis.iter().zip(header_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
    let cond_v = lower_expr(cond, mir, ctx);
    let header_phi_args = ctx.current_phi_args(&phi_names);
    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: header_phi_args.clone(),
            else_block: exit_b,
            else_args: header_phi_args,
        },
    );

    ctx.cursor = exit_b;
    ctx.terminated = false;
    ctx.env = env_before;
    for ((name, _, _), pid) in phis.iter().zip(exit_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
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
    ctx.loop_stack.push(LoopFrame {
        continue_target: exit_b,
        break_target: exit_b,
        phi_names: Vec::new(),
    });
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
    // 1. init runs in current block. Pode declarar a var de loop (ex.
    // `let i = 0`) — passa a estar em env e sera candidato a phi.
    if let Some(s) = init {
        lower_stmt(s, mir, ctx);
        if ctx.terminated {
            return;
        }
    }

    // Coleta vars mutadas no body, cond e update.
    let mut mutated = collect_mutated_vars(body);
    if let Some(c) = cond {
        collect_mut_in_expr(c, &mut mutated);
    }
    if let Some(u) = update {
        collect_mut_in_expr(u, &mut mutated);
    }
    let phis = collect_loop_phis(&mutated, mir, ctx);

    let header_b = mir.new_block();
    let body_b = mir.new_block();
    let update_b = mir.new_block();
    let exit_b = mir.new_block();

    let mut header_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    let mut entry_args: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_name, init_v, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[header_b as usize].params.push((p, ty.clone()));
        header_param_ids.push(p);
        entry_args.push(*init_v);
    }
    let mut exit_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_, _, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[exit_b as usize].params.push((p, ty.clone()));
        exit_param_ids.push(p);
    }
    let env_before = ctx.env.clone();

    set_term(
        mir,
        ctx.cursor,
        Terminator::Jump { target: header_b, args: entry_args },
    );

    // header: env aponta pra header_params. testa cond.
    ctx.cursor = header_b;
    ctx.terminated = false;
    for ((name, _, _), pid) in phis.iter().zip(header_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
    let cond_v = match cond {
        Some(e) => lower_expr(e, mir, ctx),
        None => {
            let v = mir.new_value(HirType::Bool);
            ctx.push_inst(mir, Inst::IConst { dst: v, ty: HirType::Bool, val: 1 });
            v
        }
    };
    let phi_names: Vec<String> = phis.iter().map(|(n, _, _)| n.clone()).collect();
    let header_exit_args = ctx.current_phi_args(&phi_names);

    set_term(
        mir,
        ctx.cursor,
        Terminator::Brif {
            cond: cond_v,
            then_block: body_b,
            then_args: vec![],
            else_block: exit_b,
            else_args: header_exit_args,
        },
    );

    // body: continue → update_b, break → exit_b
    ctx.cursor = body_b;
    ctx.terminated = false;
    ctx.loop_stack.push(LoopFrame {
        continue_target: update_b,
        break_target: exit_b,
        phi_names: phi_names.clone(),
    });
    lower_stmts(body, mir, ctx);
    ctx.loop_stack.pop();
    if !ctx.terminated {
        let args = ctx.current_phi_args(&phi_names);
        set_term(mir, ctx.cursor, Terminator::Jump { target: update_b, args });
    }

    // update: bloco com mesmos params dos phis. Cria params, atualiza env
    // pra apontar pra eles, e roda update expr (que pode atualizar env via
    // Assign). Por fim salta pra header com env corrente.
    let mut update_param_ids: Vec<ValueId> = Vec::with_capacity(phis.len());
    for (_, _, ty) in &phis {
        let p = mir.new_value(ty.clone());
        mir.blocks[update_b as usize].params.push((p, ty.clone()));
        update_param_ids.push(p);
    }
    ctx.cursor = update_b;
    ctx.terminated = false;
    for ((name, _, _), pid) in phis.iter().zip(update_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
    if let Some(e) = update {
        let _ = lower_expr(e, mir, ctx);
    }
    let header_back_args = ctx.current_phi_args(&phi_names);
    set_term(
        mir,
        ctx.cursor,
        Terminator::Jump { target: header_b, args: header_back_args },
    );

    ctx.cursor = exit_b;
    ctx.terminated = false;
    ctx.env = env_before;
    for ((name, _, _), pid) in phis.iter().zip(exit_param_ids.iter()) {
        ctx.env.insert(name.clone(), *pid);
    }
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
                ctx.had_placeholders = true;
                emit_zero_const(&expr.ty, mir, ctx)
            }
        },

        HirExprKind::Bin { op, lhs, rhs } => {
            let mut lv = lower_expr(lhs, mir, ctx);
            let mut rv = lower_expr(rhs, mir, ctx);
            // Numeric promotion: if the result type is float but one operand
            // came in as integer (e.g. `a:number + 1` lowers `1` as IConst i64),
            // emit a CvtFromSint so the FAdd has matching types.
            if expr.ty.is_float() {
                if lhs.ty.is_integer() || matches!(lhs.ty, HirType::Unknown | HirType::Any) {
                    let promoted = mir.new_value(expr.ty.clone());
                    ctx.push_inst(mir, Inst::CvtFromSint {
                        dst: promoted,
                        src: lv,
                        to: expr.ty.clone(),
                    });
                    lv = promoted;
                }
                if rhs.ty.is_integer() || matches!(rhs.ty, HirType::Unknown | HirType::Any) {
                    let promoted = mir.new_value(expr.ty.clone());
                    ctx.push_inst(mir, Inst::CvtFromSint {
                        dst: promoted,
                        src: rv,
                        to: expr.ty.clone(),
                    });
                    rv = promoted;
                }
            }
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

        HirExprKind::Member { object, prop } => {
            // Namespace constants: `math.PI`, `math.E`, etc. The resolver
            // returns a zero-arg extern symbol that the runtime exposes
            // (e.g. `__RTS_FN_NS_MATH_PI`). We emit a `CallExtern` with no
            // args; Cranelift will emit a single `call` instruction that
            // returns the constant. This matches the runtime's existing
            // shape and avoids modeling raw data segments per constant.
            if let HirExprKind::Ident(ns_name) = &object.kind {
                if let Some(resolver) = ctx.extern_resolver {
                    if let Some((sym, param_tys, ret_ty)) = resolver(ns_name, prop) {
                        if param_tys.is_empty() {
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
                                    args: vec![],
                                    ret_ty: ret_ty.clone(),
                                    param_tys,
                                },
                            );
                            return dst.unwrap_or_else(|| emit_zero_const(&expr.ty, mir, ctx));
                        }
                    }
                }
            }

            // arr.length em handle de array → collections.vec_len(handle)
            if prop == "length" && matches!(object.ty, HirType::Array(_)) {
                if let Some(resolver) = ctx.extern_resolver {
                    if let Some((sym, _param_tys, ret_ty)) = resolver("collections", "vec_len") {
                        let h = lower_expr(object, mir, ctx);
                        let dst = mir.new_value(ret_ty.clone());
                        ctx.push_inst(
                            mir,
                            Inst::CallExtern {
                                dst: Some(dst),
                                sym,
                                args: vec![h],
                                ret_ty: ret_ty.clone(),
                                param_tys: vec![HirType::I64],
                            },
                        );
                        return dst;
                    }
                }
            }

            // Unresolved member access — placeholder zero (codegen AST handles
            // class fields, object literals, etc.).
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        HirExprKind::Array(items) => {
            // [a, b, c] → vec_new() + vec_push(handle, a) + vec_push(handle, b) + ...
            if let Some(resolver) = ctx.extern_resolver {
                if let (Some((new_sym, _, new_ret)), Some((push_sym, _, push_ret))) = (
                    resolver("collections", "vec_new"),
                    resolver("collections", "vec_push"),
                ) {
                    // Call vec_new() to get a fresh handle.
                    let handle = mir.new_value(new_ret.clone());
                    ctx.push_inst(
                        mir,
                        Inst::CallExtern {
                            dst: Some(handle),
                            sym: new_sym,
                            args: vec![],
                            ret_ty: new_ret,
                            param_tys: vec![],
                        },
                    );
                    // Push each item in order.
                    for item in items {
                        let v = lower_expr(item, mir, ctx);
                        ctx.push_inst(
                            mir,
                            Inst::CallExtern {
                                dst: None,
                                sym: push_sym.clone(),
                                args: vec![handle, v],
                                ret_ty: push_ret.clone(),
                                param_tys: vec![HirType::I64, HirType::I64],
                            },
                        );
                    }
                    return handle;
                }
            }
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        HirExprKind::Index { object, index } => {
            // arr[i] → collections.vec_get(handle, idx)
            if let Some(resolver) = ctx.extern_resolver {
                if let Some((sym, _param_tys, ret_ty)) = resolver("collections", "vec_get") {
                    let h = lower_expr(object, mir, ctx);
                    let i = lower_expr(index, mir, ctx);
                    let dst = mir.new_value(ret_ty.clone());
                    ctx.push_inst(
                        mir,
                        Inst::CallExtern {
                            dst: Some(dst),
                            sym,
                            args: vec![h, i],
                            ret_ty: ret_ty.clone(),
                            param_tys: vec![HirType::I64, HirType::I64],
                        },
                    );
                    return dst;
                }
            }
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        HirExprKind::MethodCall { object, method, args } => {
            // Try the intrinsic resolver first — if (ns, method) maps to a
            // known IntrinsicTag, emit native Cranelift IR ops directly
            // (sqrt/fabs/fmin/fmax/abs i64/min i64/max i64) instead of a
            // CallExtern. Always cheaper than a call.
            if let HirExprKind::Ident(ns_name) = &object.kind {
                if let Some(intr_resolver) = ctx.intrinsic_resolver {
                    if let Some(tag) = intr_resolver(ns_name, method) {
                        let arg_vals: Vec<ValueId> =
                            args.iter().map(|a| lower_expr(a, mir, ctx)).collect();
                        return emit_intrinsic(tag, &arg_vals, &expr.ty, mir, ctx);
                    }
                }
            }

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
            ctx.had_placeholders = true;
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
            ctx.had_placeholders = true;
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

        // Atribuicao a variavel local: `x = expr`. Atualiza env[x] para o
        // novo ValueId e retorna o valor atribuido (semantica JS).
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if ctx.env.contains_key(name) {
                    let v = lower_expr(value, mir, ctx);
                    ctx.env.insert(name.clone(), v);
                    return v;
                }
            }
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        // `x += rhs` => `x = x + rhs` (e demais ops compostos).
        HirExprKind::AssignOp { op, target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if let Some(&cur) = ctx.env.get(name) {
                    let rhs = lower_expr(value, mir, ctx);
                    let res = lower_bin(*op, cur, rhs, &expr.ty, &target.ty, mir, ctx);
                    ctx.env.insert(name.clone(), res);
                    return res;
                }
            }
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        // `++x` / `x++` / `--x` / `x--` — em ambos os casos retornamos o
        // novo valor (simplificacao consciente; pos-fix retornar valor
        // antigo eh follow-up).
        HirExprKind::PreInc(inner)
        | HirExprKind::PostInc(inner)
        | HirExprKind::PreDec(inner)
        | HirExprKind::PostDec(inner) => {
            let is_dec = matches!(
                &expr.kind,
                HirExprKind::PreDec(_) | HirExprKind::PostDec(_)
            );
            if let HirExprKind::Ident(name) = &inner.kind {
                if let Some(&cur) = ctx.env.get(name) {
                    let cur_ty = mir.values[cur as usize].clone();
                    let one = mir.new_value(cur_ty.clone());
                    if cur_ty.is_float() {
                        ctx.push_inst(mir, Inst::F64Const { dst: one, val: 1.0 });
                    } else {
                        ctx.push_inst(
                            mir,
                            Inst::IConst { dst: one, ty: cur_ty.clone(), val: 1 },
                        );
                    }
                    let dst = mir.new_value(cur_ty.clone());
                    let inst = if cur_ty.is_float() {
                        if is_dec {
                            Inst::FSub { dst, lhs: cur, rhs: one }
                        } else {
                            Inst::FAdd { dst, lhs: cur, rhs: one }
                        }
                    } else if is_dec {
                        Inst::ISub { dst, lhs: cur, rhs: one }
                    } else {
                        Inst::IAdd { dst, lhs: cur, rhs: one }
                    };
                    ctx.push_inst(mir, inst);
                    ctx.env.insert(name.clone(), dst);
                    return dst;
                }
            }
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }

        // Unsupported: emit a placeholder of the expected type and trap when
        // we leave the block (handled by caller observing `ctx.terminated`
        // after reaching a stmt that uses it). For now, emit an iconst 0.
        _ => {
            ctx.had_placeholders = true;
            emit_zero_const(&expr.ty, mir, ctx)
        }
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

/// Emit a native intrinsic op into the current block. Returns the ValueId
/// holding the result. When the argument count doesn't match the tag the
/// fn falls back to emit_zero_const (caller's MIR is malformed but we keep
/// the IR well-formed).
fn emit_intrinsic(
    tag: IntrinsicTag,
    args: &[ValueId],
    res_ty: &HirType,
    mir: &mut MirFunc,
    ctx: &mut LowerCtx,
) -> ValueId {
    match tag {
        IntrinsicTag::Sqrt if args.len() == 1 => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::Sqrt { dst, src: args[0] });
            dst
        }
        IntrinsicTag::AbsF64 if args.len() == 1 => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::FAbs { dst, src: args[0] });
            dst
        }
        IntrinsicTag::MinF64 if args.len() == 2 => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::FMin { dst, lhs: args[0], rhs: args[1] });
            dst
        }
        IntrinsicTag::MaxF64 if args.len() == 2 => {
            let dst = mir.new_value(HirType::F64);
            ctx.push_inst(mir, Inst::FMax { dst, lhs: args[0], rhs: args[1] });
            dst
        }
        IntrinsicTag::AbsI64 if args.len() == 1 => {
            // abs(x) = x < 0 ? -x : x
            let cond = mir.new_value(HirType::Bool);
            let zero = emit_zero_const(&HirType::I64, mir, ctx);
            ctx.push_inst(mir, Inst::ICmp {
                dst: cond,
                cond: IntCond::Slt,
                lhs: args[0],
                rhs: zero,
            });
            let neg = mir.new_value(HirType::I64);
            ctx.push_inst(mir, Inst::INeg { dst: neg, src: args[0] });
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(mir, Inst::Select {
                dst,
                cond,
                on_true: neg,
                on_false: args[0],
            });
            dst
        }
        IntrinsicTag::MinI64 if args.len() == 2 => {
            // min(a, b) = a < b ? a : b
            let cond = mir.new_value(HirType::Bool);
            ctx.push_inst(mir, Inst::ICmp {
                dst: cond,
                cond: IntCond::Slt,
                lhs: args[0],
                rhs: args[1],
            });
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(mir, Inst::Select {
                dst,
                cond,
                on_true: args[0],
                on_false: args[1],
            });
            dst
        }
        IntrinsicTag::MaxI64 if args.len() == 2 => {
            // max(a, b) = a > b ? a : b
            let cond = mir.new_value(HirType::Bool);
            ctx.push_inst(mir, Inst::ICmp {
                dst: cond,
                cond: IntCond::Sgt,
                lhs: args[0],
                rhs: args[1],
            });
            let dst = mir.new_value(HirType::I64);
            ctx.push_inst(mir, Inst::Select {
                dst,
                cond,
                on_true: args[0],
                on_false: args[1],
            });
            dst
        }
        // Arity mismatch — fall through with a placeholder; verify won't
        // catch this since it's purely a shape mistake from the resolver.
        _ => emit_zero_const(res_ty, mir, ctx),
    }
}

/// Walka recursivamente uma lista de stmts/exprs e coleta o conjunto de
/// nomes de identificadores que aparecem como alvo de atribuicao
/// (Assign/AssignOp/PreInc/PostInc/PreDec/PostDec). Loops aninhados sao
/// incluidos via recursao em cada arm.
fn collect_mutated_vars(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_mut_in_stmt(s, &mut out);
    }
    out
}

fn collect_mut_in_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Expr(e) | HirStmt::Throw(e) => collect_mut_in_expr(e, out),
        HirStmt::Return(opt) => {
            if let Some(e) = opt {
                collect_mut_in_expr(e, out);
            }
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                collect_mut_in_expr(e, out);
            }
        }
        HirStmt::Const { init, .. } => collect_mut_in_expr(init, out),
        HirStmt::If { cond, then, else_ } => {
            collect_mut_in_expr(cond, out);
            for s in then {
                collect_mut_in_stmt(s, out);
            }
            if let Some(es) = else_ {
                for s in es {
                    collect_mut_in_stmt(s, out);
                }
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            collect_mut_in_expr(cond, out);
            for s in body {
                collect_mut_in_stmt(s, out);
            }
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                collect_mut_in_stmt(i, out);
            }
            if let Some(c) = cond {
                collect_mut_in_expr(c, out);
            }
            if let Some(u) = update {
                collect_mut_in_expr(u, out);
            }
            for s in body {
                collect_mut_in_stmt(s, out);
            }
        }
        HirStmt::ForOf { iterable, body, .. } => {
            collect_mut_in_expr(iterable, out);
            for s in body {
                collect_mut_in_stmt(s, out);
            }
        }
        HirStmt::ForIn { object, body, .. } => {
            collect_mut_in_expr(object, out);
            for s in body {
                collect_mut_in_stmt(s, out);
            }
        }
        HirStmt::Try { body, catch, finally } => {
            for s in body {
                collect_mut_in_stmt(s, out);
            }
            if let Some(c) = catch {
                for s in &c.body {
                    collect_mut_in_stmt(s, out);
                }
            }
            if let Some(fin) = finally {
                for s in fin {
                    collect_mut_in_stmt(s, out);
                }
            }
        }
        HirStmt::Switch { discriminant, cases } => {
            collect_mut_in_expr(discriminant, out);
            for c in cases {
                if let Some(t) = &c.test {
                    collect_mut_in_expr(t, out);
                }
                for s in &c.body {
                    collect_mut_in_stmt(s, out);
                }
            }
        }
        HirStmt::Block(body) => {
            for s in body {
                collect_mut_in_stmt(s, out);
            }
        }
        HirStmt::Labeled { body, .. } => collect_mut_in_stmt(body, out),
        HirStmt::Break(_) | HirStmt::Continue(_) | HirStmt::Raw(_) => {}
    }
}

fn collect_mut_in_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match &expr.kind {
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                out.insert(name.clone());
            }
            collect_mut_in_expr(value, out);
        }
        HirExprKind::AssignOp { target, value, .. } => {
            if let HirExprKind::Ident(name) = &target.kind {
                out.insert(name.clone());
            }
            collect_mut_in_expr(value, out);
        }
        HirExprKind::PreInc(inner)
        | HirExprKind::PreDec(inner)
        | HirExprKind::PostInc(inner)
        | HirExprKind::PostDec(inner) => {
            if let HirExprKind::Ident(name) = &inner.kind {
                out.insert(name.clone());
            }
            collect_mut_in_expr(inner, out);
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            collect_mut_in_expr(lhs, out);
            collect_mut_in_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. } => collect_mut_in_expr(operand, out),
        HirExprKind::Call { callee, args } => {
            collect_mut_in_expr(callee, out);
            for a in args {
                collect_mut_in_expr(a, out);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            collect_mut_in_expr(object, out);
            for a in args {
                collect_mut_in_expr(a, out);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                collect_mut_in_expr(a, out);
            }
        }
        HirExprKind::Member { object, .. } => collect_mut_in_expr(object, out),
        HirExprKind::Index { object, index } => {
            collect_mut_in_expr(object, out);
            collect_mut_in_expr(index, out);
        }
        HirExprKind::Array(items) => {
            for e in items {
                collect_mut_in_expr(e, out);
            }
        }
        HirExprKind::Object(fields) => {
            for (_, e) in fields {
                collect_mut_in_expr(e, out);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            collect_mut_in_expr(cond, out);
            collect_mut_in_expr(then, out);
            collect_mut_in_expr(else_, out);
        }
        HirExprKind::Cast { expr, .. } => collect_mut_in_expr(expr, out),
        HirExprKind::Await(e) | HirExprKind::Spread(e) => collect_mut_in_expr(e, out),
        HirExprKind::Seq(items) => {
            for e in items {
                collect_mut_in_expr(e, out);
            }
        }
        HirExprKind::Lit(_)
        | HirExprKind::Ident(_)
        | HirExprKind::Arrow { .. }
        | HirExprKind::Raw(_) => {}
    }
}

/// Decide se uma variavel mutada vira block param do loop. Aceitamos tipos
/// numericos simples e Bool — outros tipos (Str, Array, classes) sao
/// deixados de fora e disparam `had_placeholders` para o codegen rotear via
/// AST.
fn is_loop_phi_compatible(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
            | HirType::F32
            | HirType::F64
            | HirType::Number
            | HirType::Bool
    )
}

/// Para cada nome em `mutated`, se existe em env e o tipo eh phi-compativel,
/// retorna (nome, valor_atual, tipo). Outras sao ignoradas e marcam
/// had_placeholders.
fn collect_loop_phis(
    mutated: &HashSet<String>,
    mir: &MirFunc,
    ctx: &mut LowerCtx,
) -> Vec<(String, ValueId, HirType)> {
    let mut phis: Vec<(String, ValueId, HirType)> = Vec::new();
    for name in mutated {
        if let Some(&v) = ctx.env.get(name) {
            let ty = mir.values[v as usize].clone();
            if is_loop_phi_compatible(&ty) {
                phis.push((name.clone(), v, ty));
            } else {
                ctx.had_placeholders = true;
            }
        }
    }
    // ordem deterministica
    phis.sort_by(|a, b| a.0.cmp(&b.0));
    phis
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
