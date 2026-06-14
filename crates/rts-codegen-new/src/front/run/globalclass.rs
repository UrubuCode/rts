//! `new <RuntimeClass>(args)` + their instance methods + `instanceof` (P5.3).
//!
//! The PRIMORDIAL-vs-Registry doctrine: the engine NAMES only primordial classes
//! directly; a RUNTIME/Registry class (`Map`, `Set`, …) and the wrapper/error
//! primordials (`Error`, `Boolean`, `Number`, `String`) resolve through a
//! data-driven metadata table here — ONE generic path keyed by class NAME, never a
//! per-method switchboard. A constructed instance is a real runtime handle boxed
//! as a `TAG_OBJECT` PolyValue; the local records its static class
//! ([`crate::front::run::lower::Lowerer::global_instance_classes`]) so a later
//! `inst.method(args)` / `inst instanceof C` dispatches at compile time.
//!
//! Each constructor + method references the ACTUAL `__rtsadp_*` trampoline (which
//! wraps the REAL `__RTS_FN_*` runtime symbol — see [`crate::value::mapset`] /
//! the `globals::{error,boolean,number,string}` facade re-exports) with its real
//! ABI; the lowering marshals PolyValue<->ABI through the SAME helpers the rest of
//! the engine uses. Anything not in a metadata row BAILS explicitly — never a
//! guess (the honesty floor).

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::value;

use crate::front::error::{unsupported, FrontResult};

use super::lower::{JsKind, Lowerer, Val};

/// How a runtime/Registry class's CONSTRUCTOR marshals its arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CtorKind {
    /// `new Map()` / `new Set()` — zero args, a fresh collection word
    /// (`__rtsadp_map_new` / `__rtsadp_set_new`). A 1-arg init form
    /// (`new Map([[k,v]])`) is a later increment: BAIL.
    Collection,
    /// `new Error(msg?)` family — one optional string-message arg; the trampoline
    /// is the error-NEW wrapper, the instance a boxed error handle.
    Error,
    /// `new Boolean(x)` / `new Number(x)` / `new String(x)` — one value arg; the
    /// wrapper boxes a primitive (typeof === "object").
    Wrapper,
    /// `new RegExp(pattern[, flags])` — one or two string args; compiles via the
    /// regex compile trampoline (P5.12).
    Regex,
}

/// One runtime/Registry class the engine can construct + dispatch on.
struct ClassMeta {
    /// The codegen-owned constructor trampoline (zero-arg collection / wrapper).
    ctor_symbol: &'static str,
    kind: CtorKind,
    /// The instance-method rows: `(jsName, arity, methodSymbol)`. Each symbol is a
    /// PolyValue-in/out `__rtsadp_*` trampoline; slot 0 is the instance word.
    methods: &'static [(&'static str, usize, &'static str)],
}

/// Resolve a runtime/Registry class NAME to its [`ClassMeta`], or `None` when the
/// engine does not model it (so the caller bails / falls through). Error subtypes
/// share the Error method set but a distinct constructor symbol (each tags the
/// instance with its own `.name`).
fn class_meta(name: &str) -> Option<ClassMeta> {
    let m = match name {
        "Map" => ClassMeta {
            ctor_symbol: "__rtsadp_map_new",
            kind: CtorKind::Collection,
            methods: MAP_METHODS,
        },
        "Set" => ClassMeta {
            ctor_symbol: "__rtsadp_set_new",
            kind: CtorKind::Collection,
            methods: SET_METHODS,
        },
        "Error" => err_meta("__rtsadp_err_new"),
        "TypeError" => err_meta("__rtsadp_err_new_type"),
        "RangeError" => err_meta("__rtsadp_err_new_range"),
        "ReferenceError" => err_meta("__rtsadp_err_new_reference"),
        "SyntaxError" => err_meta("__rtsadp_err_new_syntax"),
        "URIError" => err_meta("__rtsadp_err_new_uri"),
        "EvalError" => err_meta("__rtsadp_err_new_eval"),
        "Boolean" => ClassMeta {
            ctor_symbol: "__rtsadp_w_boolean_new",
            kind: CtorKind::Wrapper,
            methods: &[],
        },
        "Number" => ClassMeta {
            ctor_symbol: "__rtsadp_w_number_new",
            kind: CtorKind::Wrapper,
            methods: &[],
        },
        "String" => ClassMeta {
            ctor_symbol: "__rtsadp_w_string_new",
            kind: CtorKind::Wrapper,
            methods: &[],
        },
        "RegExp" => ClassMeta {
            // The ctor compiles via `__rtsadp_re_compile`, but the args need
            // string-handle marshaling (pattern + optional flags), so the Regex
            // kind drives a dedicated emit path (not the generic ctor_symbol call).
            ctor_symbol: "__rtsadp_re_compile",
            kind: CtorKind::Regex,
            methods: REGEX_METHODS,
        },
        _ => return None,
    };
    Some(m)
}

fn err_meta(ctor_symbol: &'static str) -> ClassMeta {
    ClassMeta { ctor_symbol, kind: CtorKind::Error, methods: ERROR_METHODS }
}

const MAP_METHODS: &[(&str, usize, &str)] = &[
    ("set", 2, "__rtsadp_map_set"),
    ("get", 1, "__rtsadp_map_get"),
    ("has", 1, "__rtsadp_map_has"),
    ("delete", 1, "__rtsadp_map_delete"),
    ("clear", 0, "__rtsadp_map_clear"),
];

const SET_METHODS: &[(&str, usize, &str)] = &[
    ("add", 1, "__rtsadp_set_add"),
    ("has", 1, "__rtsadp_set_has"),
    ("delete", 1, "__rtsadp_set_delete"),
    ("clear", 0, "__rtsadp_set_clear"),
];

const ERROR_METHODS: &[(&str, usize, &str)] = &[("toString", 0, "__rtsadp_err_to_string")];

/// RegExp instance methods (P5.12). `.test(s)` is the high-value one: receiver
/// word + subject word → a bool word, the SAME generic shape as Map/Set methods.
/// `.exec` BAILS (capture-group array extraction is a later increment — not a row
/// here, so it is rejected as "no such method").
const REGEX_METHODS: &[(&str, usize, &str)] = &[("test", 1, "__rtsadp_re_test")];

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `class` names a runtime/Registry class the engine constructs (and
    /// is NOT shadowed by a user class of the same name).
    pub(super) fn is_global_class_ctor(&self, class: &str) -> bool {
        self.classes.get(class).is_none() && class_meta(class).is_some()
    }

    /// Lower `new <RuntimeClass>(args)` to its constructor trampoline, returning the
    /// boxed `TAG_OBJECT` instance word + the class name (so a `let` records the
    /// local's static class for method dispatch / instanceof).
    pub(super) fn lower_new_global_class(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<(Val, String)> {
        let meta = class_meta(class).expect("caller proved a global class");
        let word = match meta.kind {
            CtorKind::Collection => {
                if !args.is_empty() {
                    return unsupported!(
                        "`new {class}(<init>)` with constructor arguments \
                         (init from an iterable is a later increment)"
                    );
                }
                self.call_runtime(module, meta.ctor_symbol, &[])?
                    .expect("collection ctor returns a value")
            }
            CtorKind::Error => self.emit_error_ctor(module, meta.ctor_symbol, class, args)?,
            CtorKind::Regex => self.emit_regex_ctor(module, args)?,
            CtorKind::Wrapper => {
                if args.len() != 1 {
                    return unsupported!(
                        "`new {class}(x)` expects exactly 1 argument, got {}",
                        args.len()
                    );
                }
                let v = self.lower_expr(module, &args[0])?;
                let boxed = self.box_value(v);
                self.call_runtime(module, meta.ctor_symbol, &[boxed])?
                    .expect("wrapper ctor returns a value")
            }
        };
        Ok((Val::tagged_kind(word, JsKind::Object), class.to_string()))
    }

    /// `new Error(msg?)` — the message is an optional string arg. A 0-arg form uses
    /// the empty string; a non-string message BAILS (the runtime ctor takes a
    /// string `(ptr,len)`; coercing an arbitrary value would diverge — refuse).
    /// Returns the boxed `TAG_OBJECT` error instance word.
    fn emit_error_ctor(
        &mut self,
        module: &mut dyn Module,
        symbol: &str,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        let msg_word = match args.len() {
            0 => {
                let pv = value::abi_adapter::intern_poly("");
                self.builder.ins().iconst(types::I64, pv.raw() as i64)
            }
            1 => {
                let v = self.lower_expr(module, &args[0])?;
                if !matches!(v.kind, JsKind::Str) {
                    return unsupported!(
                        "`new {class}(msg)` with a non-string message \
                         (string coercion of the message is a later increment)"
                    );
                }
                self.box_value(v)
            }
            n => {
                return unsupported!("`new {class}(..)` expects 0 or 1 args, got {n}");
            }
        };
        Ok(self
            .call_runtime(module, symbol, &[msg_word])?
            .expect("error ctor returns a value"))
    }

    /// `new RegExp(pattern[, flags])` — both args must be proven strings (a regex
    /// from a non-string pattern is a later increment). Interns nothing extra: the
    /// pattern/flags string PolyValue words go straight to `__rtsadp_re_compile`.
    /// Returns the boxed `TAG_OBJECT` RegExp instance word.
    fn emit_regex_ctor(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        if args.is_empty() || args.len() > 2 {
            return unsupported!("`new RegExp(..)` expects 1 or 2 args, got {}", args.len());
        }
        let pat = self.lower_expr(module, &args[0])?;
        if !matches!(pat.kind, JsKind::Str) {
            return unsupported!(
                "`new RegExp(pattern)` with a non-string pattern (a regex-from-regex \
                 copy / coercion is a later increment)"
            );
        }
        let pat_word = self.box_value(pat);
        let flags_word = if args.len() == 2 {
            let f = self.lower_expr(module, &args[1])?;
            if !matches!(f.kind, JsKind::Str) {
                return unsupported!("`new RegExp(pattern, flags)` with a non-string flags arg");
            }
            self.box_value(f)
        } else {
            let pv = value::abi_adapter::intern_poly("");
            self.builder.ins().iconst(types::I64, pv.raw() as i64)
        };
        Ok(self
            .call_runtime(module, "__rtsadp_re_compile", &[pat_word, flags_word])?
            .expect("regex compile returns a value"))
    }

    /// Try to lower `inst.method(args)` where `inst`'s static class is a recorded
    /// runtime/Registry class (`Map`/`Set`/`Error`/…). Returns `Ok(Some(val))` on a
    /// resolved method, `Ok(None)` when the receiver is not a recorded global-class
    /// instance (caller falls through), or an explicit bail for an unknown method.
    pub(super) fn try_global_class_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some(class) = self.global_instance_class(object) else {
            return Ok(None);
        };
        let meta = class_meta(&class).expect("recorded global class must resolve");
        // `.size` is a PROPERTY in JS, not a method — but the corpus reaches it as
        // both `m.size` (member) and is routed here only for calls. The member form
        // is handled in `lower_member`. A method named `size` does not exist.
        let Some(&(_, arity, symbol)) =
            meta.methods.iter().find(|(n, a, _)| *n == method && *a == args.len())
        else {
            return Err(crate::front::error::Unsupported::new(format!(
                "`{class}.{method}({} args)` — no such method on runtime class `{class}`",
                args.len()
            )));
        };
        debug_assert_eq!(arity, args.len());

        // A Map KEY / Set ELEMENT (the first arg of set/get/has/delete/add) that is
        // a whole OBJECT/ARRAY value cannot be marshaled to the runtime's string-key
        // / stable-key ABI soundly (it would key on `[object Object]` or a handle id,
        // diverging from JS SameValueZero) — BAIL (the honesty floor). Methods whose
        // first arg is a key/element: every Map/Set method here except `clear`.
        let key_is_first = matches!(class.as_str(), "Map" | "Set") && !args.is_empty();
        if key_is_first && self.is_whole_heap_value(&args[0]) {
            return Err(crate::front::error::Unsupported::new(format!(
                "`{class}.{method}()` with a non-primitive (object/array) key/element \
                 (object-keyed collections are a later increment)"
            )));
        }

        // Receiver word (slot 0) — the instance is a Tagged local; use its raw word.
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len() + 1);
        call_args.push(recv_word);
        for a in args {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.box_value(v));
        }
        let res = self
            .call_runtime(module, symbol, &call_args)?
            .expect("global-class method returns a value");
        // Every trampoline returns a PolyValue word. The static kind is generally
        // Unknown (a get could be anything); `.set`/`.add` return the SAME instance
        // (kind Object), `.has`/`.delete` a bool — but Unknown is always sound here.
        Ok(Some(Val::new(res, crate::repr::Repr::Tagged)))
    }

    /// `inst.size` — the `.size` PROPERTY of a recorded Map/Set instance. Returns
    /// `Ok(Some(val))` when `object` is such an instance and `prop == "size"`; else
    /// `Ok(None)` (caller falls through to its normal member handling).
    pub(super) fn try_global_class_member(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Option<Val>> {
        let Some(class) = self.global_instance_class(object) else {
            return Ok(None);
        };
        let symbol = match (class.as_str(), prop) {
            ("Map", "size") => "__rtsadp_map_size",
            ("Set", "size") => "__rtsadp_set_size",
            // `e.message` / `e.name` / `e.stack` on a runtime Error instance.
            ("Map" | "Set", _) => {
                return Err(crate::front::error::Unsupported::new(format!(
                    "`{class}.{prop}` — only `.size` is a property on a runtime {class}"
                )))
            }
            ("RegExp", "source") => "__rtsadp_re_source",
            ("RegExp", "flags") => "__rtsadp_re_flags",
            ("RegExp", "global") => "__rtsadp_re_global",
            ("RegExp", "ignoreCase") => "__rtsadp_re_ignore_case",
            ("RegExp", "multiline") => "__rtsadp_re_multiline",
            ("RegExp", "lastIndex") => "__rtsadp_re_last_index",
            ("RegExp", other) => {
                return Err(crate::front::error::Unsupported::new(format!(
                    "`RegExp.{other}` — only source/flags/global/ignoreCase/multiline/\
                     lastIndex are properties on a runtime RegExp"
                )))
            }
            _ if is_error_class(&class) => match prop {
                "message" => "__rtsadp_err_message",
                "name" => "__rtsadp_err_name",
                "stack" => "__rtsadp_err_stack",
                other => {
                    return Err(crate::front::error::Unsupported::new(format!(
                        "`{class}.{other}` — only message/name/stack are properties on a runtime {class}"
                    )))
                }
            },
            _ => return Ok(None),
        };
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let res = self
            .call_runtime(module, symbol, &[recv_word])?
            .expect("global-class member returns a value");
        // `.size` is a number; `RegExp.global` is a bool; the error props +
        // `RegExp.source`/`.flags` are strings. Tag accordingly so a later
        // `console.log` / coercion formats them correctly.
        let kind = match prop {
            "size" | "lastIndex" => JsKind::Number,
            "global" | "ignoreCase" | "multiline" => JsKind::Bool,
            _ => JsKind::Str,
        };
        Ok(Some(Val::new_with_kind(res, crate::repr::Repr::Tagged, kind)))
    }

    /// The recorded runtime/Registry class of a receiver, if statically known:
    /// - `new C(args)` directly (chained `new Map().set(..)`);
    /// - a bare identifier recorded in `global_instance_classes`.
    pub(super) fn global_instance_class(&self, object: &HirExpr) -> Option<String> {
        // A bare regex LITERAL receiver `/pat/.test(s)` (P5.12): a RegExp instance.
        if super::regex::is_regex_literal(object) {
            return Some(super::regex::REGEX_CLASS.to_string());
        }
        match &object.kind {
            HirExprKind::New { class, .. } if self.is_global_class_ctor(class) => {
                Some(class.clone())
            }
            HirExprKind::Ident(name) => self.global_instance_classes.get(name).cloned(),
            _ => None,
        }
    }

    /// Lower `lhs instanceof <ClassName>` when the engine can decide it via a
    /// runtime class-tag check. Returns `Ok(Some(val))` (a bool) when handled, or
    /// `Ok(None)` when the right side is not an engine-checkable class (caller bails
    /// — never a guess). The check is a real runtime trampoline that inspects the
    /// instance's `Entry` kind / error name, so it is correct for ANY operand.
    pub(super) fn try_instanceof(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        class: &str,
    ) -> FrontResult<Option<Val>> {
        // A user class instanceof: a compile-time class-name compare (the local's
        // recorded class equals `class` or a descendant). Only resolvable when the
        // lhs has a statically-known class.
        if self.classes.get(class).is_some() {
            return self.user_instanceof(lhs, class).map(Some);
        }
        let symbol = match class {
            "Map" => "__rtsadp_is_map",
            "Set" => "__rtsadp_is_set",
            "Error" => "__rtsadp_is_error",
            "Array" => "__rtsadp_arr_is_array",
            // RegExp instanceof: no dedicated runtime tag trampoline yet, so only a
            // STATICALLY-recorded RegExp local/literal can be decided here (compile-
            // time class-name compare). A dynamic operand BAILS (never a guess).
            "RegExp" => {
                if self.global_instance_class(lhs).as_deref() == Some("RegExp") {
                    let word = value::PolyValue::bool(true).raw() as i64;
                    let v = self.builder.ins().iconst(types::I64, word);
                    return Ok(Some(Val::tagged_kind(v, JsKind::Bool)));
                }
                return Ok(None);
            }
            _ if is_error_class(class) => "__rtsadp_is_error",
            _ => return Ok(None),
        };
        let v = self.lower_expr(module, lhs)?;
        let boxed = self.box_value(v);
        let res = self
            .call_runtime(module, symbol, &[boxed])?
            .expect("instanceof trampoline returns a value");
        Ok(Some(Val::tagged_kind(res, JsKind::Bool)))
    }

    /// `lhs instanceof <UserClass>` — a compile-time class-name match. The lhs must
    /// have a statically-known class (`new C()` / a recorded local); the result is
    /// `true` iff that class IS `class` or a descendant of it. An unprovable lhs
    /// BAILS (we never guess instanceof for an opaque value against a user class).
    fn user_instanceof(&mut self, lhs: &HirExpr, class: &str) -> FrontResult<Val> {
        let Some(lhs_class) = self.static_instance_class(lhs) else {
            return unsupported!(
                "`x instanceof {class}` where x has no statically-known class \
                 (dynamic-receiver instanceof on a user class is a later increment)"
            );
        };
        let is = self.class_is_a(&lhs_class, class);
        let word = value::PolyValue::bool(is).raw() as i64;
        let v = self.builder.ins().iconst(types::I64, word);
        Ok(Val::tagged_kind(v, JsKind::Bool))
    }

    /// Whether `derived` IS `base` or transitively extends it (walking `parent`),
    /// over the user ClassTable. A builtin-error virtual parent participates too.
    fn class_is_a(&self, derived: &str, base: &str) -> bool {
        let mut cur = Some(derived.to_string());
        let mut steps = 0;
        while let Some(name) = cur {
            if name == base {
                return true;
            }
            if steps > 64 {
                break;
            }
            steps += 1;
            cur = self.classes.get(&name).and_then(|d| d.parent.clone());
        }
        false
    }
}

/// Whether `name` is one of the runtime Error family the engine models.
fn is_error_class(name: &str) -> bool {
    matches!(
        name,
        "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError" | "URIError"
            | "EvalError"
    )
}
