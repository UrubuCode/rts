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
//! The unsound-to-lower mechanism is SPEC DATA: a class may stamp a member
//! `MemberFlags::UNSOUND` and [`Lowerer::try_registry_instance_method`] bails on
//! the flag generically — no per-class predicate in the engine. (Date's
//! formatters + `setX` mutators carried the flag until 2026-07-02; the suite
//! defines their DETERMINISTIC UTC surface as the modeled semantic, so the flag
//! was dropped from the spec — the mechanism stays for future specs.)
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
//! (overloads, defaults) is SPEC DATA in the `rts-shared` Date class
//! (`Sig::default_args` + the registered members).

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
        // A ctor whose spec declares an ARRAY return (`new Uint8Array(..):
        // number[]` — the Vec-backed typed arrays) reboxes as a plain JS array
        // (`__rtsadp_box_handle_auto`), so the instance rides the whole array
        // surface (length/index/iteration). Everything else stays an Object.
        let kind = if call.ret_is_array_handle {
            JsKind::Array
        } else {
            JsKind::Object
        };
        let res = self.emit_registry_call(module, &call, None, &vals, kind)?;
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
                // Kind Undefined: the StrPtr marshal maps a defaulted string
                // slot to "" (not the "undefined" text); numeric slots decode
                // the sentinel NaN as before.
                Val::tagged_kind(w, JsKind::Undefined)
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
        // hardcoded per-class predicate. BAIL instead of emitting a wrong value.
        if call.flags.contains(MemberFlags::UNSOUND) {
            return unsupported!(
                "`{class}.{method}()` — unsound-to-lower method (timezone-divergent / \
                 mutating; only the deterministic surface is modeled — a later \
                 increment)"
            );
        }
        let recv = self.lower_expr(module, object)?;
        let mut vals: Vec<Val> = Vec::with_capacity(call.arg_abis.len());
        for a in args {
            vals.push(self.lower_expr(module, a)?);
        }
        // Pad the omitted TAIL with the spec's `default_args` (an INSTANCE
        // method with optional args — `d.setUTCHours(h)` on the 4-slot spec):
        // the same data-driven fill the ctor/static paths already apply.
        for i in args.len()..call.arg_abis.len() {
            vals.push(self.default_arg_val(call.default_args.get(i), class, i)?);
        }
        // A `Handle` return is a STRING handle only when the spec's ts-signature
        // says so (`ret_is_string_handle`, e.g. Date.toISOString → string);
        // otherwise it is an OBJECT handle (e.g. `WeakRef.deref(): object`). Using
        // the data-driven flag instead of "Handle ⇒ Str" keeps Date strings as Str
        // while letting object-returning methods box as TAG_OBJECT.
        let result_kind = match call.ret {
            AbiType::Handle if call.ret_is_string_handle => JsKind::Str,
            // A FRESH JS-array return (ts `): T[]`, e.g. `URLSearchParams.getAll`):
            // rebox via `__rtsadp_box_handle_auto`, which normalizes legacy raw
            // string-handle elements to words — an object-boxed raw Vec read its
            // elements as garbage doubles.
            AbiType::Handle if call.ret_is_array_handle => JsKind::Array,
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
        let call = super::registry::class_member(&recv_class, method, argc)
            .or_else(|| super::registry::class_instance_getter(&recv_class, method))?;
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
        self.resolve_static_via_registry(module, name, method, args)
            .map(Some)
    }

    /// Resolve `Class.method(args)` through the Registry's static-member metadata
    /// (overload pick by [required, total] arity window → arg marshal → emit). The
    /// generic engine for BOTH a pure-Registry class (`Date`, gated by the caller)
    /// and a PRIMORDIAL with registry-backed statics (`Promise.resolve`, routed from
    /// the global-static path). `name` must already be confirmed a static class
    /// reference (not a shadowing local) by the caller.
    pub(super) fn resolve_static_via_registry(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
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
        // Lower the provided args. A `StrPtr` param fed a non-proven-string is
        // ToString-coerced by the generic marshal (`marshal_reg_arg`'s StrPtr arm
        // routes the value word through `__rtsadp_to_string` — the ONE coercion
        // authority, Pilar 3), exactly like the instance-method path already does.
        // This is JS-correct: `Date.parse(123)` ≡ `Date.parse("123")` (→ NaN),
        // `Buffer.from(x)` ToStrings a non-string `x`. (Was previously a bail.)
        let mut vals: Vec<Val> = Vec::with_capacity(total);
        for a in args.iter() {
            vals.push(self.lower_expr(module, a)?);
        }
        // Pad the omitted trailing args with the spec-declared defaults. A tail
        // slot is always a default here (argc ≥ required), so a `Required` /
        // missing entry would be a spec bug — BAIL rather than mis-marshal.
        for i in argc..total {
            vals.push(self.default_arg_val(call.default_args.get(i), name, i)?);
        }
        // Same data-driven Handle-return classification as the instance path:
        // a string only when the ts-signature says so, an array when it
        // declares `T[]`, otherwise an OBJECT handle (tag-dispatched rebox).
        // The old blanket "Handle ⇒ Str" made `Promise.withResolvers()` (a
        // `{promise, resolve, reject}` Map handle) ride as a string.
        let result_kind = match call.ret {
            AbiType::Handle if call.ret_is_string_handle => JsKind::Str,
            AbiType::Handle if call.ret_is_array_handle => JsKind::Array,
            AbiType::Handle => JsKind::Object,
            AbiType::Bool => JsKind::Bool,
            _ => JsKind::Number,
        };
        let v = self.emit_registry_call(module, &call, None, &vals, result_kind)?;
        Ok(v)
    }
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// DYNAMIC Registry-class instance dispatch on a TAGGED receiver of unproven
    /// class (the Registry counterpart of `try_user_virtual_dynamic`): for every
    /// Registry class that declares an `instanceof_predicate` AND resolves
    /// `method` at this argc, emit a runtime arm
    /// `if is_object(recv) && predicate(handle) { marshal + call }` — fully
    /// DATA-DRIVEN (iterates the Registry metadata; the front names no class).
    /// Args lower ONCE before any branch (single-eval; a Spread declines). The
    /// fall-through (no predicate matched / non-object receiver) yields the
    /// `undefined` sentinel — a TypeError class in JS, never a wrong value.
    ///
    /// `Ok(None)` when NO candidate exists — the caller keeps its later
    /// fallbacks, so this must be tried LAST in the Tagged dispatch chain.
    pub(super) fn try_registry_virtual_dynamic(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        use super::registry;
        let cands = registry::dyn_instance_candidates(method, args.len());
        if cands.is_empty() {
            return Ok(None);
        }
        if args
            .iter()
            .any(|a| matches!(a.kind, HirExprKind::Spread(_)))
        {
            return Ok(None);
        }
        // Single-eval: lower every arg ONCE; the SSA values dominate all arms.
        let mut argvals: Vec<Val> = Vec::with_capacity(args.len());
        for a in args {
            argvals.push(self.lower_expr(module, a)?);
        }
        self.emit_registry_dyn_arms(module, recv, &cands, &argvals, args.len(), None)
            .map(Some)
    }

    /// DYNAMIC Registry instance GETTER dispatch on a TAGGED receiver — the
    /// member-READ counterpart of [`Self::try_registry_virtual_dynamic`]
    /// (`re.source` / `re.flags` / `d.lastIndex` through an `any`): one arm per
    /// predicate-carrying class declaring the getter. `Ok(None)` when no
    /// candidate exists (the caller keeps its dynamic data read).
    pub(super) fn try_registry_getter_dynamic(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        prop: &str,
    ) -> FrontResult<Option<Val>> {
        use super::registry;
        let cands = registry::dyn_getter_candidates(prop);
        if cands.is_empty() {
            return Ok(None);
        }
        // The fall-through reads the DATA SLOT (`__rtsadp_obj_get`) — a plain
        // object with a real `prop` key (a JSON `flags` field) must not be
        // stolen by a same-named Registry getter's `undefined` sentinel.
        self.emit_registry_dyn_arms(module, recv, &cands, &[], 0, Some(prop))
            .map(Some)
    }

    /// Shared arm emitter for the two dynamic Registry dispatchers: gate on
    /// `is_object(recv)`, then one `if predicate(handle) { marshal + call }`
    /// arm per candidate (defaults padded from the spec), merging every arm's
    /// boxed result. The fall-through yields the `undefined` sentinel — or,
    /// when `data_read_key` is set (the GETTER variant), the plain
    /// `__rtsadp_obj_get` data-slot read (a plain object's real `prop` value).
    fn emit_registry_dyn_arms(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        cands: &[(String, &'static str, super::registry::ResolvedCall)],
        argvals: &[Val],
        argc: usize,
        data_read_key: Option<&str>,
    ) -> FrontResult<Val> {
        let recv_word = self.box_value(recv);
        // is_object gate + the real handle (same emission as
        // `emit_registry_instanceof`; predicates tolerate a bogus handle → 0).
        let boxed = value::emit_is_boxed(self.builder, recv_word);
        let shifted = self
            .builder
            .ins()
            .ushr_imm(recv_word, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let want = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let is_obj_tag = self.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::Equal,
            tag,
            want,
        );
        let is_object = self.builder.ins().band(boxed, is_obj_tag);
        let handle = value::emit_marshal::emit_table_load(module, self.builder, recv_word);

        let merge = self.builder.create_block();
        self.builder
            .append_block_param(merge, types::I64);
        for (class, pred, call) in cands {
            let pred_v = value::emit_marshal::emit_call_sig(
                module,
                self.builder,
                pred,
                &[handle],
                &[AbiType::Handle],
                AbiType::Bool,
            )
            .expect("instanceof predicate returns a value");
            // Normalize the predicate's i64 0/1 to the icmp width before the
            // `band` with `is_object` (mixed-width band is a verifier error).
            let pred_b = self.builder.ins().icmp_imm(
                cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                pred_v,
                0,
            );
            let cond = self.builder.ins().band(is_object, pred_b);
            let arm_blk = self.builder.create_block();
            let next_blk = self.builder.create_block();
            self.builder.ins().brif(cond, arm_blk, &[], next_blk, &[]);

            self.builder.switch_to_block(arm_blk);
            self.builder.seal_block(arm_blk);
            // Pad the omitted defaulted tail (same rule as the static instance
            // path) and classify the Handle return by the spec's flags.
            let mut vals = argvals.to_vec();
            for i in argc..call.arg_abis.len() {
                vals.push(self.default_arg_val(call.default_args.get(i), class, i)?);
            }
            let result_kind = match call.ret {
                AbiType::Handle if call.ret_is_string_handle => JsKind::Str,
                AbiType::Handle if call.ret_is_array_handle => JsKind::Array,
                AbiType::Handle => JsKind::Object,
                AbiType::Bool => JsKind::Bool,
                _ => JsKind::Number,
            };
            let v = self.emit_registry_call(module, call, Some(recv), &vals, result_kind)?;
            let w = self.box_value(v);
            self.builder.ins().jump(merge, &[w.into()]);

            self.builder.switch_to_block(next_blk);
            self.builder.seal_block(next_blk);
        }
        // DEFAULT: no predicate matched. The GETTER variant falls back to the
        // plain DATA-SLOT read (a same-named real key on a plain object);
        // the METHOD variant yields the `undefined` sentinel.
        match data_read_key {
            Some(prop) => {
                let key = crate::value::abi_adapter::intern_poly(prop);
                let key_word = self.builder.ins().iconst(types::I64, key.raw() as i64);
                let data = self
                    .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
                    .expect("__rtsadp_obj_get returns a value");
                self.builder.ins().jump(merge, &[data.into()]);
            }
            None => {
                let undef = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                self.builder.ins().jump(merge, &[undef.into()]);
            }
        }

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let result = self.builder.block_params(merge)[0];
        Ok(Val::new_with_kind(
            result,
            crate::repr::Repr::Tagged,
            JsKind::Unknown,
        ))
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
