//! Generic dispatch for a "pure RUNTIME/Registry class" — a class the engine
//! constructs and calls ENTIRELY through the REAL Registry (Pilar 6), with NO
//! codegen `__rtsadp_*` trampoline and NO hand-written per-class method/ctor
//! table. A class qualifies when it is registered (`registry::has_class`) and is
//! NOT shadowed by an ambient `.ts` user class nor listed in the small legacy
//! `class_meta` table (see [`Lowerer::is_pure_registry_class`]).
//!
//! Every form here resolves its real `__RTS_FN_*` symbol + `AbiType` signature
//! from [`super::registry`] and lowers through the ONE generic marshal
//! ([`super::registry_call`]):
//!
//! - [`Lowerer::emit_registry_ctor`] — `new C(...)`; the ctor overload is picked
//!   from the registered constructors by arity / proven arg type;
//! - [`Lowerer::try_registry_instance_method`] — `inst.method(args)`, resolved
//!   via the Registry's instance members;
//! - [`Lowerer::try_registry_static_call`] — `C.static(...)`, resolved via the
//!   Registry's static members.
//!
//! TODAY the only pure-Registry class reaching these paths is `Date` (`RegExp`
//! is in `class_meta`; `Map`/`Set`/`URL`/… have ambient `.ts` classes). Routing
//! is now DATA-DRIVEN — no `"Date"` literal in the dispatch control flow.
//!
//! The unsound-to-lower method set (Date's timezone-divergent formatters + `setX`
//! mutators) is now SPEC DATA: the `rts-shared` Date class stamps those members
//! `MemberFlags::UNSOUND` and [`Lowerer::try_registry_instance_method`] bails on
//! the flag generically — no per-class predicate in the engine.
//!
//! CONSTRUCTORS ([`Lowerer::emit_registry_ctor`]) and STATIC calls
//! ([`Lowerer::try_registry_static_call`]) are now FULLY GENERIC: resolved from
//! the registered overloads, the `[required, total]` arity window + the
//! omitted-tail defaults come from the spec's `Sig::default_args` (`Date.UTC`'s
//! day=1/rest=0; the calendar ctor's `undefined` tail). A same-arity overload
//! ambiguity (the 1-arg `new Date(ms)` `[I64]` vs `new Date(iso)` `[StrPtr]`) is
//! broken by the provided arg's PROVEN `JsKind` matching the param `AbiType` —
//! still DATA from the registered ctors, no method-name / arity literal.
//!
//! The engine now encodes ZERO Date-specific knowledge: every Date semantic
//! (overloads, defaults, unsound methods) is SPEC DATA in the `rts-shared` Date
//! class (`Sig::default_args` + `MemberFlags::UNSOUND` + the registered members).

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_engine::abi::{AbiType, DefaultArg, MemberFlags};
use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `new <RegistryClass>([...])` — FULLY GENERIC, mirroring
    /// [`Self::try_registry_static_call`]. Resolve the matching constructor from
    /// the registered overloads: keep the ctors whose `[required, total]` arity
    /// window (derived from `Sig::default_args`) contains `argc`; when more than
    /// one matches the same arity (the `new Date(ms)` `[I64]` vs `new Date(iso)`
    /// `[StrPtr]` case) break the tie by the provided args' PROVEN `JsKind`
    /// matching the param `AbiType`. Pad the omitted tail with the chosen ctor's
    /// spec defaults (`Int`/`Float`/`Undefined`).
    ///
    /// A 1-arg `new C(x)` whose arg is NEITHER a proven number nor a proven
    /// string (an `any`/Tagged value) BAILS: the overload depends on the runtime
    /// type, which we cannot pick statically without guessing — the honesty
    /// floor. A class / arity with no matching registered ctor BAILS too. Returns
    /// the boxed `TAG_OBJECT` instance word.
    pub(super) fn emit_registry_ctor(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        use super::registry;
        let argc = args.len();
        // Candidate ctors = those whose [required, total] arity window admits argc.
        let candidates: Vec<_> = registry::class_ctors(class)
            .into_iter()
            .filter(|c| {
                let total = c.arg_abis.len();
                (required_args(&c.default_args, total)..=total).contains(&argc)
            })
            .collect();
        if candidates.is_empty() {
            return unsupported!("`new {class}({argc} args)` — no matching constructor");
        }
        // Lower the provided args ONCE (needed to disambiguate same-arity
        // overloads by proven kind, and reused for the marshal).
        let mut vals: Vec<Val> = Vec::with_capacity(argc);
        for a in args {
            vals.push(self.lower_expr(module, a)?);
        }
        // Pick the overload: a single candidate wins outright; a tie is broken by
        // every provided arg's proven `JsKind` matching its param `AbiType`. None
        // matching (an `any`/Tagged arg) BAILS — no runtime-type guessing.
        let call = if candidates.len() == 1 {
            candidates.into_iter().next().expect("len==1")
        } else {
            let Some(call) = candidates.into_iter().find(|c| {
                vals.iter()
                    .enumerate()
                    .all(|(i, v)| abi_accepts_kind(c.arg_abis[i], v.kind))
            }) else {
                return unsupported!(
                    "`new {class}(x)` with a non-number / non-string argument \
                     (the overload dispatch depends on the runtime type — a later \
                     increment)"
                );
            };
            call
        };
        // A `StrPtr` param fed a non-proven-string BAILS (the marshal would
        // ToString-coerce — a behavior change we refuse; same rule as the statics).
        for (i, v) in vals.iter().enumerate() {
            if matches!(call.arg_abis[i], AbiType::StrPtr) && !matches!(v.kind, JsKind::Str) {
                return unsupported!(
                    "`new {class}(..)` — argument {i} is not a proven string"
                );
            }
        }
        // Pad the omitted tail with the chosen ctor's spec defaults.
        for i in argc..call.arg_abis.len() {
            vals.push(self.default_arg_val(call.default_args.get(i), class, i)?);
        }
        let res = self.emit_registry_call(module, &call, None, &vals, JsKind::Object)?;
        Ok(res.v)
    }

    /// Materialise one spec [`DefaultArg`] as the padding [`Val`] for an omitted
    /// trailing arg. `Int`→`iconst I64`, `Float`→`f64const`, `Undefined`→the
    /// `undefined` sentinel word (the runtime extern reads its NaN form as its own
    /// default). A `Required` / missing entry is a spec bug — BAIL, never
    /// mis-marshal.
    fn default_arg_val(
        &mut self,
        d: Option<&DefaultArg>,
        class: &str,
        i: usize,
    ) -> FrontResult<Val> {
        Ok(match d {
            Some(DefaultArg::Int(n)) => {
                let w = self.builder.ins().iconst(types::I64, *n);
                Val::new(w, crate::repr::Repr::Int64)
            }
            Some(DefaultArg::Float(f)) => {
                let w = self.builder.ins().f64const(*f);
                Val::new(w, crate::repr::Repr::Float64)
            }
            Some(DefaultArg::Undefined) => {
                let undef = value::PolyValue::undefined().raw() as i64;
                let w = self.builder.ins().iconst(types::I64, undef);
                Val::tagged_kind(w, JsKind::Number)
            }
            _ => {
                return unsupported!("`{class}(..)` — argument {i} has no spec default");
            }
        })
    }

    /// Try to lower a `inst.method(args)` through the REAL Registry (Pilar 6).
    /// Returns `Ok(Some(val))` on a resolved method, or an explicit bail for an
    /// unknown / unsound method. The result kind follows the Registry return
    /// type: a `Handle` return is a string method (`toISOString`/…) → `Str`; a
    /// `Bool` return → `Bool`; otherwise a numeric getter → `Number`.
    pub(super) fn try_registry_instance_method(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        use super::registry;
        let Some(call) = registry::class_member(class, method, args.len()) else {
            return unsupported!(
                "`{class}.{method}({} args)` — no such method on runtime class `{class}`",
                args.len()
            );
        };
        // Some methods resolve in the Registry but are NOT sound to lower (the
        // honesty floor). The spec marks them `MemberFlags::UNSOUND` — data, not a
        // hardcoded per-class predicate. For `Date` these are the timezone-divergent
        // formatters + the `setX` mutators. BAIL instead of emitting a wrong value.
        if call.flags.contains(MemberFlags::UNSOUND) {
            return unsupported!(
                "`{class}.{method}()` — unsound-to-lower method (timezone-divergent / \
                 mutating; only the deterministic surface is modeled — a later \
                 increment)"
            );
        }
        let recv = self.lower_expr(module, object)?;
        let mut vals: Vec<Val> = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.lower_expr(module, a)?);
        }
        // A `Handle` return is a STRING handle only when the spec's ts-signature
        // says so (`ret_is_string_handle`, e.g. Date.toISOString → string);
        // otherwise it is an OBJECT handle (e.g. `WeakRef.deref(): object`). Using
        // the data-driven flag instead of "Handle ⇒ Str" keeps Date strings as Str
        // while letting object-returning methods box as TAG_OBJECT.
        let result_kind = match call.ret {
            AbiType::Handle if call.ret_is_string_handle => JsKind::Str,
            AbiType::Handle => JsKind::Object,
            AbiType::Bool => JsKind::Bool,
            _ => JsKind::Number,
        };
        self.emit_registry_call(module, &call, Some(recv), &vals, result_kind)
    }

    /// The runtime CLASS a registry instance-method/getter `object.method(args)`
    /// RETURNS, when the spec annotates a named registered class
    /// (`ArrayBuffer.slice(): ArrayBuffer`, `URL.searchParams: URLSearchParams`).
    /// Lets `const a = buf.slice(..)` record `a`'s class (in `global_instance_classes`)
    /// so a chained `a.byteLength` dispatches. `None` when the receiver's class is
    /// not statically a registry class, the method is unknown, or its return is not
    /// a named registered class — so nothing is invented.
    pub(super) fn registry_method_ret_class(
        &self,
        object: &HirExpr,
        method: &str,
        argc: usize,
    ) -> Option<String> {
        let recv_class = self.global_instance_class(object)?;
        let call = super::registry::class_member(&recv_class, method, argc)?;
        let cls = call.ret_class?;
        // Only trust a return-type name that is itself a registered class.
        super::registry::has_class(&cls).then_some(cls)
    }

    /// Try to lower a `C.static(...)` call where `C` is a bare pure-Registry
    /// class global. Returns `Ok(None)` when `object` is not such a global (so
    /// the caller falls through), or an explicit bail for an unknown static /
    /// out-of-range arity / a StrPtr param fed a non-string.
    ///
    /// FULLY GENERIC (no method-name match, no `"Date"` literal): resolve the
    /// static by name ([`registry::class_static_any`]), validate the caller's
    /// argc against the member's `[required, total]` window (derived from the
    /// spec's `default_args`), pad the omitted trailing args with the
    /// spec-declared defaults, and marshal. Today the only pure-Registry statics
    /// are `Date.now`/`UTC`/`parse`; their Date-shaped padding/required rules are
    /// now SPEC DATA (the `Date.UTC` member's `Sig::with_defaults`), not codegen.
    pub(super) fn try_registry_static_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if self.local(name).is_some() || !self.is_pure_registry_class(name) {
            return Ok(None);
        }
        use super::registry;
        let argc = args.len();
        // ALL overloads of `name.method`, then pick the one whose [required, total]
        // arity window admits `argc` (e.g. `URL.canParse(href)` vs `(href, base)`).
        // `class_static_any` returned only the FIRST, wrongly rejecting a 2-arg call
        // to a name whose 1-arg overload was declared first.
        let overloads = registry::class_statics(name, method);
        if overloads.is_empty() {
            return unsupported!("`{name}.{method}()` — no such static on `{name}`");
        }
        let Some(call) = overloads.into_iter().find(|c| {
            let total = c.arg_abis.len();
            let required = required_args(&c.default_args, total);
            argc >= required && argc <= total
        }) else {
            return unsupported!(
                "`{name}.{method}({argc} args)` — no overload accepts {argc} arg(s)"
            );
        };
        let total = call.arg_abis.len();
        // Lower the provided args. A `StrPtr` param fed a non-proven-string BAILS
        // (the generic marshal would ToString-coerce instead — a behavior change
        // we refuse; preserves the old `Date.parse(non-string)` bail).
        let mut vals: Vec<Val> = Vec::with_capacity(total);
        for (i, a) in args.iter().enumerate() {
            let v = self.lower_expr(module, a)?;
            if matches!(call.arg_abis[i], AbiType::StrPtr) && !matches!(v.kind, JsKind::Str) {
                return unsupported!(
                    "`{name}.{method}(..)` — argument {i} is not a proven string \
                     (ToString coercion is a later increment)"
                );
            }
            vals.push(v);
        }
        // Pad the omitted trailing args with the spec-declared defaults. A tail
        // slot is always a default here (argc ≥ required), so a `Required` /
        // missing entry would be a spec bug — BAIL rather than mis-marshal.
        for i in argc..total {
            vals.push(self.default_arg_val(call.default_args.get(i), name, i)?);
        }
        let result_kind = match call.ret {
            AbiType::Handle => JsKind::Str,
            AbiType::Bool => JsKind::Bool,
            _ => JsKind::Number,
        };
        let v = self.emit_registry_call(module, &call, None, &vals, result_kind)?;
        Ok(Some(v))
    }
}

/// The minimum explicit arg count a member accepts: the leading run of
/// `DefaultArg::Required`. An empty `default_args` slice means every declared
/// param is required (`required == total`). The `[required, total]` window is the
/// arity a ctor/static admits — the spec-data replacement for hardcoded arity
/// literals.
fn required_args(default_args: &[DefaultArg], total: usize) -> usize {
    if default_args.is_empty() {
        total
    } else {
        default_args
            .iter()
            .take_while(|d| matches!(d, DefaultArg::Required))
            .count()
    }
}

/// Whether a provided arg of proven `JsKind` may fill a param of `AbiType` — the
/// rule that breaks a same-arity ctor overload tie (`[I64]` vs `[StrPtr]`) by the
/// PROVEN type, never by guessing. A lost/`any` kind (`JsKind::Any`/`Unknown`)
/// matches nothing, so an untyped arg bails rather than picking an overload.
fn abi_accepts_kind(abi: AbiType, kind: JsKind) -> bool {
    match abi {
        AbiType::StrPtr => matches!(kind, JsKind::Str),
        AbiType::Handle => matches!(kind, JsKind::Str | JsKind::Object),
        AbiType::F64 | AbiType::I64 | AbiType::U64 | AbiType::I32 => matches!(kind, JsKind::Number),
        AbiType::Bool => matches!(kind, JsKind::Bool),
        // A raw PolyValue param carries any value verbatim — accepts any kind.
        AbiType::PolyValue => true,
        AbiType::Void => false,
    }
}
