//! Statement + control-flow lowering for the whole-program path, plus the
//! local-mutation helpers (`let`/assignment/`++`/`--`).
//!
//! Control flow uses real Cranelift blocks; locals are Cranelift [`Variable`]s
//! so SSA φ-nodes at `if`/`while` merges are constructed by the builder. A
//! condition is reduced through JS `ToBoolean` ([`Lowerer::as_bool_value`]), so
//! `if (x)` / `while (i)` over numbers or Tagged values work, not just booleans.

use cranelift_codegen::ir::InstBuilder;
use cranelift_module::Module;

use rts_hir::ir::{HirExprKind, HirLit};
use rts_hir::{HirBinOp, HirExpr, HirStmt, HirType};

use crate::repr::Repr;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a single statement.
    pub(super) fn lower_stmt(&mut self, module: &mut dyn Module, s: &HirStmt) -> FrontResult<()> {
        match s {
            HirStmt::Return(arg) => self.lower_return(module, arg.as_ref()),
            HirStmt::Let { name, ty, init } => self.lower_let(module, name, ty, init.as_ref()),
            HirStmt::Const { name, ty, init } => self.lower_let(module, name, ty, Some(init)),
            HirStmt::Expr(e) => {
                self.lower_expr(module, e)?;
                Ok(())
            }
            HirStmt::If { cond, then, else_ } => {
                self.lower_if(module, cond, then, else_.as_deref())
            }
            HirStmt::While { cond, body } => self.lower_while(module, cond, body),
            HirStmt::DoWhile { body, cond } => self.lower_do_while(module, body, cond),
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => self.lower_for(
                module,
                init.as_deref(),
                cond.as_ref(),
                update.as_ref(),
                body,
            ),
            HirStmt::ForOf {
                binding,
                binding_ty,
                iterable,
                body,
            } => self.lower_for_of(module, binding, binding_ty, iterable, body),
            HirStmt::ForIn {
                binding,
                object,
                body,
            } => self.lower_for_in(module, binding, object, body),
            HirStmt::Break(label) => self.lower_break(module, label.as_deref()),
            HirStmt::Continue(label) => self.lower_continue(module, label.as_deref()),
            // A labeled NON-loop statement (`block1: { … break block1; … }`) is a
            // valid JS `break`-only target — lowered with its own break-target
            // `LoopCtx` (see `lower_labeled_block`).
            HirStmt::Labeled { label, body } if !super::breakcont::stmt_consumes_label(body) => {
                self.lower_labeled_block(module, label, body)
            }
            // A labeled LOOP/SWITCH: the construct itself `take`s the pending label
            // into its own `LoopCtx`. Clear it after so it never leaks to a later
            // loop.
            HirStmt::Labeled { label, body } => {
                self.pending_label = Some(label.clone());
                self.lower_stmt(module, body)?;
                self.pending_label = None;
                Ok(())
            }
            // An EXPLICIT `{ … }` block is a LEXICAL SCOPE: a `let`/`const`
            // declared inside must not leak (JS block scoping) — save the
            // name-keyed binding maps, lower, restore. An inner `let a` gets a
            // FRESH Variable (see `fresh_local_var`), so the outer `a` keeps its
            // register AND its map entry after the block. Reassignments to
            // OUTER names persist (they `def_var` the outer Variable — the map
            // entry itself is untouched). `var` declarations were rewritten to
            // plain assignments by the hoist pass, so they escape correctly.
            //
            // EXCEPTION — a DECLARATIONS-ONLY block: rts-hir encodes a
            // multi-declarator (`let i = 0, j = 10` — one statement, NO lexical
            // scope of its own) as a Block of individual Lets, indistinguishable
            // from a real `{ let … }` here. Lower it WITHOUT the scope restore:
            // the multi-declarator NEEDS its names to escape (a `for` header /
            // plain statement declares into the enclosing scope), and for a real
            // declarations-only block the extra visibility can never change a
            // VALID program's value (nothing else inside the block observes the
            // names; an invalid out-of-scope read is tsc's to reject).
            HirStmt::Block(stmts)
                if !stmts.is_empty()
                    && stmts
                        .iter()
                        .all(|s| matches!(s, HirStmt::Let { .. } | HirStmt::Const { .. })) =>
            {
                self.lower_block(module, stmts)
            }
            HirStmt::Block(stmts) => {
                let saved_locals = self.locals.clone();
                let saved_shapes = self.local_shapes.clone();
                let saved_obj = self.object_locals.clone();
                let saved_str = self.string_locals.clone();
                let saved_classes = self.local_classes.clone();
                let saved_class_refs = self.local_class_refs.clone();
                let saved_gen = self.generator_locals.clone();
                let saved_glob = self.global_instance_classes.clone();
                let r = self.lower_block(module, stmts);
                self.locals = saved_locals;
                self.local_shapes = saved_shapes;
                self.object_locals = saved_obj;
                self.string_locals = saved_str;
                self.local_classes = saved_classes;
                self.local_class_refs = saved_class_refs;
                self.generator_locals = saved_gen;
                self.global_instance_classes = saved_glob;
                r
            }
            HirStmt::Throw(arg) => self.lower_throw(module, arg),
            HirStmt::Try {
                body,
                catch,
                finally,
            } => self.lower_try(module, body, catch.as_ref(), finally.as_deref()),
            HirStmt::Switch {
                discriminant,
                cases,
            } => self.lower_switch(module, discriminant, cases),
            HirStmt::Raw(text) => unsupported!("unrecognized statement `{}`", text.trim()),
            other => unsupported!("statement {}", stmt_name(other)),
        }
    }

    fn lower_return(&mut self, module: &mut dyn Module, arg: Option<&HirExpr>) -> FrontResult<()> {
        // Compute the return VALUE first (JS evaluates the return expression BEFORE
        // running any enclosing `finally`), then — if inside a try/finally — run every
        // enclosing finalizer innermost-first, and only THEN emit the real `return`.
        let ret_val: Option<cranelift_codegen::ir::Value> = match (self.ret, arg) {
            (None, None) => None,
            (None, Some(e)) => {
                // value returned from a void context: evaluate for effects, drop.
                self.lower_expr(module, e)?;
                None
            }
            (Some(ret), None) => {
                // JS `return;` yields `undefined` — a lifted arrow/callback with
                // an early `return;` returns the undefined word coerced to the
                // fn's return repr (Tagged for a lifted arrow, the common case).
                let undef = self.builder.ins().iconst(
                    cranelift_codegen::ir::types::I64,
                    crate::value::PolyValue::undefined().raw() as i64,
                );
                let v = self.coerce(
                    super::lower::Val::tagged_kind(undef, super::lower::JsKind::Undefined),
                    ret,
                )?;
                Some(v)
            }
            (Some(ret), Some(e)) => {
                // TAIL CALL (TCO): `return f(args)` where both this fn and `f`
                // are in the program's tail set (CallConv::Tail) and the site
                // qualifies (no enclosing try/finally, exact arity, matching
                // return repr) → ONE `return_call`, no frame growth. All checks
                // run before any lowering, so the fallback below never
                // double-evaluates. See `super::tco`.
                if self.try_tail_return_call(module, e)? {
                    return Ok(());
                }
                let v = self.lower_expr(module, e)?;
                // A PROVEN HEAP value returned through an `Int*`-declared fn
                // (`function make(): number { return new (F as any)(); }` — the
                // handle-as-number interop surface): the numeric decode would
                // read the OBJECT word as NaN→0. Route through the
                // tag-dispatched `__rtsadp_word_to_abi_i64` (heap → real handle
                // id, number → truncation) — the same rule the registry marshal
                // applies to a U64 param. Numeric kinds keep the pure-IR path.
                if matches!(ret, Repr::Int32 | Repr::Int64)
                    && matches!(v.repr, Repr::Tagged)
                    && matches!(
                        v.kind,
                        super::lower::JsKind::Object
                            | super::lower::JsKind::Array
                            | super::lower::JsKind::Str
                            | super::lower::JsKind::Function
                    )
                {
                    let w = self.box_value(v);
                    let h = self
                        .call_runtime(module, "__rtsadp_word_to_abi_i64", &[w])?
                        .expect("__rtsadp_word_to_abi_i64 returns a value");
                    Some(h)
                } else {
                    Some(self.coerce(v, ret)?)
                }
            }
        };

        if !self.finally_stack.is_empty() {
            // Run each enclosing finalizer innermost-first. While finalizer `i` lowers,
            // only the OUTER finalizers (0..i) stay active so a `return` inside it runs
            // them but not itself. If a finalizer terminates the block (its own
            // `return`/`throw`), it OVERRIDES this return — stop without emitting it.
            let finallys = std::mem::take(&mut self.finally_stack);
            for i in (0..finallys.len()).rev() {
                self.finally_stack = finallys[..i].to_vec();
                self.lower_block(module, &finallys[i])?;
                if self.block_terminated {
                    self.finally_stack = finallys;
                    return Ok(());
                }
            }
            self.finally_stack = finallys;
        }

        match ret_val {
            Some(v) => {
                self.builder.ins().return_(&[v]);
            }
            None => {
                self.builder.ins().return_(&[]);
            }
        }
        self.block_terminated = true;
        Ok(())
    }

    /// `obj.prop++` / `arr[i]--` (side-effect-free object, already checked): read the
    /// OLD value, store `target = target ± 1` via a synthesized member/index `=`
    /// (which returns the NEW value), and produce NEW for prefix / OLD for postfix.
    /// The object is re-read for the load and inside the store's RHS; harmless for a
    /// bare-ident object on a data slot (the accessor case bailed in `lower_incdec`).
    pub(super) fn lower_member_index_incdec(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        inc: bool,
        prefix: bool,
    ) -> FrontResult<Val> {
        // OLD value (only needed for postfix, but reading it first matches the read-
        // before-write order; for prefix we discard it).
        let old = self.lower_expr(module, target)?;
        // target = (target <Add|Sub> 1)
        let one = HirExpr::new(HirExprKind::Lit(HirLit::Int(1)), HirType::I64);
        let op = if inc { HirBinOp::Add } else { HirBinOp::Sub };
        let new_value = HirExpr::new(
            HirExprKind::Bin {
                op,
                lhs: Box::new(target.clone()),
                rhs: Box::new(one),
            },
            HirType::Unknown,
        );
        let new = self.lower_assign(module, target, &new_value)?;
        Ok(if prefix { new } else { old })
    }

    // ---- control flow ----

    fn lower_if(
        &mut self,
        module: &mut dyn Module,
        cond: &HirExpr,
        then: &[HirStmt],
        else_: Option<&[HirStmt]>,
    ) -> FrontResult<()> {
        let c = self.lower_expr(module, cond)?;
        let cond_v = self.as_bool_value(module, c)?;

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let cont_block = self.builder.create_block();

        self.builder
            .ins()
            .brif(cond_v, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        self.block_terminated = false;
        self.lower_block(module, then)?;
        let then_falls_through = !self.block_terminated;
        if then_falls_through {
            self.builder.ins().jump(cont_block, &[]);
        }

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        self.block_terminated = false;
        if let Some(else_body) = else_ {
            self.lower_block(module, else_body)?;
        }
        let else_falls_through = !self.block_terminated;
        if else_falls_through {
            self.builder.ins().jump(cont_block, &[]);
        }

        self.builder.seal_block(cont_block);
        self.builder.switch_to_block(cont_block);
        self.block_terminated = !(then_falls_through || else_falls_through);
        Ok(())
    }

    fn lower_while(
        &mut self,
        module: &mut dyn Module,
        cond: &HirExpr,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let c = self.lower_expr(module, cond)?;
        let cond_v = self.as_bool_value(module, c)?;
        self.builder
            .ins()
            .brif(cond_v, body_block, &[], exit_block, &[]);

        self.builder.switch_to_block(body_block);
        self.builder.seal_block(body_block);
        self.block_terminated = false;
        // `continue` re-tests (jump to header); `break` exits.
        let label = self.pending_label.take();
        self.loop_stack.push(super::lower::LoopCtx {
            exit: exit_block,
            continue_target: header,
            label,
            finally_depth: self.finally_stack.len(),
        });
        self.lower_block(module, body)?;
        self.loop_stack.pop();
        if !self.block_terminated {
            self.builder.ins().jump(header, &[]);
        }

        self.builder.seal_block(header);

        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// `do { body } while (cond)` — like `while`, but the body runs ONCE before the
    /// first test (the test is at the BOTTOM). `continue` re-tests (jumps to the
    /// condition block, NOT the top), `break` exits.
    fn lower_do_while(
        &mut self,
        module: &mut dyn Module,
        body: &[HirStmt],
        cond: &HirExpr,
    ) -> FrontResult<()> {
        let body_block = self.builder.create_block();
        let cond_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(body_block, &[]);

        // ---- body: runs first; `continue` → cond_block, `break` → exit_block ----
        self.builder.switch_to_block(body_block);
        self.block_terminated = false;
        let label = self.pending_label.take();
        self.loop_stack.push(super::lower::LoopCtx {
            exit: exit_block,
            continue_target: cond_block,
            label,
            finally_depth: self.finally_stack.len(),
        });
        self.lower_block(module, body)?;
        self.loop_stack.pop();
        if !self.block_terminated {
            self.builder.ins().jump(cond_block, &[]);
        }

        // ---- cond: re-test at the BOTTOM → body_block else exit ----
        // Sealed now: its predecessors (the body fall-through + every `continue`)
        // are all emitted.
        self.builder.seal_block(cond_block);
        self.builder.switch_to_block(cond_block);
        self.block_terminated = false;
        let c = self.lower_expr(module, cond)?;
        let cond_v = self.as_bool_value(module, c)?;
        self.builder
            .ins()
            .brif(cond_v, body_block, &[], exit_block, &[]);

        // `body_block` preds (the entry jump + the cond true edge) are now emitted.
        self.builder.seal_block(body_block);
        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// If `init` is a CALL to a generator constructor, its kind: `Some(true)` lazy
    /// (GenState handle), `Some(false)` eager (`__gen_buf` array). `None` otherwise.
    pub(super) fn gen_call_kind(&self, init: &HirExpr) -> Option<bool> {
        let HirExprKind::Call { callee, .. } = &init.kind else {
            return None;
        };
        let HirExprKind::Ident(f) = &callee.kind else {
            return None;
        };
        let s = self.sigs.get(f)?;
        if s.ret_lazy_gen {
            Some(true)
        } else if s.ret_eager_gen {
            Some(false)
        } else {
            None
        }
    }

    /// The runtime GenState/Vec handle of a GENERATOR-valued receiver (a generator
    /// local, or a direct `g()` call), for `GENERATOR_NEXT`/`RETURN`/`THROW`. EAGER
    /// (`__gen_buf` array word) → its real Vec handle (`POLY_TO_HANDLE`); LAZY (raw
    /// `Int64` GenState handle) → verbatim. `Ok(None)` when not a generator.
    pub(super) fn generator_receiver_handle(
        &mut self,
        module: &mut dyn Module,
        recv: &HirExpr,
    ) -> FrontResult<Option<cranelift_codegen::ir::Value>> {
        let is_lazy = match &recv.kind {
            HirExprKind::Ident(n) => match self.generator_locals.get(n) {
                Some(&lazy) => lazy,
                None => return Ok(None),
            },
            HirExprKind::Call { .. } => match self.gen_call_kind(recv) {
                Some(lazy) => lazy,
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        let v = self.lower_expr(module, recv)?;
        let handle = if is_lazy {
            self.coerce(v, Repr::Int64)?
        } else {
            let word = self.box_value(v);
            crate::value::emit_marshal::emit_table_load(module, self.builder, word)
        };
        Ok(Some(handle))
    }
}

/// Require an assignment/increment target to be a bare identifier.
pub(super) fn ident_target(target: &HirExpr) -> FrontResult<String> {
    match &target.kind {
        HirExprKind::Ident(name) => Ok(name.clone()),
        _ => unsupported!("assignment target must be a simple identifier"),
    }
}

/// A readable name for an unsupported statement variant.
fn stmt_name(s: &HirStmt) -> &'static str {
    match s {
        HirStmt::Expr(_) => "expression",
        HirStmt::Return(_) => "return",
        HirStmt::Let { .. } => "let",
        HirStmt::Const { .. } => "const",
        HirStmt::If { .. } => "if",
        HirStmt::While { .. } => "while",
        HirStmt::DoWhile { .. } => "do-while",
        HirStmt::For { .. } => "for",
        HirStmt::ForOf { .. } => "for-of",
        HirStmt::ForIn { .. } => "for-in",
        HirStmt::Break(_) => "break",
        HirStmt::Continue(_) => "continue",
        HirStmt::Try { .. } => "try",
        HirStmt::Throw(_) => "throw",
        HirStmt::Switch { .. } => "switch",
        HirStmt::Block(_) => "block",
        HirStmt::Labeled { .. } => "labeled",
        HirStmt::Raw(_) => "raw",
    }
}

/// A readable name for an unsupported expression variant (used by `expr.rs`).
pub(super) fn expr_variant_name(k: &HirExprKind) -> &'static str {
    match k {
        HirExprKind::Lit(_) => "literal",
        HirExprKind::Ident(_) => "identifier",
        HirExprKind::Bin { .. } => "binary",
        HirExprKind::Unary { .. } => "unary",
        HirExprKind::Assign { .. } => "assignment",
        HirExprKind::AssignOp { .. } => "compound-assignment",
        HirExprKind::Call { .. } => "call",
        HirExprKind::MethodCall { .. } => "method-call",
        HirExprKind::New { .. } => "new",
        HirExprKind::Member { .. } => "member-access",
        HirExprKind::Index { .. } => "index",
        HirExprKind::Array(_) => "array-literal",
        HirExprKind::Object(_) => "object-literal",
        HirExprKind::Ternary { .. } => "ternary",
        HirExprKind::Await(_) => "await",
        HirExprKind::Cast { .. } => "cast",
        HirExprKind::Arrow { .. } => "arrow",
        HirExprKind::PreInc(_) => "pre-increment",
        HirExprKind::PreDec(_) => "pre-decrement",
        HirExprKind::PostInc(_) => "post-increment",
        HirExprKind::PostDec(_) => "post-decrement",
        HirExprKind::Spread(_) => "spread",
        HirExprKind::Seq(_) => "sequence",
        HirExprKind::Raw(_) => "raw/unrecognized",
    }
}
