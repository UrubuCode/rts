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
            HirStmt::Break(label) => self.lower_break(label.as_deref()),
            HirStmt::Continue(label) => self.lower_continue(label.as_deref()),
            HirStmt::Block(stmts) => self.lower_block(module, stmts),
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
        match (self.ret, arg) {
            (None, None) => {
                // void `return;` inside main.
                self.builder.ins().return_(&[]);
            }
            (None, Some(e)) => {
                // value returned from a void context: evaluate for effects, drop.
                self.lower_expr(module, e)?;
                self.builder.ins().return_(&[]);
            }
            (Some(_ret), None) => {
                return unsupported!("`return;` with no value in a value-returning function");
            }
            (Some(ret), Some(e)) => {
                // TAIL-CALL OPTIMIZATION: `return f(args)` to a tail-callable user
                // function whose return repr matches ours lowers to a Cranelift
                // `return_call` (constant stack — deep tail recursion no longer
                // overflows). Falls back to a normal call+return when ANY condition
                // is unmet (kept conservative and sound — never a wrong value).
                if self.try_tail_return(module, e, ret)? {
                    self.block_terminated = true;
                    return Ok(());
                }
                let v = self.lower_expr(module, e)?;
                let coerced = self.coerce(v, ret)?;
                self.builder.ins().return_(&[coerced]);
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
        self.loop_stack.push(super::lower::LoopCtx {
            exit: exit_block,
            continue_target: header,
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
