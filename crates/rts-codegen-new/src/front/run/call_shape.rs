//! Whole-heap-value shape predicates — split out of [`super::call`] (the <500-line
//! module rule). These decide whether a console.log / dispatch argument is a WHOLE
//! object/array value (vs a scalar pulled from one), which routes it to the inspect
//! trampolines or BAILS (object inspect near-misses).

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;

use super::lower::{HeapShape, Lowerer};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `object.length` should dispatch DYNAMICALLY (P5.9): the receiver is
    /// a Tagged value of UNPROVEN heap shape (a param, a call/method-call return),
    /// for which `.length` is read at runtime via `__rtsadp_dyn_length`. A receiver
    /// with a PROVEN shape (an array literal local, an object local) is excluded —
    /// it keeps the faster static `VEC_LEN` / shape path above. A non-Tagged
    /// (proven-numeric/bool) receiver is excluded too (`.length` on a number reads
    /// `undefined` in JS, but the corpus does not need it and the static path bails
    /// it, so we do not perturb that).
    pub(super) fn is_dynamic_length_receiver(&self, object: &HirExpr) -> bool {
        match &object.kind {
            // An identifier bound to a Tagged local WITHOUT a proven heap shape
            // (a param, a re-`let` opaque local). A shaped local / object local is
            // handled by the static path; a numeric/bool local is not Tagged.
            HirExprKind::Ident(name) => {
                self.local(name)
                    .is_some_and(|l| matches!(l.repr, Repr::Tagged))
                    && self.local_shapes.get(name).is_none()
                    && !self.object_locals.contains(name)
            }
            // A USER-FUNCTION call result (`mk().length`) is an opaque value →
            // dispatch its `.length` at runtime. Restricted to a bare-identifier
            // callee: a `Member` callee (`Array.from(..)`/`String.x(..)`) may return
            // an unsupported-source SENTINEL whose `.length` must BAIL, not read
            // `undefined` — so those keep the static path's bail.
            HirExprKind::Call { callee, .. } => matches!(callee.kind, HirExprKind::Ident(_)),
            // A real INSTANCE method-call result (`s.split(",").length`,
            // `a.map(..).length`) is a genuine array/string value → dispatch its
            // `.length` at runtime. A GLOBAL-CLASS STATIC (`Array.from(..)`,
            // `Array.of(..)`, `String.x(..)`, `Object.x(..)`) is EXCLUDED — it may
            // return an unsupported-source sentinel whose `.length` must bail.
            HirExprKind::MethodCall { object, .. } => !self.is_global_class_static_recv(object),
            // A MEMBER / INDEX result (`obj.list.length`, `rows[i].length`) is an
            // opaque Tagged value → read `.length` at runtime. `__rtsadp_dyn_length`
            // is sound for ANY value (string→code-units, array→len, else→undefined),
            // so this never produces a wrong length.
            HirExprKind::Member { .. } | HirExprKind::Index { .. } => true,
            // `new X(..).length` — the fresh instance's `.length` (an Array ctor, a
            // user class with a `length` field) dispatched at runtime.
            HirExprKind::New { .. } => true,
            // A SHORT-CIRCUIT logical result (`(s || "default").length`,
            // `(a && b).length`, `(x ?? y).length`) yields one of its operands — a
            // real value whose `.length` reads dynamically.
            HirExprKind::Bin { op, .. } => matches!(
                op,
                rts_hir::ir::HirBinOp::LogOr
                    | rts_hir::ir::HirBinOp::LogAnd
                    | rts_hir::ir::HirBinOp::NullCoalesce
            ),
            _ => false,
        }
    }

    /// Whether `recv` is a bare GLOBAL-CLASS identifier used as a STATIC receiver
    /// (`Array`/`String`/`Object`/`Number`/`Math`), not shadowed by a local or a
    /// user class. A method call on such a receiver is a static (`Array.from`,
    /// `String.fromCharCode`, …) whose result may be an unsupported-source sentinel
    /// — so its `.length` must keep the static path's bail, not read `undefined`.
    fn is_global_class_static_recv(&self, recv: &HirExpr) -> bool {
        matches!(&recv.kind, HirExprKind::Ident(n)
            if matches!(n.as_str(), "Array" | "String" | "Object" | "Number" | "Math")
                && self.local(n).is_none()
                && self.classes.get(n).is_none())
    }

    /// Whether `e` evaluates to a WHOLE OBJECT value (object literal, or an
    /// identifier bound to a local of proven OBJECT shape). Object inspect needs
    /// runtime key recovery (a later increment) — these BAIL.
    pub(super) fn is_whole_object_value(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Object(_) => true,
            // A class instance (`new C()`) is an OBJECT — its slot-0 global shape-id
            // lets the inspect trampoline render `{ field: value }` (P4.9).
            HirExprKind::New { class, .. } => self.classes.get(class).is_some(),
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Object(_)))
                    || self.object_locals.contains(name)
            }
            _ => false,
        }
    }

    /// Whether `e` evaluates to a WHOLE object OR array value. Used where BOTH
    /// kinds must bail (binary `+`/`==` ToPrimitive, method dispatch on a literal).
    pub(super) fn is_whole_heap_value(&self, e: &HirExpr) -> bool {
        self.is_whole_object_value(e) || self.is_whole_array_value(e)
    }

    /// Whether `e` is an array literal that (transitively) contains an OBJECT
    /// element — which would render as a keyless array (`[ 1 ]` for `{a:1}`), a
    /// near-miss vs bun's `{ a: 1 }`. Such logs BAIL until object inspect lands.
    /// Conservative: only array LITERALS are inspected statically (an array local's
    /// elements are opaque, but they can only become objects via paths that already
    /// bail), so this static walk covers the reachable near-miss.
    pub(super) fn array_arg_has_object_element(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Array(elems) => elems.iter().any(|el| self.is_object_producing(el)),
            _ => false,
        }
    }

    /// Whether `e` (an array element) statically produces an OBJECT value: an
    /// object literal, an array literal that itself contains an object, or an
    /// identifier bound to an object-shaped local.
    fn is_object_producing(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Object(_) => true,
            HirExprKind::Array(_) => self.array_arg_has_object_element(e),
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Object(_)))
            }
            _ => false,
        }
    }

    /// Whether `e` evaluates to a WHOLE ARRAY value (array literal, or an
    /// identifier bound to a local of proven ARRAY shape). These render via
    /// `__rtsadp_inspect` (`[ … ]`). A scalar member/index access is NOT one.
    pub(super) fn is_whole_array_value(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Array(_) => true,
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Array))
            }
            _ => false,
        }
    }
}
