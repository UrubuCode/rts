//! Statement + control-flow lowering for the whole-program path, plus the
//! local-mutation helpers (`let`/assignment/`++`/`--`).
//!
//! Control flow uses real Cranelift blocks; locals are Cranelift [`Variable`]s
//! so SSA φ-nodes at `if`/`while` merges are constructed by the builder. A
//! condition is reduced through JS `ToBoolean` ([`Lowerer::as_bool_value`]), so
//! `if (x)` / `while (i)` over numbers or Tagged values work, not just booleans.

use cranelift_codegen::ir::{types, InstBuilder};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirBinOp, HirExpr, HirStmt, HirType};

use crate::repr::Repr;

use crate::front::error::{unsupported, FrontResult, Unsupported};
use crate::front::repr_map::repr_of;

use super::lower::{cl_type, Local, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a single statement.
    pub(super) fn lower_stmt(
        &mut self,
        module: &mut dyn Module,
        s: &HirStmt,
    ) -> FrontResult<()> {
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
            HirStmt::Block(stmts) => self.lower_block(module, stmts),
            HirStmt::Raw(text) => unsupported!("unrecognized statement `{}`", text.trim()),
            other => unsupported!("statement {}", stmt_name(other)),
        }
    }

    fn lower_return(
        &mut self,
        module: &mut dyn Module,
        arg: Option<&HirExpr>,
    ) -> FrontResult<()> {
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
                let v = self.lower_expr(module, e)?;
                let coerced = self.coerce(v, ret)?;
                self.builder.ins().return_(&[coerced]);
            }
        }
        self.block_terminated = true;
        Ok(())
    }

    /// `let`/`const` with an initializer. The local's repr is the annotation when
    /// numeric; otherwise the initializer's value repr.
    fn lower_let(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        ty: &HirType,
        init: Option<&HirExpr>,
    ) -> FrontResult<()> {
        let Some(init) = init else {
            return unsupported!("`let {name}` without an initializer");
        };
        let val = self.lower_expr(module, init)?;

        let annotated = repr_of(ty);
        let repr = if annotated.is_unboxed() { annotated } else { val.repr };

        let coerced = self.coerce(val, repr)?;
        let var = self.builder.declare_var(cl_type(repr));
        self.builder.def_var(var, coerced);
        self.locals.insert(name.to_string(), Local { var, repr });
        Ok(())
    }

    /// Plain assignment `x = e`. Only to an existing local; the value coerces to
    /// the local's repr.
    pub(super) fn lower_assign(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("assignment to unbound `{name}`")))?;
        let val = self.lower_expr(module, value)?;
        let coerced = self.coerce(val, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val::new(coerced, local.repr))
    }

    /// Compound assignment `x += e` etc. → `x = x <op> e`.
    pub(super) fn lower_assign_op(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("compound-assign to unbound `{name}`")))?;
        let cur = self.builder.use_var(local.var);
        let cur_val = Val::new(cur, local.repr);
        let rhs = self.lower_expr(module, value)?;
        let result = if op.is_arithmetic() {
            self.lower_arith(module, op, cur_val, rhs)?
        } else {
            return unsupported!("compound-assign operator {op:?}");
        };
        let coerced = self.coerce(result, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val::new(coerced, local.repr))
    }

    /// `++x` / `x++` / `--x` / `x--` on a numeric local.
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
        Ok(Val::new(produced, local.repr))
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
        self.lower_block(module, body)?;
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

/// Require an assignment/increment target to be a bare identifier.
fn ident_target(target: &HirExpr) -> FrontResult<String> {
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
