//! Two constructor forms the engine had no model for, both measured on a real
//! minified page bundle (WhatsApp Web) and both a compile-time BAIL until now:
//!
//! 1. `new e(..)` where `e` is not a class NAME in this program — it is a class
//!    VALUE reached through a CAPTURE, a module-global cell, or any other binding
//!    [`Lowerer::lower_ident`] resolves. This is the DOMINANT shape in minified /
//!    transpiled code, where every class ends up behind a one-letter binding.
//!    See [`Lowerer::lower_new_dynamic`].
//! 2. `Error("x")` — an Error constructor CALLED WITHOUT `new`. Per the spec the
//!    native Error constructors behave identically called either way, so this
//!    routes to the SAME `lower_new` path the `new` spelling takes and the two can
//!    never diverge. See [`Lowerer::lower_error_call`].
//!
//! Both live here rather than in `call.rs` / `expr.rs`, which are already over the
//! codegen line ceiling.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use rts_hir::ir::HirExpr;

use crate::front::error::FrontResult;
use crate::repr::Repr;
use crate::value;
use crate::value::emit_marshal;

use super::lower::{Lowerer, Val};

/// The PRIMORDIAL Error constructors — the exact set ES defines as callable with
/// or without `new` (`Error(m)` is specified to do the same thing as
/// `new Error(m)`). Naming them is inside the doctrine: `Error` and its
/// subclasses are PRIMORDIAL, so the engine MAY name them (CLAUDE.md, the
/// PRIMORDIAL-vs-REGISTRY rule, lists exactly these).
///
/// A user's `class MyErr extends Error {}` is deliberately NOT here: an ES2015
/// class constructor throws when called without `new`, and only these eight are
/// the legacy function-style constructors that do not.
const ERROR_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "URIError",
    "EvalError",
    "AggregateError",
];

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `name(args)` is a bare CALL of a primordial Error constructor —
    /// `Error("x")` with no `new`.
    ///
    /// Requires the ambient prelude class to actually be present
    /// (`rts-primitives/src/error.ts` declares the whole family), so the call
    /// routes to a real constructor and never to a synthesized stub. A local, a
    /// user function, or a capture of the same name SHADOWS the primordial,
    /// exactly like the `RegExp` bare-call form next door
    /// (`regex::is_bare_regexp_call`).
    pub(super) fn is_bare_error_call(&self, name: &str) -> bool {
        ERROR_CLASSES.contains(&name)
            && self.classes.get(name).is_some()
            && self.local(name).is_none()
            && !self.captures.contains_key(name)
            && !self.sigs.contains_key(name)
    }

    /// `Error(message[, options])` — the call form. Delegates to `lower_new`, so
    /// the with-`new` and without-`new` spellings share ONE lowering and cannot
    /// drift apart (the ctor arity, the `{ cause }` options bag, and the
    /// `.stack` capture all come from the same `.ts` constructor).
    pub(super) fn lower_error_call(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let (val, _class, _shape) = self.lower_new(module, name, args)?;
        Ok(val)
    }

    /// Whether `new <name>(..)` would hit `lower_new`'s "not a user class" bail —
    /// i.e. the name is neither a class declared in this program nor a static
    /// alias of one (`const C = Box`), and is not the `new Object(x)` form
    /// `lower_new` handles itself.
    ///
    /// Every earlier, more specific `new` route (builtin `Array`, a global /
    /// Registry class, a function-as-constructor, a class-valued LOCAL, the
    /// primordial `Function`) is checked by the caller BEFORE this, so relaxing
    /// the bail cannot steal one of them.
    pub(super) fn new_would_bail(&self, name: &str, argc: usize) -> bool {
        let resolved: &str = self
            .local_class_refs
            .get(name)
            .map(String::as_str)
            .unwrap_or(name);
        if resolved == "Object" && argc <= 1 {
            return false;
        }
        self.classes.get(resolved).is_none()
    }

    /// `new <name>(args)` where `<name>` holds a class VALUE the static path
    /// cannot see: a variable CAPTURED by the enclosing arrow/method, a
    /// module-global cell, a re-exported binding — anything
    /// [`Lowerer::lower_ident`] can read.
    ///
    /// The value is constructed through [`__rtsadp_new_invoke`], the same
    /// registered-new-thunk path a class-valued LOCAL takes: it constructs only
    /// when the value's stored thunk is a registered class NEW-THUNK, and throws a
    /// TypeError otherwise — a non-constructor is never mis-constructed.
    ///
    /// A name that resolves to NOTHING is not a compile error here either:
    /// `lower_ident` emits a runtime `ReferenceError` throw, which is what JS
    /// does and what keeps a UMD bundle's untaken feature-sniff branch compilable.
    pub(super) fn lower_new_dynamic(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let v = self.lower_ident(module, name)?;
        let fn_word = self.box_value(v);
        self.lower_new_value_word(module, fn_word, args)
    }

    /// Lower `new <local>(args)` where `<local>` holds a runtime VALUE (a class
    /// reified into a local / `globalThis` field — `const G = globalThis.Box; new
    /// G(5)`). The value is invoked through [`__rtsadp_new_invoke`]: if its stored
    /// thunk is a registered class NEW-THUNK it constructs (allocates + runs the
    /// ctor + returns the instance); otherwise a TypeError is thrown (the value is
    /// not a constructor — never mis-constructed). The result is an opaque
    /// PolyValue word (kind Unknown), so a `let c = new G()` records no static class.
    pub(super) fn lower_new_value(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let local = self
            .local(name)
            .expect("caller proved `name` is an in-scope local value");
        let fn_word = self.builder.use_var(local.var);
        self.lower_new_value_word(module, fn_word, args)
    }

    /// [`Self::lower_new_value`] over an already-lowered class-VALUE word, so a
    /// class reached through something other than a plain local — a CAPTURE, a
    /// module-global cell (see [`Self::lower_new_dynamic`]) —
    /// constructs through the SAME `__rtsadp_new_invoke` path with the same
    /// not-a-constructor guarantee.
    pub(super) fn lower_new_value_word(
        &mut self,
        module: &mut dyn Module,
        fn_word: Value,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // Box the first four positional args; overflow (5th+) into a rest array.
        let undef = || value::PolyValue::undefined().raw() as i64;
        let mut slots: [Value; 4] = [self.builder.ins().iconst(types::I64, undef()); 4];
        for (i, a) in args.iter().take(4).enumerate() {
            let v = self.lower_expr(module, a)?;
            slots[i] = self.box_value(v);
        }
        let rest = if args.len() > 4 {
            let arr = emit_marshal::emit_new_vec_object(module, self.builder);
            for a in &args[4..] {
                let v = self.lower_expr(module, a)?;
                let word = self.box_value(v);
                emit_marshal::emit_vec_push(module, self.builder, arr, word);
            }
            arr
        } else {
            self.builder.ins().iconst(types::I64, undef())
        };
        let res = self
            .call_runtime(
                module,
                "__rtsadp_new_invoke",
                &[fn_word, slots[0], slots[1], slots[2], slots[3], rest],
            )?
            .expect("__rtsadp_new_invoke returns a value");
        // A non-constructor value left a pending TypeError — unwind it here.
        self.emit_post_call_error_check(module)?;
        Ok(Val::new(res, Repr::Tagged))
    }
}
