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
//! REMAINING Date-shaped residual (the smaller next drain): the `now`/`UTC`/
//! `parse` static arms + the `Date.UTC` calendar-default padding ([`pad_utc_defaults`])
//! and the calendar-ctor `undefined` padding below. These match by METHOD NAME
//! (no `"Date"` literal, so they do not trip the gate); fully removing them needs
//! ctor/static default-arg + overload-arg-type metadata on the spec.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_engine::abi::{AbiType, MemberFlags};
use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `new <RegistryClass>([...])` — pick the REAL registered constructor by
    /// arity / arg type. For `Date`:
    /// - 0 args → current instant (the 0-arg ctor, NON-deterministic);
    /// - 1 numeric arg → epoch ms (the `[I64]` ctor);
    /// - 1 string arg → ISO parse (the `[StrPtr]` ctor);
    /// - 2..=7 args → calendar components, month 0-indexed (the `[F64;7]` ctor,
    ///   padded with `undefined` for the missing tail; TZ-dependent).
    ///
    /// A 1-arg `new C(x)` whose arg is NEITHER a proven number nor a proven
    /// string (an `any`/Tagged value) BAILS: the overload depends on the runtime
    /// type, which we cannot pick statically without guessing — the honesty
    /// floor. A class with no matching registered ctor BAILS too. Returns the
    /// boxed `TAG_OBJECT` instance word.
    pub(super) fn emit_registry_ctor(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        use super::registry;
        match args.len() {
            0 => {
                let Some(call) = registry::class_ctor(class, 0) else {
                    return unsupported!("`new {class}()` — no 0-arg constructor registered");
                };
                let v = self.emit_registry_call(module, &call, None, &[], JsKind::Object)?;
                Ok(v.v)
            }
            1 => {
                let v = self.lower_expr(module, &args[0])?;
                // Pick the 1-arg ctor overload by the arg's PROVEN type: a string
                // arg matches the `[StrPtr]` ctor, a number the `[I64]` ctor. An
                // `any`/Tagged arg cannot be statically picked — BAIL.
                let want = match v.kind {
                    JsKind::Str => AbiType::StrPtr,
                    JsKind::Number => AbiType::I64,
                    _ => {
                        return unsupported!(
                            "`new {class}(x)` with a non-number / non-string argument \
                             (the overload dispatch depends on the runtime type — a \
                             later increment)"
                        );
                    }
                };
                let Some(call) = registry::class_ctors(class)
                    .into_iter()
                    .find(|c| c.arg_abis.first() == Some(&want))
                else {
                    return unsupported!("`new {class}(x)` — no matching 1-arg constructor overload");
                };
                let res = self.emit_registry_call(module, &call, None, &[v], JsKind::Object)?;
                Ok(res.v)
            }
            n @ 2..=7 => {
                // Calendar components: the registered 7-arg `[F64;7] -> Handle`
                // constructor (month 0-indexed); pad the missing tail with
                // `undefined` (ToNumber → NaN → the runtime default day=1/rest=0).
                let Some(call) = registry::class_ctor(class, 7) else {
                    return unsupported!("`new {class}(..)` with {n} args — no matching constructor");
                };
                let undef = value::PolyValue::undefined().raw() as i64;
                let mut vals: Vec<Val> = Vec::with_capacity(7);
                for a in args {
                    vals.push(self.lower_expr(module, a)?);
                }
                for _ in n..7 {
                    let w = self.builder.ins().iconst(types::I64, undef);
                    vals.push(Val::tagged_kind(w, JsKind::Number));
                }
                let res = self.emit_registry_call(module, &call, None, &vals, JsKind::Object)?;
                Ok(res.v)
            }
            n => unsupported!("`new {class}(..)` expects 0..=7 args, got {n}"),
        }
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
        let result_kind = match call.ret {
            AbiType::Handle => JsKind::Str,
            AbiType::Bool => JsKind::Bool,
            _ => JsKind::Number,
        };
        self.emit_registry_call(module, &call, Some(recv), &vals, result_kind)
    }

    /// Try to lower a `C.static(...)` call where `C` is a bare pure-Registry
    /// class global. Returns `Ok(None)` when `object` is not such a global (so
    /// the caller falls through), or an explicit bail for an unknown static.
    ///
    /// The `now`/`UTC`/`parse` arms are Date's registered static surface (the
    /// remaining Date-shaped residual — see the module doc); they are reached
    /// only via the [`Lowerer::is_pure_registry_class`] predicate, never a
    /// `"Date"` literal.
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
        match (method, args.len()) {
            // `Date.now()` — registered as a 0-arg `Function` returning I64 ms.
            ("now", 0) => {
                let Some(call) = registry::class_static(name, "now", 0) else {
                    return unsupported!("`{name}.now()` — no such static");
                };
                let v = self.emit_registry_call(module, &call, None, &[], JsKind::Number)?;
                Ok(Some(Val::new_with_kind(v.v, v.repr, JsKind::Number)))
            }
            // `Date.UTC(year, month, day?, hour?, min?, sec?, ms?)` — the
            // registered static takes 7 `I64`s (ms since epoch). At least
            // year+month is required (JS treats fewer as NaN — a divergent edge
            // we refuse).
            ("UTC", 2..=7) => {
                let Some(call) = registry::class_static(name, "UTC", 7) else {
                    return unsupported!("`{name}.UTC()` — no such static");
                };
                let mut vals = self.lower_call_part_args(module, args)?;
                // Pad the missing tail with the JS defaults (day=1, rest=0) so the
                // `FROM_PARTS` extern (which does not itself default) is correct.
                self.pad_utc_defaults(&mut vals, args.len());
                let v = self.emit_registry_call(module, &call, None, &vals, JsKind::Number)?;
                Ok(Some(Val::new_with_kind(v.v, v.repr, JsKind::Number)))
            }
            ("UTC", _) => unsupported!(
                "`{name}.UTC(..)` with fewer than 2 args (year+month) — the 0/1-arg \
                 NaN edge is a later increment"
            ),
            // `Date.parse(s)` — registered as `(StrPtr) -> F64`. The arg must be a
            // proven string (a non-string would diverge — refuse).
            ("parse", 1) => {
                let v = self.lower_expr(module, &args[0])?;
                if !matches!(v.kind, JsKind::Str) {
                    return unsupported!(
                        "`{name}.parse(x)` with a non-string argument (ToString coercion \
                         is a later increment)"
                    );
                }
                let Some(call) = registry::class_static(name, "parse", 1) else {
                    return unsupported!("`{name}.parse()` — no such static");
                };
                let res = self.emit_registry_call(module, &call, None, &[v], JsKind::Number)?;
                Ok(Some(Val::new_with_kind(res.v, res.repr, JsKind::Number)))
            }
            _ => unsupported!(
                "`{name}.{method}({} args)` — no such static on `{name}`",
                args.len()
            ),
        }
    }

    /// Lower a static's component args to `Val`s. The caller pads the tail (the
    /// runtime `FROM_PARTS` needs all 7).
    fn lower_call_part_args(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Vec<Val>> {
        let mut vals = Vec::with_capacity(7);
        for a in args {
            vals.push(self.lower_expr(module, a)?);
        }
        Ok(vals)
    }

    /// Pad `vals` to 7 with the calendar JS defaults (day=1, hour/min/sec/ms=0).
    /// `present` is how many components the caller supplied (≥2: year, month).
    fn pad_utc_defaults(&mut self, vals: &mut Vec<Val>, present: usize) {
        // index → default for the missing tail: [_, _, day=1, h=0, m=0, s=0, ms=0].
        const DEFAULTS: [i64; 7] = [0, 0, 1, 0, 0, 0, 0];
        for &d in DEFAULTS.iter().take(7).skip(present) {
            let w = self.builder.ins().iconst(types::I64, d);
            vals.push(Val::new(w, crate::repr::Repr::Int64));
        }
    }
}
