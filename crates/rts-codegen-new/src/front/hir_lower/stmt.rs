//! Statement + control-flow lowering for the numeric subset, plus the
//! local-binding mutation helpers (`let`/assignment/`++`/`--`) shared with
//! [`super::expr`].
//!
//! Control flow uses real Cranelift blocks; locals are Cranelift [`Variable`]s,
//! so the `FunctionBuilder`'s SSA construction inserts the φ-nodes at `if`/`while`
//! merges automatically — no hand-rolled block params. `if`/`while` are
//! structured: each opens its own blocks, seals them at the right time, and
//! resumes at a continuation block.

use cranelift_codegen::ir::{InstBuilder, types};

use rts_hir::ir::HirExprKind;
use rts_hir::{HirBinOp, HirExpr, HirStmt, HirType};

use crate::repr::Repr;

use super::super::error::{FrontResult, Unsupported, unsupported};
use super::super::repr_map::repr_of;
use super::{Local, Lowerer, Val, cl_type};

impl<'a, 'b> Lowerer<'a, 'b> {
    /// Look up a local binding by name.
    pub(super) fn local(&self, name: &str) -> Option<Local> {
        self.locals.get(name).copied()
    }

    /// Lower a single statement.
    pub(super) fn lower_stmt(&mut self, s: &HirStmt) -> FrontResult<()> {
        match s {
            HirStmt::Return(arg) => self.lower_return(arg.as_ref()),
            HirStmt::Let { name, ty, init } => self.lower_let(name, ty, init.as_ref()),
            HirStmt::Const { name, ty, init } => self.lower_let(name, ty, Some(init)),
            HirStmt::Expr(e) => {
                // Evaluate for side effects (assignment / ++). Drop the value.
                self.lower_expr(e)?;
                Ok(())
            }
            HirStmt::If { cond, then, else_ } => self.lower_if(cond, then, else_.as_deref()),
            HirStmt::While { cond, body } => self.lower_while(cond, body),
            HirStmt::Block(stmts) => self.lower_block(stmts),
            HirStmt::Raw(text) => unsupported!("unrecognized statement `{}`", text.trim()),
            other => unsupported!("statement {}", stmt_name(other)),
        }
    }

    fn lower_return(&mut self, arg: Option<&HirExpr>) -> FrontResult<()> {
        let Some(e) = arg else {
            return unsupported!("`return;` with no value (numeric subset returns a number)");
        };
        let v = self.lower_expr(e)?;
        let coerced = self.coerce(v, self.ret)?;
        self.builder.ins().return_(&[coerced]);
        self.block_terminated = true;
        Ok(())
    }

    /// `let`/`const` with an initializer. The local's repr comes from the
    /// annotation when it is numeric, else from the initializer's value repr.
    fn lower_let(&mut self, name: &str, ty: &HirType, init: Option<&HirExpr>) -> FrontResult<()> {
        let Some(init) = init else {
            return unsupported!("`let {name}` without an initializer");
        };
        let val = self.lower_expr(init)?;

        // Prefer an explicit numeric annotation; otherwise take the init repr.
        let annotated = repr_of(ty);
        let repr = if annotated.is_unboxed() {
            annotated
        } else {
            val.repr
        };
        if !repr.is_unboxed() {
            return unsupported!("`let {name}` has non-numeric type {ty:?}");
        }

        let coerced = self.coerce(val, repr)?;
        let var = self.builder.declare_var(cl_type(repr));
        self.builder.def_var(var, coerced);
        self.locals.insert(name.to_string(), Local { var, repr });
        Ok(())
    }

    /// Plain assignment `x = e` (as an expression — returns the assigned value).
    /// Only assignment to an existing numeric local is supported.
    pub(super) fn lower_assign(&mut self, target: &HirExpr, value: &HirExpr) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("assignment to unbound `{name}`")))?;
        let val = self.lower_expr(value)?;
        let coerced = self.coerce(val, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val {
            v: coerced,
            repr: local.repr,
        })
    }

    /// Compound assignment `x += e` etc. Desugars to `x = x <op> e`.
    pub(super) fn lower_assign_op(
        &mut self,
        op: HirBinOp,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("compound-assign to unbound `{name}`")))?;
        let cur = self.builder.use_var(local.var);
        let cur_val = Val {
            v: cur,
            repr: local.repr,
        };
        let rhs = self.lower_expr(value)?;
        let result = if op.is_arithmetic() {
            self.lower_arith(op, cur_val, rhs)?
        } else {
            return unsupported!("compound-assign operator {op:?}");
        };
        let coerced = self.coerce(result, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val {
            v: coerced,
            repr: local.repr,
        })
    }

    /// `++x` / `x++` / `--x` / `x--` on a numeric local. `prefix` selects whether
    /// the produced value is the new (prefix) or old (postfix) one.
    pub(super) fn lower_incdec(
        &mut self,
        target: &HirExpr,
        inc: bool,
        prefix: bool,
    ) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("`++`/`--` on unbound `{name}`")))?;
        let old = self.builder.use_var(local.var);
        let new = match local.repr {
            Repr::Int32 | Repr::Int64 => {
                let one = self.builder.ins().iconst(types::I64, 1);
                if inc {
                    self.builder.ins().iadd(old, one)
                } else {
                    self.builder.ins().isub(old, one)
                }
            }
            Repr::Float64 => {
                let one = self.builder.ins().f64const(1.0);
                if inc {
                    self.builder.ins().fadd(old, one)
                } else {
                    self.builder.ins().fsub(old, one)
                }
            }
            other => return unsupported!("`++`/`--` on repr {other:?}"),
        };
        self.builder.def_var(local.var, new);
        let produced = if prefix { new } else { old };
        Ok(Val {
            v: produced,
            repr: local.repr,
        })
    }

    // ---- control flow ----

    fn lower_if(
        &mut self,
        cond: &HirExpr,
        then: &[HirStmt],
        else_: Option<&[HirStmt]>,
    ) -> FrontResult<()> {
        let c = self.lower_expr(cond)?;
        let cond_v = self.as_bool_value(c)?;

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let cont_block = self.builder.create_block();

        self.builder
            .ins()
            .brif(cond_v, then_block, &[], else_block, &[]);

        // then arm
        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        self.block_terminated = false;
        self.lower_block(then)?;
        let then_falls_through = !self.block_terminated;
        if then_falls_through {
            self.builder.ins().jump(cont_block, &[]);
        }

        // else arm
        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        self.block_terminated = false;
        if let Some(else_body) = else_ {
            self.lower_block(else_body)?;
        }
        let else_falls_through = !self.block_terminated;
        if else_falls_through {
            self.builder.ins().jump(cont_block, &[]);
        }

        // continuation — reachable iff at least one arm fell through.
        self.builder.seal_block(cont_block);
        if then_falls_through || else_falls_through {
            self.builder.switch_to_block(cont_block);
            self.block_terminated = false;
        } else {
            // Both arms returned: the continuation is dead, but Cranelift still
            // needs the current insertion point to be a valid (terminated) block.
            // Switch into the (empty, sealed) cont and mark terminated so the
            // driver doesn't try to append to a fallthrough we never created.
            self.builder.switch_to_block(cont_block);
            self.block_terminated = true;
        }
        Ok(())
    }

    fn lower_while(&mut self, cond: &HirExpr, body: &[HirStmt]) -> FrontResult<()> {
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(header, &[]);

        // header: evaluate cond, branch.
        self.builder.switch_to_block(header);
        let c = self.lower_expr(cond)?;
        let cond_v = self.as_bool_value(c)?;
        self.builder
            .ins()
            .brif(cond_v, body_block, &[], exit_block, &[]);

        // body: lower, then jump back to header (unless it returned).
        self.builder.switch_to_block(body_block);
        self.builder.seal_block(body_block);
        self.block_terminated = false;
        self.lower_block(body)?;
        if !self.block_terminated {
            self.builder.ins().jump(header, &[]);
        }

        // All predecessors of `header` (entry jump + body back-edge) are now
        // emitted, so it can be sealed.
        self.builder.seal_block(header);

        // exit: resume normal control here.
        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }
}

/// Require an assignment/increment target to be a bare identifier.
fn ident_target(target: &HirExpr) -> FrontResult<String> {
    match &target.kind {
        HirExprKind::Ident(name) => Ok(name.clone()),
        _ => unsupported!("assignment target must be a simple identifier in the numeric subset"),
    }
}

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
