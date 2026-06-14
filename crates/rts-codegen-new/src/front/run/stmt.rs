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
use rts_hir::{HirExpr, HirStmt, HirType};

use crate::repr::Repr;

use crate::front::error::{unsupported, FrontResult, Unsupported};
use crate::front::repr_map::repr_of;

use super::lower::{cl_type, HeapShape, Local, Lowerer, Val};

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
            HirStmt::For { init, cond, update, body } => {
                self.lower_for(module, init.as_deref(), cond.as_ref(), update.as_ref(), body)
            }
            HirStmt::ForOf { binding, binding_ty, iterable, body } => {
                self.lower_for_of(module, binding, binding_ty, iterable, body)
            }
            HirStmt::ForIn { binding, object, body } => {
                self.lower_for_in(module, binding, object, body)
            }
            HirStmt::Break(label) => self.lower_break(label.as_deref()),
            HirStmt::Continue(label) => self.lower_continue(label.as_deref()),
            HirStmt::Block(stmts) => self.lower_block(module, stmts),
            HirStmt::Throw(arg) => self.lower_throw(module, arg),
            HirStmt::Try { body, catch, finally } => {
                self.lower_try(module, body, catch.as_ref(), finally.as_deref())
            }
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

        // Object/array literal initializers: lower specially and RECORD the
        // local's proven heap shape, so later `obj.key` / `arr[i]` / `arr.length`
        // resolve to constant-slot `VEC_GET`/`VEC_SET`. The local rides `Tagged`
        // (an i64 register holding the boxed object/array handle word).
        let shape = match &init.kind {
            HirExprKind::Object(fields) => {
                let (val, shape_id, lit_class) = self.lower_object_literal(module, fields)?;
                self.bind_tagged_local(name, val);
                // P5.15: a method-bearing literal recorded a synthesized literal-class;
                // record it on the local so `obj.method()` static-dispatches and
                // `${obj}`/`obj + 1`/`String(obj)` run the toString/valueOf chain.
                if let Some(class) = lit_class {
                    self.local_classes.insert(name.to_string(), class);
                }
                Some(HeapShape::Object(shape_id))
            }
            HirExprKind::Array(elems) => {
                let val = self.lower_array_literal(module, elems)?;
                self.bind_tagged_local(name, val);
                Some(HeapShape::Array)
            }
            // `let re = /pat/flags`: a regex LITERAL initializer (P5.12). Compile to
            // a RegExp instance word and record the local's static class so
            // `re.test(s)` / `re.source` / `re instanceof RegExp` dispatch.
            HirExprKind::Raw(_) if super::regex::is_regex_literal(init) => {
                let (pattern, flags) =
                    super::regex::regex_literal_parts(init).expect("is_regex_literal proved parts");
                let val = self.lower_regex_literal(module, &pattern, &flags)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes
                    .insert(name.to_string(), super::regex::REGEX_CLASS.to_string());
                return Ok(());
            }
            // `let a = new Array(n)`: the built-in Array constructor → an array
            // local (HeapShape::Array), NOT a class instance.
            HirExprKind::New { class, args } if self.is_builtin_array_ctor(class) => {
                let val = self.lower_new_array(module, args)?;
                self.bind_tagged_local(name, val);
                Some(HeapShape::Array)
            }
            // `let m = new Map()` / `new Set()` / `new Error(..)` / wrapper (P5.3):
            // a RUNTIME/Registry class instance. Record its static class in
            // `global_instance_classes` so `m.method()` / `m instanceof C` dispatch.
            HirExprKind::New { class, args } if self.is_global_class_ctor(class) => {
                let (val, class_name) = self.lower_new_global_class(module, class, args)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes.insert(name.to_string(), class_name);
                // No object FIELD shape (the instance is an opaque runtime handle);
                // return early so the generic `let` tail does not run.
                return Ok(());
            }
            // `let c = new C(args)`: build the instance, record the local's CLASS
            // (for static `c.method()` dispatch) and OBJECT shape (for `c.field`).
            HirExprKind::New { class, args } => {
                let (val, class_name, shape_id) = self.lower_new(module, class, args)?;
                self.bind_tagged_local(name, val);
                self.local_classes.insert(name.to_string(), class_name);
                Some(HeapShape::Object(shape_id))
            }
            _ => None,
        };
        if let Some(shape) = shape {
            self.local_shapes.insert(name.to_string(), shape);
            return Ok(());
        }

        let val = self.lower_expr(module, init)?;

        // An array-VALUED expression (e.g. `let b = a.slice(1, 3)`) produces a
        // TAG_OBJECT array word: bind it as a Tagged local AND record the array
        // shape, so `b.length` / `b[i]` / `b.method(..)` resolve like a literal.
        if matches!(val.kind, super::lower::JsKind::Array) {
            self.bind_tagged_local(name, val);
            self.local_shapes.insert(name.to_string(), HeapShape::Array);
            return Ok(());
        }

        // The local's repr is the numeric annotation when the INITIALIZER ITSELF is
        // unboxed-numeric; a Tagged initializer keeps its `Tagged` repr even under a
        // numeric annotation. This is the soundness seam: `let a: number = null`
        // (or a bare `let a = null`/`= undefined`) evaluates to a Tagged singleton
        // PolyValue word, and forcing it into a `Float64`/`Int` slot would reinterpret
        // the singleton bits as a number (reading back NaN/0 instead of `null`/
        // `undefined`). Only widen to the annotation when the value can actually live
        // there unboxed.
        let annotated = repr_of(ty);
        let repr = if annotated.is_unboxed() && val.repr.is_unboxed() {
            annotated
        } else {
            val.repr
        };

        let coerced = self.coerce(val, repr)?;
        let var = self.builder.declare_var(cl_type(repr));
        self.builder.def_var(var, coerced);
        self.locals.insert(name.to_string(), Local { var, repr });
        // A non-literal initializer leaves no proven shape; if `name` was a shaped
        // local being re-`let`, drop the stale shape/class (its value is now opaque).
        self.local_shapes.remove(name);
        self.local_classes.remove(name);
        self.global_instance_classes.remove(name);
        self.object_locals.remove(name);
        Ok(())
    }

    /// Bind `name` to a fresh `Tagged` local holding `val.v` (used for
    /// object/array literal locals, whose value is a boxed handle word). The
    /// caller records the heap shape separately.
    fn bind_tagged_local(&mut self, name: &str, val: Val) {
        let var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.builder.def_var(var, val.v);
        self.locals.insert(name.to_string(), Local { var, repr: Repr::Tagged });
    }

    /// Plain assignment `x = e`. Only to an existing local; the value coerces to
    /// the local's repr.
    pub(super) fn lower_assign(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // Property/index writes (`obj.k = v`, `arr[i] = v`) on a proven shape.
        match &target.kind {
            HirExprKind::Member { object, prop } => {
                return self.lower_member_assign(module, object, prop, value);
            }
            HirExprKind::Index { object, index } => {
                return self.lower_index_assign(module, object, index, value);
            }
            _ => {}
        }
        let name = ident_target(target)?;
        // Re-`x = {…}` to a DIFFERENT object literal: the local stays a proven
        // keyed OBJECT, but its exact shape may now differ from the old one. Lower
        // the new literal (recording its global shape-id in slot 0), bind it, and
        // mark `name` an `object_local` (dynamic-access) — so `x.k` after the
        // reassignment resolves the key at RUNTIME instead of bailing. If the new
        // literal happens to share the prior shape, the dynamic path still reads
        // correctly; we conservatively use the dynamic path rather than prove it.
        if matches!(&value.kind, HirExprKind::Object(_)) && self.local(&name).is_some() {
            if let HirExprKind::Object(fields) = &value.kind {
                let local = self.local(&name).expect("checked above");
                if matches!(local.repr, Repr::Tagged) {
                    let (val, _shape, lit_class) = self.lower_object_literal(module, fields)?;
                    self.builder.def_var(local.var, val.v);
                    self.local_shapes.remove(&name);
                    self.global_instance_classes.remove(&name);
                    self.object_locals.insert(name.clone());
                    // P5.15: re-assigning to a method-bearing literal records its
                    // literal-class (so a later `obj.method()` dispatches); a plain
                    // literal clears any stale class.
                    match lit_class {
                        Some(class) => {
                            self.local_classes.insert(name.clone(), class);
                        }
                        None => {
                            self.local_classes.remove(&name);
                        }
                    }
                    return Ok(Val::new(val.v, Repr::Tagged));
                }
            }
        }
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("assignment to unbound `{name}`")))?;
        let val = self.lower_expr(module, value)?;
        let coerced = self.coerce(val, local.repr)?;
        self.builder.def_var(local.var, coerced);
        // The local now holds an opaque value: drop any stale proven shape/class so
        // a later `name.key`/`name.method()` does NOT resolve against the old layout.
        self.local_shapes.remove(&name);
        self.local_classes.remove(&name);
        self.global_instance_classes.remove(&name);
        self.object_locals.remove(&name);
        Ok(Val::new(coerced, local.repr))
    }

    /// `++x` / `x++` / `--x` / `x--` on a local.
    ///
    /// For a proven numeric repr (`Int*`/`Float64`) this is a native add/sub.
    /// For a `Tagged` local (e.g. a counter assigned from an `any` value) JS
    /// semantics apply: `ToNumber(old)`, then `± 1`, store back, and produce
    /// the OLD number (postfix) or the NEW number (prefix). The ToNumber and the
    /// arithmetic reuse the same generic `__rtsadp_*` trampolines as `+`/`-`.
    pub(super) fn lower_incdec(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        inc: bool,
        prefix: bool,
    ) -> FrontResult<Val> {
        let name = ident_target(target)?;
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("`++`/`--` on unbound `{name}`")))?;
        let old = self.builder.use_var(local.var);
        if matches!(local.repr, Repr::Tagged) {
            // ToNumber(old): a number PolyValue (boxed int32 or inline double).
            let old_num = self
                .call_runtime(module, "__rtsadp_pos", &[old])?
                .expect("__rtsadp_pos returns a value");
            // Box the literal `1` as a double PolyValue and add/sub generically.
            let one_f64 = self.builder.ins().f64const(1.0);
            let one_word = self.box_value(Val::new(one_f64, Repr::Float64));
            let sym = if inc { "__rtsadp_add" } else { "__rtsadp_sub" };
            let new_num = self
                .call_runtime(module, sym, &[old_num, one_word])?
                .expect("generic arithmetic returns a value");
            self.builder.def_var(local.var, new_num);
            let produced = if prefix { new_num } else { old_num };
            return Ok(Val::new(produced, Repr::Tagged));
        }
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
        // `continue` re-tests (jump to header); `break` exits.
        self.loop_stack.push(super::lower::LoopCtx { exit: exit_block, continue_target: header });
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
