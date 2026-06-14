//! Whole-heap-value shape predicates — split out of [`super::call`] (the <500-line
//! module rule). These decide whether a console.log / dispatch argument is a WHOLE
//! object/array value (vs a scalar pulled from one), which routes it to the inspect
//! trampolines or BAILS (object inspect near-misses).

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use super::lower::{HeapShape, Lowerer};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
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
