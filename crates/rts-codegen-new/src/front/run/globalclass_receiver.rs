//! Static receiver-class inference for the Registry/global-class dispatch path:
//! given a receiver expression, the runtime/Registry class it is statically
//! proven to hold (or `None` — never a guess). Split out of `globalclass.rs`
//! (which kept growing past the file-size floor) since it is one cohesive
//! responsibility: mapping a receiver HIR shape → its class, recursing through
//! chains so `a.b().c()` / `f(x).m()` dispatch without an intermediate local.

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use super::lower::Lowerer;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The recorded runtime/Registry class of a receiver, if statically known:
    /// - `new C(args)` directly (e.g. a chained `new RegExp(..).test(..)`);
    /// - a bare identifier recorded in `global_instance_classes`, or one whose
    ///   top-level `const` was force-promoted to a GCELL (which erases the
    ///   `global_instance_classes` entry) and carries its class in `gcell_classes`;
    /// - a chained method-call / getter / builtin-import-call receiver, resolved
    ///   data-driven by the specs' ts return classes (recursion covers N-deep).
    pub(super) fn global_instance_class(&self, object: &HirExpr) -> Option<String> {
        // A bare regex LITERAL receiver `/pat/.test(s)` (P5.12): a RegExp instance.
        if super::regex::is_regex_literal(object) {
            return Some(super::regex::REGEX_CLASS.to_string());
        }
        match &object.kind {
            HirExprKind::New { class, .. } if self.is_global_class_ctor(class) => {
                Some(class.clone())
            }
            HirExprKind::Ident(name) => self
                .global_instance_classes
                .get(name)
                .cloned()
                // A top-level `const s = createSocket('udp4')` that a FUNCTION
                // captures is force-promoted to a gcell (so every scope reads the one
                // instance), which drops the `global_instance_classes` entry the plain
                // local would carry. `gcell_classes` kept `name → class` from the same
                // spec data — recover it here so `s.on(..)` still dispatches on the
                // Registry class in any scope. Same guards as the user-class path: the
                // name must actually BE a gcell, no same-named local may shadow it,
                // and the class must be a REGISTERED one (a user class recorded there
                // belongs to `static_instance_class`, not this path).
                .or_else(|| {
                    self.gcell_classes
                        .get(name)
                        .filter(|_| self.gcells.contains_key(name) && self.local(name).is_none())
                        .filter(|c| super::registry::has_class(c))
                        .cloned()
                }),
            // A CHAINED registry call receiver, resolved by the specs' ts return
            // classes (data-driven, no class named here):
            // - a STATIC (`Promise.resolve(4)` → its spec ret class);
            // - an INSTANCE method on a receiver whose class this same fn
            //   resolves (`p.then(a).then(b)` — recursion covers N-deep chains).
            HirExprKind::MethodCall {
                object: inner,
                method,
                ..
            } => {
                if let HirExprKind::Ident(cn) = &inner.kind {
                    if self.local(cn).is_none() {
                        if let Some(c) = super::registry::class_statics(cn, method)
                            .iter()
                            .find_map(|c| c.ret_class.clone())
                            .filter(|c| super::registry::has_class(c))
                        {
                            return Some(c);
                        }
                    }
                }
                let recv = self.global_instance_class(inner)?;
                super::registry::class_member_ret_class(&recv, method)
            }
            // A CHAINED registry GETTER receiver (`url.searchParams.get(..)`
            // without the intermediate local): the getter's spec ts-signature
            // names its class (`readonly searchParams: URLSearchParams`) —
            // data-driven, recursion covers N-deep chains.
            HirExprKind::Member {
                object: inner,
                prop,
            } => {
                let recv = self.global_instance_class(inner)?;
                super::registry::class_getter_ret_class(&recv, prop)
            }
            // A BUILTIN-IMPORT FUNCTION-CALL receiver (`createHash(a).update(b)`
            // without an intermediate local): the module member's spec ts return
            // names a registered class (`createHash(): Hash`) — the same data-
            // driven resolution as the `const h = createHash(a)` arm in
            // stmt_let, applied inline so a fluent chain dispatches statically.
            HirExprKind::Call { callee, args } => {
                if let HirExprKind::Ident(fname) = &callee.kind {
                    if let Some((ns, member)) = self.builtins.get(fname).cloned() {
                        return super::registry::namespace_member(&ns, &member, args.len())
                            .and_then(|c| c.ret_class)
                            .filter(|c| super::registry::has_class(c));
                    }
                }
                None
            }
            _ => None,
        }
    }
}
