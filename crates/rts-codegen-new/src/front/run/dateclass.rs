//! `Date` constructors, statics, and instance methods — resolved through the REAL
//! Registry (Pilar 6, P5.16). NO codegen `__rtsadp_date_*` trampoline, NO
//! hand-written method/ctor table: `Date` is a RUNTIME/Registry class
//! (PRIMORDIAL-vs-Registry doctrine), so every form here resolves its real
//! `__RTS_FN_GL_DATE_*` / `__RTS_FN_NS_DATE_*` symbol + `AbiType` signature from
//! [`super::registry`] and lowers through the ONE generic marshal
//! ([`super::registry_call`]):
//!
//! - [`Lowerer::emit_date_ctor`] — `new Date(...)`; the 0/1-ms/1-iso/7-field ctor
//!   overloads come straight from the registered `Date` constructors;
//! - [`Lowerer::try_date_method`] — `d.getTime()` / `d.getUTCFullYear()` /
//!   `d.toISOString()` / …, resolved via the Registry's instance methods;
//! - [`Lowerer::try_date_static_call`] — `Date.now()` / `Date.UTC(...)` /
//!   `Date.parse(s)`, resolved via the Registry's static members.
//!
//! Determinism: ms is stored in UTC, so the UTC/epoch ctors + statics + getters
//! match bun. The timezone-divergent formatters (`toString`/`toDateString`/…) and
//! the `setX` mutators resolve in the Registry but BAIL here (the honesty floor —
//! see [`is_divergent_date_method`]).

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_engine::abi::AbiType;
use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::value;

use crate::front::error::{unsupported, FrontResult};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `new Date([...])` — pick the REAL registered constructor by arity / arg type:
    /// - 0 args → current instant (`__RTS_FN_GL_DATE_NEW_NOW`, NON-deterministic);
    /// - 1 numeric arg → epoch ms (the `[I64]` ctor);
    /// - 1 string arg → ISO parse (the `[StrPtr]` ctor);
    /// - 2..=7 args → calendar components, month 0-indexed (the `[F64;7]` ctor,
    ///   padded with `undefined` for the missing tail; TZ-dependent).
    ///
    /// A 1-arg `new Date(x)` whose arg is NEITHER a proven number nor a proven
    /// string (an `any`/Tagged value) BAILS: JS dispatches on the runtime type
    /// (ms vs ISO vs another Date), which we cannot pick statically without
    /// guessing — the honesty floor. Returns the boxed `TAG_OBJECT` Date word.
    pub(super) fn emit_date_ctor(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        use super::registry;
        match args.len() {
            0 => {
                let call = registry::class_ctor("Date", 0)
                    .expect("Date 0-arg ctor is registered");
                let v = self.emit_registry_call(module, &call, None, &[], JsKind::Object)?;
                Ok(v.v)
            }
            1 => {
                let v = self.lower_expr(module, &args[0])?;
                // Pick the 1-arg ctor overload by the arg's PROVEN type: a string
                // arg matches the `[StrPtr]` (ISO) ctor, a number the `[I64]` (ms)
                // ctor. An `any`/Tagged arg cannot be statically picked — BAIL.
                let want = match v.kind {
                    JsKind::Str => AbiType::StrPtr,
                    JsKind::Number => AbiType::I64,
                    _ => {
                        return unsupported!(
                            "`new Date(x)` with a non-number / non-string argument \
                             (the ms-vs-ISO-vs-Date dispatch depends on the runtime \
                             type — a later increment)"
                        )
                    }
                };
                let call = registry::class_ctors("Date")
                    .into_iter()
                    .find(|c| c.arg_abis.first() == Some(&want))
                    .expect("Date 1-arg ctor overload is registered");
                let res = self.emit_registry_call(module, &call, None, &[v], JsKind::Object)?;
                Ok(res.v)
            }
            n @ 2..=7 => {
                // Calendar components: the registered 7-arg `[F64;7] -> Handle`
                // constructor (month 0-indexed); pad the missing tail with
                // `undefined` (ToNumber → NaN → the runtime default day=1/rest=0).
                let call = registry::class_ctor("Date", 7)
                    .expect("Date 7-field ctor is registered");
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
            n => unsupported!("`new Date(..)` expects 0..=7 args, got {n}"),
        }
    }

    /// Try to lower a `dateInstance.method(args)` through the REAL Registry (Pilar
    /// 6). Returns `Ok(Some(val))` on a resolved Date method, or an explicit bail
    /// for an unknown method. The result kind follows the Registry return type: a
    /// `Handle` return is a Date string method (`toISOString`/…) → `Str`; an
    /// `I64` return is a numeric getter → `Number`.
    pub(super) fn try_date_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        use super::registry;
        // The Registry exposes Date's full method surface, but two families are NOT
        // sound to lower here and must BAIL (the honesty floor), even though they
        // resolve: (1) the LOCALE/`toString`/`toDateString`/`toUTCString`/
        // `toTimeString` formatters are timezone-divergent from bun (RTS stores UTC
        // and aliases local→UTC, bun renders the machine's local zone); (2) the
        // `setX`/`setTime` MUTATORS — the engine models a Date instance as an
        // immutable boxed handle, so in-place mutation is a later increment.
        if is_divergent_date_method(method) {
            return unsupported!(
                "`Date.{method}()` — timezone-divergent / mutating Date method \
                 (a later increment; only the deterministic UTC/epoch surface is \
                 modeled)"
            );
        }
        let Some(call) = registry::class_member("Date", method, args.len()) else {
            return unsupported!(
                "`Date.{method}({} args)` — no such method on runtime class `Date`",
                args.len()
            );
        };
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

    /// Try to lower a `Date.now()` / `Date.UTC(...)` / `Date.parse(s)` static call.
    /// Returns `Ok(None)` when `object` is not the bare `Date` global (so the
    /// caller falls through), or an explicit bail for an unknown static. `Date` is
    /// shadowed by a user class / local of the same name (then `Ok(None)`).
    pub(super) fn try_date_static_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if name != "Date" || self.local(name).is_some() || self.classes.get(name).is_some() {
            return Ok(None);
        }
        use super::registry;
        match (method, args.len()) {
            // `Date.now()` — registered as a 0-arg `Function` returning I64 ms.
            ("now", 0) => {
                let call = registry::class_static("Date", "now", 0)
                    .expect("Date.now is registered");
                let v = self.emit_registry_call(module, &call, None, &[], JsKind::Number)?;
                Ok(Some(Val::new_with_kind(v.v, v.repr, JsKind::Number)))
            }
            // `Date.UTC(year, month, day?, hour?, min?, sec?, ms?)` — the registered
            // static takes 7 `I64`s (ms since epoch). At least year+month is
            // required (JS treats fewer as NaN — a divergent edge we refuse).
            ("UTC", 2..=7) => {
                let call = registry::class_static("Date", "UTC", 7)
                    .expect("Date.UTC is registered");
                let mut vals = self.lower_date_part_args(module, args, true)?;
                // Pad the missing tail with the JS defaults (day=1, rest=0) so the
                // `FROM_PARTS` extern (which does not itself default) is correct.
                self.pad_utc_defaults(&mut vals, args.len());
                let v = self.emit_registry_call(module, &call, None, &vals, JsKind::Number)?;
                Ok(Some(Val::new_with_kind(v.v, v.repr, JsKind::Number)))
            }
            ("UTC", _) => unsupported!(
                "`Date.UTC(..)` with fewer than 2 args (year+month) — the 0/1-arg \
                 NaN edge is a later increment"
            ),
            // `Date.parse(s)` — registered as `(StrPtr) -> F64`. The arg must be a
            // proven string (a non-string would diverge — refuse).
            ("parse", 1) => {
                let v = self.lower_expr(module, &args[0])?;
                if !matches!(v.kind, JsKind::Str) {
                    return unsupported!(
                        "`Date.parse(x)` with a non-string argument (ToString coercion \
                         is a later increment)"
                    );
                }
                let call = registry::class_static("Date", "parse", 1)
                    .expect("Date.parse is registered");
                let res = self.emit_registry_call(module, &call, None, &[v], JsKind::Number)?;
                Ok(Some(Val::new_with_kind(res.v, res.repr, JsKind::Number)))
            }
            _ => unsupported!("`Date.{method}({} args)` — no such static on Date", args.len()),
        }
    }

    /// Lower `Date.UTC`'s component args to `Val`s. With `pad=false` returns just
    /// the lowered values; the caller pads (the runtime `FROM_PARTS` needs all 7).
    fn lower_date_part_args(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        _pad: bool,
    ) -> FrontResult<Vec<Val>> {
        let mut vals = Vec::with_capacity(7);
        for a in args {
            vals.push(self.lower_expr(module, a)?);
        }
        Ok(vals)
    }

    /// Pad `vals` to 7 with `Date.UTC`'s JS defaults (day=1, hour/min/sec/ms=0).
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

/// Whether a Date method name is timezone-divergent or mutating — resolvable via
/// the Registry but NOT sound to lower (the determinism / immutable-instance
/// floor). Such a method BAILS rather than emitting a wrong-but-close value.
fn is_divergent_date_method(method: &str) -> bool {
    matches!(
        method,
        "toString"
            | "toDateString"
            | "toUTCString"
            | "toGMTString"
            | "toTimeString"
            | "toLocaleString"
            | "toLocaleDateString"
            | "toLocaleTimeString"
    ) || method.starts_with("set")
}
