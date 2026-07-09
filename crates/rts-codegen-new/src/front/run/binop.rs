//! Binary-operator lowering for the whole-program path.
//!
//! Split out of [`super::expr`] (the 500-line module rule). Holds the
//! native-vs-generic decision for every binary operator + the helpers the
//! decision needs:
//!
//! - **Equality `==`/`===`** — swc conflates the two onto one HIR op, so we lower
//!   it only when the operand KINDS prove `==`/`===` agree (same proven kind);
//!   cross/unknown kind BAILS. Same-kind Tagged → the runtime `strict_eq`;
//!   same-kind native → the native compare.
//! - **Relational `< <= > >=`** — native when both proven numeric; the generic
//!   `__rtsadp_{lt,le,gt,ge}` PolyValue path when any operand is Tagged.
//! - **Arithmetic `+ - * / % **`** — native fast path UNCHANGED for proven
//!   numeric operands; any Tagged/string/mixed operand routes to the matching
//!   `__rtsadp_*` (`+` is the one generic concat/add path). `%` on proven floats
//!   and `**` (no native op) route generic for correctness.
//! - **Bitwise/shifts `& | ^ << >> >>>`** — ALWAYS generic: JS bitwise semantics
//!   (ToInt32/ToUint32, 5-bit mask, unsigned `>>>`) are not a native i64 op.
//!
//! Equality / `&&` / `||` / `??` lowering lives in [`super::binop_eq`].

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::{HirBinOp, HirExpr};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The CLASS an operator-overload `lhs <op> rhs` RETURNS, when the lhs's
    /// static class defines the mapped method and that method's return type is a
    /// known class (`Vec2.add(): Vec2`). Lets `const c = a + b` record `c`'s
    /// class so a chained `c.describe()` / `c == f` dispatches.
    pub(super) fn overload_ret_class(&self, lhs: &HirExpr, op: HirBinOp) -> Option<String> {
        let class = self.static_instance_class(lhs)?;
        let m = overload_method_name(op)?;
        let fn_name = self.classes.get(&class)?.methods.get(m)?.clone();
        self.sigs.get(&fn_name)?.ret_class.clone()
    }

    /// Whether `e`, as a coercing-op operand, is EXACT through the runtime generic
    /// path's ToPrimitive. Safe when: it is not an object; the object has no custom
    /// `toString`/`valueOf` (renders the spec `[object Object]`); it carries a
    /// `[Symbol.toPrimitive]`; or it is an OBJECT-LITERAL instance (inline or a var of
    /// a `__rtsl_lit_*` class) — those materialize methods as OWN slots the runtime
    /// `to_primitive_via_method` reaches via `__rtsadp_obj_get` (the real
    /// OrdinaryToPrimitive chain). A general class instance with a custom
    /// `toString`/`valueOf` stays a conservative bail.
    fn add_operand_to_primitive_safe(&self, e: &HirExpr) -> bool {
        if !self.is_whole_object_value(e) {
            return true;
        }
        // Any INLINE literal reaching here is safe (an unsupported-member literal bails
        // earlier in `lower_object_literal`; a recovered/plain one has own-slot methods).
        if matches!(&e.kind, rts_hir::ir::HirExprKind::Object(_)) {
            return true;
        }
        match self.static_instance_class(e) {
            // A LITERAL class instance (`const o = { valueOf(){…} }`) — own-slot methods.
            Some(class) if class.starts_with("__rtsl_lit_") => true,
            Some(class) => self.classes.get(&class).is_none_or(|d| {
                !d.methods.contains_key("toString") && !d.methods.contains_key("valueOf")
            }),
            None => true,
        }
    }
}

/// The Rust-style operator-overload method name for `op` (`a + b` → `a.add(b)`
/// when the class defines it), or `None` for a non-overloadable op.
pub(super) fn overload_method_name(op: HirBinOp) -> Option<&'static str> {
    Some(match op {
        HirBinOp::Add => "add",
        HirBinOp::Sub => "sub",
        HirBinOp::Mul => "mul",
        HirBinOp::Div => "div",
        HirBinOp::Rem => "rem",
        HirBinOp::Eq | HirBinOp::StrictEq => "eq",
        HirBinOp::Lt => "lt",
        HirBinOp::Le => "le",
        HirBinOp::Gt => "gt",
        HirBinOp::Ge => "ge",
        HirBinOp::BitAnd => "bit_and",
        HirBinOp::BitOr => "bit_or",
        HirBinOp::BitXor => "bit_xor",
        _ => return None,
    })
}

use crate::repr::Repr;
use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    pub(super) fn lower_bin(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        if matches!(op, HirBinOp::LogAnd | HirBinOp::LogOr) {
            return self.lower_logical(module, op, lhs, rhs);
        }

        // `a ?? b` (nullish coalescing): short-circuit — evaluate `a`; if it is
        // `null`/`undefined` evaluate and yield `b`, otherwise yield `a` (and `b`
        // is NEVER evaluated). Mirrors the optional-chain block structure.
        if matches!(op, HirBinOp::NullCoalesce) {
            return self.lower_nullish_coalesce(module, lhs, rhs);
        }

        // `key in obj` — own-property membership. Box both operands and call the
        // shape-aware `__rtsadp_obj_has` trampoline (the same one `engine.obj_has`
        // uses), returning a bool. Covers the common own-property `in`; an
        // inherited-property `in` is a later increment but rare.
        if matches!(op, HirBinOp::In) {
            // PRIVATE names in `in`:
            // - `#field in obj` (the BRAND check — rts-hir marks the real
            //   private-name expr with the `\0pn` prefix): probe the storage
            //   spelling `#field` through the normal obj_has (an instance of
            //   the declaring class carries that slot).
            // - `"#field" in obj` (a plain STRING literal): a private field is
            //   never a string-keyed property — constant `false`.
            if let rts_hir::ir::HirExprKind::Lit(rts_hir::ir::HirLit::Str(s)) = &lhs.kind {
                if let Some(real) = s.strip_prefix("\u{0}pn") {
                    // The BRAND is per-CLASS (Cat's `#name` ≠ Dog's `#name`,
                    // same spelling): inside a class body the check is exactly
                    // "is `rhs` an instance of the DECLARING class" — the
                    // shape-set instanceof. Outside a known class body (should
                    // not parse in JS) fall back to the storage-key probe.
                    if let Some(class) = self.enclosing_class() {
                        return self.user_instanceof(module, rhs, &class);
                    }
                    let k = crate::value::abi_adapter::intern_poly(real);
                    let key_word = self.builder.ins().iconst(types::I64, k.raw() as i64);
                    let obj = self.lower_expr(module, rhs)?;
                    let obj_word = self.box_value(obj);
                    let res = self
                        .call_runtime(module, "__rtsadp_obj_has", &[obj_word, key_word])?
                        .expect("__rtsadp_obj_has returns a bool");
                    return Ok(Val::new(res, Repr::Bool));
                }
                if s.starts_with('#') {
                    self.lower_expr(module, rhs)?; // evaluate for effects
                    let v = self.builder.ins().iconst(types::I64, 0);
                    return Ok(Val::new(v, Repr::Bool));
                }
                // `"m" in inst` with a STATICALLY-known class receiver: decide
                // from the class descriptors (fields + methods + accessors,
                // walking the extends chain) plus the Object.prototype members
                // every object inherits. Compile-time constant — the dynamic
                // shape probe cannot see class METHODS (static dispatch).
                if let Some(class) = self.static_instance_class(rhs) {
                    let mut cur = Some(class);
                    let mut found = false;
                    while let Some(c) = cur {
                        let Some(d) = self.classes.get(&c) else { break };
                        if d.fields.iter().any(|f| f == s)
                            || d.methods.contains_key(s.as_str())
                            || d.accessor(s).is_some()
                        {
                            found = true;
                            break;
                        }
                        cur = d.parent.clone();
                    }
                    let found = found
                        || matches!(
                            s.as_str(),
                            "toString" | "valueOf" | "hasOwnProperty" | "isPrototypeOf"
                                | "propertyIsEnumerable" | "toLocaleString" | "constructor"
                        );
                    self.lower_expr(module, rhs)?; // evaluate for effects
                    let v = self.builder.ins().iconst(types::I64, i64::from(found));
                    return Ok(Val::new(v, Repr::Bool));
                }
            }
            let key = self.lower_expr(module, lhs)?;
            let key_word = self.box_value(key);
            let obj = self.lower_expr(module, rhs)?;
            let obj_word = self.box_value(obj);
            let res = self
                .call_runtime(module, "__rtsadp_obj_has", &[obj_word, key_word])?
                .expect("__rtsadp_obj_has returns a bool");
            return Ok(Val::new(res, Repr::Bool));
        }

        // `x instanceof C` (P5.3). swc collapses `instanceof`/`in`/etc onto
        // `HirBinOp::Unsupported`; we only treat it as instanceof when the RHS is a
        // bare identifier naming a class the engine can check (a user class, or a
        // runtime/Registry class Map/Set/Error-family/Array). That keeps `"k" in o`
        // (rhs not a class ident) safely bailed — never a wrong instanceof.
        if matches!(op, HirBinOp::Unsupported) {
            if let rts_hir::ir::HirExprKind::Ident(class) = &rhs.kind {
                if let Some(val) = self.try_instanceof(module, lhs, class)? {
                    return Ok(val);
                }
            }
            return unsupported!(
                "binary operator (other) — `instanceof`/`in`/unmapped op (rhs is not an \
                 engine-checkable class)"
            );
        }

        // A WHOLE object/array operand needs JS ToPrimitive (`[1]+[2]` → `"12"`,
        // `[]+{}` → `"[object Object]"`, with array `.join(",")` coercion) — a
        // later increment. Bail rather than emit the runtime ToString, which
        // diverges from Bun/Node for these.
        //
        // EXCEPTION (P5.8): a `+` where the OTHER operand is a PROVEN STRING and the
        // heap operand is an ARRAY is pure string concatenation — `"x" + [1,2,3]` is
        // `"x1,2,3"`, well-defined by `String(array)` = `.join(",")`, which
        // `__rtsadp_add`'s string path does exactly (and identically for
        // `${[1,2,3]}` in a template). A whole OBJECT operand is NOT relaxed: an
        // object may override `toString`/`valueOf`/`Symbol.toPrimitive` (the engine
        // would render `[object Object]` and diverge), so it keeps bailing.
        // ToPrimitive (issue #304): a `+` where an operand is a STATICALLY-KNOWN-
        // CLASS object that defines `toString`/`valueOf` coerces that object via its
        // method AT LOWERING TIME (where the class is in scope), then concatenates/
        // adds the resulting primitives — never the default `[object Object]`. Plain
        // objects, arrays, and dynamic-class objects keep the gate below.
        //
        // `string + knownPlainObject` (a KNOWN class WITHOUT `toString`/`valueOf`):
        // also route here — `add_operand_word` lowers the object and the generic
        // `__rtsadp_add` string path renders it as `[object Object]`, exactly the JS
        // `String(obj)` for a class with no `toString`. Gated on a STATICALLY-KNOWN
        // class (so a dynamic object that may define `toString` at runtime still
        // bails below, never silently rendering the wrong `[object Object]`).
        let str_plus_known_obj = matches!(op, HirBinOp::Add)
            && ((is_proven_string_expr(lhs) && self.static_instance_class(rhs).is_some())
                || (is_proven_string_expr(rhs) && self.static_instance_class(lhs).is_some()));
        if matches!(op, HirBinOp::Add)
            && (self.has_object_toprimitive(lhs)
                || self.has_object_toprimitive(rhs)
                || str_plus_known_obj)
        {
            return self.lower_add_with_toprimitive(module, lhs, rhs);
        }

        // RUST-STYLE OPERATOR OVERLOAD: `a + b` → `a.add(b)` at COMPILE TIME when
        // the lhs's statically-known class defines the mapped method (add/sub/mul/
        // div/rem/eq/lt/gt/le/ge). Checked before the object gate — a class that
        // opts in gets the method call; everything else keeps the honest bail.
        if let Some(class) = self.static_instance_class(lhs) {
            if let Some(m) = overload_method_name(op) {
                if self
                    .classes
                    .get(&class)
                    .is_some_and(|d| d.methods.contains_key(m))
                {
                    let call = HirExpr::new(
                        rts_hir::ir::HirExprKind::MethodCall {
                            object: Box::new(lhs.clone()),
                            method: m.to_string(),
                            args: vec![rhs.clone()],
                        },
                        rts_hir::HirType::Unknown,
                    );
                    return self.lower_expr(module, &call);
                }
            }
        }

        // ToPrimitive (issue #1447): a NON-`+` coercing op (`- * / % ** < <= > >=
        // == !=`) where an operand is a STATICALLY-KNOWN-CLASS object that defines
        // `toString`/`valueOf` (and did NOT opt into the operator overload above)
        // coerces that object via its method AT LOWERING TIME, then applies the op
        // on the resulting primitives through the generic `__rtsadp_*` path — never
        // the whole-object bail. `obj * 2` / `obj == 3` / `obj - 5` now match
        // Bun/Node. Plain objects, arrays, and dynamic-class objects keep the gate
        // below (their JIT'd method — if any — is unreachable from the trampoline).
        if Self::op_needs_object_toprimitive(op)
            && (self.has_object_toprimitive(lhs) || self.has_object_toprimitive(rhs))
        {
            return self.lower_binop_object_toprimitive(module, op, lhs, rhs);
        }

        // String-concat `+` with an ARRAY-LITERAL-with-object-elements operand
        // (issue #1499 — `"" + [both, both]`, the desugared `${[both, both]}`): the
        // array's `String()` join renders each element via `String(element)`, so
        // rewrite the array literal's object elements to their `toString()` /
        // `[object Object]` HIR (the SAME rewrite `Array.join` uses) and re-lower.
        // Only when the OTHER operand is a proven string (a real string-concat) and
        // every object element is statically resolvable (else `None` → keep bail).
        if matches!(op, HirBinOp::Add) {
            if is_proven_string_expr(lhs) && self.array_arg_has_object_element(rhs) {
                if let Some(r2) = self.array_literal_with_object_elems_to_primitives(rhs) {
                    return self.lower_bin(module, op, lhs, &r2);
                }
            }
            if is_proven_string_expr(rhs) && self.array_arg_has_object_element(lhs) {
                if let Some(l2) = self.array_literal_with_object_elems_to_primitives(lhs) {
                    return self.lower_bin(module, op, &l2, rhs);
                }
            }
        }

        // STRICT equality `===`/`!==` on a whole object/array operand is pure
        // IDENTITY (reference equality) — NO ToPrimitive, so it is always sound and
        // skips the bail below (which guards the coercing ops). The operands fall
        // through to the `StrictEq`/`StrictNe` arm (the runtime `strict_eq` compares
        // the boxed handle words: same object ⇒ equal). This unblocks the common
        // `a === b` / `a !== b` object-identity / cycle-detection pattern.
        let is_strict_eq = matches!(op, HirBinOp::StrictEq | HirBinOp::StrictNe);
        let obj_operand = self.is_whole_object_value(lhs) || self.is_whole_object_value(rhs);
        if !is_strict_eq && (self.is_whole_heap_value(lhs) || self.is_whole_heap_value(rhs)) {
            // An array OPERAND containing a method-bearing OBJECT element would
            // string-concat each element via the runtime join, rendering the object
            // as the default `[object Object]` (the JIT'd `toString` is unreachable
            // from the trampoline) — a wrong value vs bun's element ToPrimitive. Bail
            // (array-of-object element ToPrimitive is a later increment).
            let array_has_object_element =
                self.array_arg_has_object_element(lhs) || self.array_arg_has_object_element(rhs);
            let array_string_concat = matches!(op, HirBinOp::Add)
                && !obj_operand
                && !array_has_object_element
                && (is_proven_string_expr(lhs) || is_proven_string_expr(rhs));
            // GENERIC-`+` OBJECT path: `Add` where every OBJECT operand is
            // ToPrimitive-safe through the runtime generic `+` — its statically
            // known class (if any) defines neither `toString` nor `valueOf` (a
            // custom one is unreachable from the trampoline → wrong value;
            // `[Symbol.toPrimitive]` IS consulted by the generic path, and a
            // method-free object renders the spec `[object Object]`). Array
            // operands keep the stricter `array_string_concat` rule above.
            let object_generic_add = matches!(op, HirBinOp::Add)
                && !self.is_whole_array_value(lhs)
                && !self.is_whole_array_value(rhs)
                && self.add_operand_to_primitive_safe(lhs)
                && self.add_operand_to_primitive_safe(rhs);
            // GENERIC coercing op with ToPrimitive-SAFE operands: every object
            // operand has no custom toString/valueOf (the runtime generic op's
            // spec rendering is exact) and every ARRAY operand has no OBJECT
            // element (the join renders elements exactly). Covers `[] + []`,
            // `[1] == 1`, `{} - 1`, relationals — the coercion-semantics corpus.
            let coercing_generic = {
                let safe = |e: &HirExpr| {
                    !self.array_arg_has_object_element(e) && self.add_operand_to_primitive_safe(e)
                };
                safe(lhs) && safe(rhs)
            };
            if !array_string_concat && !object_generic_add && !coercing_generic {
                return unsupported!(
                    "binary `{op:?}` on a whole object/array operand (ToPrimitive coercion is a later increment)"
                );
            }
        }

        let l = self.lower_expr(module, lhs)?;
        let r = self.lower_expr(module, rhs)?;

        // Strict equality `===`/`!==`. swc now lowers these to distinct
        // `StrictEq`/`StrictNe` ops, so the engine knows it is strict and can
        // lower soundly for ANY operand kinds (no coercion). Tagged → the runtime
        // `strict_eq`; native (proven-numeric/bool) → the native compare.
        if matches!(op, HirBinOp::StrictEq | HirBinOp::StrictNe) {
            if is_tagged(l) || is_tagged(r) {
                return self.lower_strict_eq(module, op, l, r);
            }
            return self.lower_compare(op, l, r);
        }
        // Loose equality `==`/`!=`. swc now lowers these to DISTINCT `Eq`/`Ne` ops
        // (no longer conflated with `===`/`!==`), so the engine can run the REAL JS
        // Abstract Equality algorithm (`__rtsadp_loose_eq`). The proven-same-kind
        // native path stays the fast `iadd`/`fcmp`-style compare (`0 == ""` etc.
        // need coercion, but two proven numbers don't); everything else routes to
        // the generic loose-eq, which ToPrimitive/ToNumber-coerces per spec.
        if matches!(op, HirBinOp::Eq | HirBinOp::Ne) {
            if !is_tagged(l) && !is_tagged(r) && same_proven_kind(l, r) {
                // Both proven, same kind (number==number, bool==bool): native.
                return self.lower_compare(op, l, r);
            }
            return self.lower_loose_eq(module, op, l, r);
        }
        // Relational `< <= > >=`: native when both proven numeric; else the
        // generic PolyValue path (mixed/string operands compared per JS rules).
        if matches!(
            op,
            HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge
        ) {
            if is_tagged(l) || is_tagged(r) {
                return self.lower_generic_relational(module, op, l, r);
            }
            return self.lower_compare(op, l, r);
        }
        // Arithmetic `+ - * / %` (and `**`): native fast path when both proven
        // numeric; the generic `__rtsadp_*` path when any operand is Tagged.
        if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
            return self.lower_arith(module, op, l, r);
        }
        // Bitwise/shifts `& | ^ << >> >>>`: always the generic PolyValue path.
        if matches!(
            op,
            HirBinOp::BitAnd
                | HirBinOp::BitOr
                | HirBinOp::BitXor
                | HirBinOp::Shl
                | HirBinOp::Shr
                | HirBinOp::UShr
        ) {
            return self.lower_bitwise(module, op, l, r);
        }
        unsupported!("binary operator {op:?}")
    }

    /// Generic relational `< <= > >=` over a tag-dispatched runtime compare →
    /// a `Bool` (i64 0/1). Used when either operand is Tagged.
    fn lower_generic_relational(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::Lt => "__rtsadp_lt",
            HirBinOp::Le => "__rtsadp_le",
            HirBinOp::Gt => "__rtsadp_gt",
            HirBinOp::Ge => "__rtsadp_ge",
            _ => return unsupported!("generic relational op {op:?}"),
        };
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("relational returns a value");
        Ok(self.poly_bool_to_bool(res))
    }

    /// Bitwise/shift ops — always the generic `__rtsadp_*` trampoline (JS bitwise
    /// semantics are ToInt32/ToUint32 + 5-bit shift-count mask, NOT a native i64
    /// op). Result is a JS number (int32, or a double for a large `>>>`).
    pub(super) fn lower_bitwise(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::BitAnd => "__rtsadp_band",
            HirBinOp::BitOr => "__rtsadp_bor",
            HirBinOp::BitXor => "__rtsadp_bxor",
            HirBinOp::Shl => "__rtsadp_shl",
            HirBinOp::Shr => "__rtsadp_shr",
            HirBinOp::UShr => "__rtsadp_ushr",
            _ => return unsupported!("bitwise op {op:?}"),
        };
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("bitwise returns a value");
        Ok(Val {
            v: res,
            repr: Repr::Tagged,
            kind: JsKind::Number,
        })
    }

    /// Reduce a boolean PolyValue word to an i64 0/1 `Bool` carrier by comparing
    /// against the `true` singleton (the shared tail of every generic predicate).
    pub(super) fn poly_bool_to_bool(&mut self, res: Value) -> Val {
        let true_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
        let b = self.builder.ins().icmp(IntCC::Equal, res, true_word);
        let widened = self.builder.ins().uextend(types::I64, b);
        Val::new(widened, Repr::Bool)
    }

    /// Arithmetic `+ - * / % **`. Both-numeric uses the native fast path
    /// (UNCHANGED — the proven-numeric benchmarks must NOT route through the
    /// generic trampolines); any Tagged/string/mixed operand boxes both and calls
    /// the matching generic `__rtsadp_*` (`+` → `__rtsadp_add`, the ONE `+` path).
    /// If `v` is defined by an `iconst`, return its integer value. Used to keep
    /// the fast native-`srem` path only when a divisor is a known non-zero
    /// constant (a zero divisor would trap the CPU; JS wants `NaN`).
    fn const_int_value(&self, v: cranelift_codegen::ir::Value) -> Option<i64> {
        use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef};
        if let ValueDef::Result(inst, _) = self.builder.func.dfg.value_def(v) {
            match self.builder.func.dfg.insts[inst] {
                InstructionData::UnaryImm {
                    opcode: Opcode::Iconst,
                    imm,
                } => return Some(imm.bits()),
                // A negative literal lowers as `ineg(iconst n)` — see through it
                // (`0 * -1`'s signed-zero fold needs the `-1`).
                InstructionData::Unary {
                    opcode: Opcode::Ineg,
                    arg,
                } => return self.const_int_value(arg).map(|n| n.wrapping_neg()),
                _ => {}
            }
        }
        None
    }

    pub(super) fn lower_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        if is_tagged(l) || is_tagged(r) {
            return self.lower_generic_arith(module, op, l, r);
        }

        // A boolean operand ToNumbers to 0/1 (JS: `true + 1 === 2`). Bool is already
        // an i64 0/1 in our repr, so coercing to Int32 is a pure reinterpret — then
        // the normal integer/float arithmetic below applies.
        let l = if matches!(l.repr, Repr::Bool) {
            Val::new(self.coerce(l, Repr::Int32)?, Repr::Int32)
        } else {
            l
        };
        let r = if matches!(r.repr, Repr::Bool) {
            Val::new(self.coerce(r, Repr::Int32)?, Repr::Int32)
        } else {
            r
        };
        let both_int = is_int_repr(l.repr) && is_int_repr(r.repr);
        match op {
            HirBinOp::Div => {
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = self.builder.ins().fdiv(lv, rv);
                Ok(Val::new(v, Repr::Float64))
            }
            // Float `%` is fmod-style (sign of dividend); `**` has no native op.
            // Route both to the generic numeric trampolines (correct, rare).
            HirBinOp::Rem if !both_int => self.lower_generic_arith(module, op, l, r),
            HirBinOp::Exp => self.lower_generic_arith(module, op, l, r),
            // Integer `%`: native `srem` TRAPS on a zero divisor, but JS `x % 0`
            // is `NaN` (a Number). Only stay on the fast int path when the divisor
            // is a compile-time-known non-zero constant (covers `i % 2` etc.);
            // otherwise route to the generic trampoline that yields NaN correctly.
            HirBinOp::Rem if both_int => match self.const_int_value(r.v) {
                Some(d) if d != 0 => {
                    let v = self.builder.ins().srem(l.v, r.v);
                    Ok(Val::new(v, wider_int(l.repr, r.repr)))
                }
                _ => self.lower_generic_arith(module, op, l, r),
            },
            _ if both_int => {
                // JS `0 * negative` is NEGATIVE zero — the int `imul` loses the
                // sign. Both-CONST operands whose product is a signed zero fold
                // to the exact `-0.0` double at compile time (`1 / (0 * -1)` →
                // -Infinity); the variable-operand int fast path is unchanged.
                if matches!(op, HirBinOp::Mul) {
                    if let (Some(a), Some(b)) =
                        (self.const_int_value(l.v), self.const_int_value(r.v))
                    {
                        if a.checked_mul(b) == Some(0) && ((a < 0) != (b < 0)) {
                            let v = self.builder.ins().f64const(-0.0);
                            return Ok(Val::new(v, Repr::Float64));
                        }
                    }
                }
                let v = match op {
                    HirBinOp::Add => self.builder.ins().iadd(l.v, r.v),
                    HirBinOp::Sub => self.builder.ins().isub(l.v, r.v),
                    HirBinOp::Mul => self.builder.ins().imul(l.v, r.v),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val::new(v, wider_int(l.repr, r.repr)))
            }
            _ => {
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = match op {
                    HirBinOp::Add => self.builder.ins().fadd(lv, rv),
                    HirBinOp::Sub => self.builder.ins().fsub(lv, rv),
                    HirBinOp::Mul => self.builder.ins().fmul(lv, rv),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val::new(v, Repr::Float64))
            }
        }
    }

    /// The generic arithmetic path: box both operands to PolyValue and call the
    /// matching `__rtsadp_*` trampoline (the one tag-dispatched arithmetic path).
    fn lower_generic_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::Add => "__rtsadp_add",
            HirBinOp::Sub => "__rtsadp_sub",
            HirBinOp::Mul => "__rtsadp_mul",
            HirBinOp::Div => "__rtsadp_div",
            HirBinOp::Rem => "__rtsadp_mod",
            HirBinOp::Exp => "__rtsadp_pow",
            _ => return unsupported!("generic arithmetic op {op:?}"),
        };
        let (lk, rk) = (l.kind, r.kind);
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("generic arithmetic returns a value");
        // Every arithmetic op EXCEPT `+` produces a JS number unconditionally
        // (only `+` can yield a string via concatenation). Recording the proven
        // `Number` kind lets a following `x%2 === 0` see same-kind operands and
        // lower the strict-eq soundly instead of bailing on Unknown.
        //
        // For `+`: if EITHER operand is a PROVEN string, the result is PROVEN a
        // string — JS `string + x` ToString-concats `x` (and `x + string` too), so
        // the concat result is a string regardless of the other operand. Recording
        // `Str` lets `(v + "\n").length` / string methods on the concat result
        // dispatch instead of bailing "unproven shape". Otherwise (number-add vs
        // concat depends on runtime types) it stays Unknown.
        let kind = if matches!(op, HirBinOp::Add) {
            if matches!(lk, JsKind::Str) || matches!(rk, JsKind::Str) {
                JsKind::Str
            } else {
                JsKind::Unknown
            }
        } else {
            JsKind::Number
        };
        Ok(Val {
            v: res,
            repr: Repr::Tagged,
            kind,
        })
    }
}

// Free helpers shared by the binary-op lowering (and `expr.rs`).

pub(super) fn is_tagged(v: Val) -> bool {
    matches!(v.repr, Repr::Tagged)
}

/// Whether two operands have the SAME statically-proven JS kind — the condition
/// under which `==` and `===` agree (so equality is sound to lower despite the
/// HIR conflating the two operators). `Unknown` kinds never qualify.
fn same_proven_kind(l: Val, r: Val) -> bool {
    l.kind != JsKind::Unknown && l.kind == r.kind
}

pub(super) fn is_int_repr(r: Repr) -> bool {
    matches!(r, Repr::Int32 | Repr::Int64)
}

/// Whether `e` is a STATICALLY-PROVEN string expression (P5.8): a string literal,
/// or a `+` chain whose HIR result type is `Str` (what the template desugar emits —
/// the seed quasi is a string literal, forcing the whole chain to string
/// concatenation). Used to allow `string + array/object` (pure concatenation via
/// `String(...)`), which is well-defined, while still bailing the true ToPrimitive
/// `array + array` case.
pub(super) fn is_proven_string_expr(e: &HirExpr) -> bool {
    use rts_hir::ir::{HirExprKind, HirLit};
    if matches!(e.ty, rts_hir::HirType::Str) {
        return true;
    }
    match &e.kind {
        HirExprKind::Lit(HirLit::Str(_)) => true,
        HirExprKind::Bin {
            op: HirBinOp::Add,
            lhs,
            rhs,
        } => is_proven_string_expr(lhs) || is_proven_string_expr(rhs),
        _ => false,
    }
}

/// Whether `e` is CONSERVATIVELY side-effect-free — safe to evaluate eagerly in
/// the `&&`/`||` bool fast path (a call/assignment/update MUST short-circuit, so it
/// returns `false` for those). Literals, identifiers, member reads, and pure
/// unary/binary combinations of effect-free operands qualify; anything that could
/// run user code or mutate state does not.
pub(super) fn is_effect_free(e: &HirExpr) -> bool {
    use rts_hir::ir::HirExprKind;
    match &e.kind {
        HirExprKind::Lit(_) | HirExprKind::Ident(_) => true,
        HirExprKind::Unary { operand, .. } => is_effect_free(operand),
        HirExprKind::Bin { lhs, rhs, .. } => is_effect_free(lhs) && is_effect_free(rhs),
        HirExprKind::Member { object, .. } => is_effect_free(object),
        _ => false,
    }
}

pub(super) fn wider_int(a: Repr, b: Repr) -> Repr {
    if matches!(a, Repr::Int64) || matches!(b, Repr::Int64) {
        Repr::Int64
    } else {
        Repr::Int32
    }
}

pub(super) fn float_cc(op: HirBinOp) -> FrontResult<FloatCC> {
    Ok(match op {
        HirBinOp::Eq | HirBinOp::StrictEq => FloatCC::Equal,
        HirBinOp::Ne | HirBinOp::StrictNe => FloatCC::NotEqual,
        HirBinOp::Lt => FloatCC::LessThan,
        HirBinOp::Le => FloatCC::LessThanOrEqual,
        HirBinOp::Gt => FloatCC::GreaterThan,
        HirBinOp::Ge => FloatCC::GreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}

pub(super) fn int_cc(op: HirBinOp) -> FrontResult<IntCC> {
    Ok(match op {
        HirBinOp::Eq | HirBinOp::StrictEq => IntCC::Equal,
        HirBinOp::Ne | HirBinOp::StrictNe => IntCC::NotEqual,
        HirBinOp::Lt => IntCC::SignedLessThan,
        HirBinOp::Le => IntCC::SignedLessThanOrEqual,
        HirBinOp::Gt => IntCC::SignedGreaterThan,
        HirBinOp::Ge => IntCC::SignedGreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}
