//! `let`/`const` binding lowering + the local-classification reset and the
//! Tagged-local bind helper — split out of `stmt.rs` (the <500-line module rule).

use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirExpr, HirType};

use crate::repr::Repr;

use crate::front::error::FrontResult;
use crate::front::repr_map::repr_of;

use super::lower::{HeapShape, Local, Lowerer, Val, cl_type};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `let`/`const` with an initializer. The local's repr is the annotation when
    /// numeric; otherwise the initializer's value repr.
    pub(super) fn lower_let(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        ty: &HirType,
        init: Option<&HirExpr>,
    ) -> FrontResult<()> {
        // `let u;` — no initializer: the binding starts as `undefined` (JS). Bind a
        // Tagged local (or store into the gcell/cell for captured/module-level
        // bindings) holding the undefined word; a later assignment re-routes
        // through the normal Tagged assign.
        let Some(init) = init else {
            let undef = self.builder.ins().iconst(
                types::I64,
                crate::value::PolyValue::undefined().raw() as i64,
            );
            if self.is_main {
                if let Some(id) = self.gcells.get(name).copied() {
                    self.emit_gcell_set(module, id, undef)?;
                    return Ok(());
                }
            }
            if self.is_cell_local(name) {
                let handle = self.emit_cell_alloc(module, undef);
                self.bind_tagged_local(name, Val::new(handle, Repr::Tagged));
                return Ok(());
            }
            self.clear_local_classifications(name);
            self.bind_tagged_local(name, Val::new(undef, Repr::Tagged));
            return Ok(());
        };

        // MODULE-LEVEL MUTABLE GLOBAL (epic #195): a top-level `let` IN MAIN
        // written from a function. Store the initializer into the runtime cell (no
        // Cranelift Variable) — every later read/write of `name` routes through the
        // cell by id (`gcell_id` resolves it once `name` is not a local). Shape
        // tracking is dropped (a from-a-function-mutated var is opaque anyway).
        //
        // ONLY in `__rtsn_main`: a same-spelled `let` inside a FUNCTION declares a
        // fresh LOCAL that shadows the global (JS scoping) — routing it to the cell
        // made the prelude's `Console.__format` local `let out` CLOBBER a user
        // gcell named `out` (every console.log corrupted the captured variable).
        if self.is_main {
            if let Some(id) = self.gcells.get(name).copied() {
                let val = self.lower_expr(module, init)?;
                let word = self.box_value(val);
                self.emit_gcell_set(module, id, word)?;
                return Ok(());
            }
        }

        // FUNCTION-LOCAL CELL (#195): a function-scope `let` a closure captures AND
        // mutates. Allocate a per-invocation 1-slot cell holding the boxed init and
        // bind the HANDLE to a Tagged local — every later read/write of `name`
        // routes through `emit_cell_get`/`emit_cell_set`, and a capturing closure
        // snapshots the HANDLE by value (shared mutable box). Shape tracking is
        // dropped (a captured-mutated var is opaque).
        if self.is_cell_local(name) {
            // A HOISTED cell (a fn-valued decl — mutual forward refs) was
            // pre-allocated by the prologue: SET into that same box below. Any
            // other cell-local allocs a FRESH box at each execution of its decl
            // (a loop-body `let` gets the per-iteration binding).
            if self.local(name).is_none() || !self.hoisted_cells.contains(name) {
                let undef = self.builder.ins().iconst(
                    types::I64,
                    crate::value::PolyValue::undefined().raw() as i64,
                );
                let handle = self.emit_cell_alloc(module, undef);
                self.bind_tagged_local(name, Val::new(handle, Repr::Tagged));
            }
            let val = self.lower_expr(module, init)?;
            let word = self.box_value(val);
            let handle = self
                .local(name)
                .map(|l| l.var)
                .map(|v| self.builder.use_var(v))
                .expect("cell local just bound");
            self.emit_cell_set(module, handle, word);
            return Ok(());
        }

        // Re-`let` hygiene: from here `name` becomes a fresh Cranelift-local binding,
        // and exactly ONE of the classification branches below records its kind. Drop
        // EVERY prior classification of `name` up front so a re-`let` to a different
        // kind cannot leave a STALE entry in another map (e.g. `let s = "x"` then
        // `let s = {a:1}`: without this the old `string_locals["s"]` survives next to
        // the new object shape and `s.length` wrongly takes the string fast path).
        // Each branch re-inserts only its own; the single clear replaces the partial
        // per-branch removes that previously covered only the numeric tail.
        self.clear_local_classifications(name);

        // `const p = Promise.resolve(x)`: a registry-class STATIC call whose spec
        // return is a NAMED registered class (`resolve(): Promise<T>`) — record
        // the result's CLASS so `p.then(cb)` dispatches. Registration ONLY: the
        // binding itself follows the normal flow below (forcing a Tagged bind here
        // regressed `promise.wait(q)` — the namespace marshal wants the native
        // repr the generic tail preserves).
        if let HirExprKind::MethodCall { object, method, .. } = &init.kind {
            if let HirExprKind::Ident(cn) = &object.kind {
                if self.local(cn).is_none() {
                    if let Some(cls) = super::registry::class_statics(cn, method)
                        .iter()
                        .find_map(|c| c.ret_class.clone())
                        .filter(|c| super::registry::has_class(c))
                    {
                        self.global_instance_classes.insert(name.to_string(), cls);
                    }
                }
            }
        }

        // `const u = pathToFileURL(p)`: a BUILTIN-IMPORT namespace FUNCTION whose
        // spec return is a NAMED registered class (`): URL`) — record the result's
        // CLASS so `u.href` dispatches (the namespace-call analogue of the class-
        // static arm above; without it the URL handle reboxed as an opaque number).
        if let HirExprKind::Call { callee, args } = &init.kind {
            if let HirExprKind::Ident(fname) = &callee.kind {
                if let Some((ns, member)) = self.builtins.get(fname).cloned() {
                    if let Some(cls) = super::registry::namespace_member(&ns, &member, args.len())
                        .and_then(|c| c.ret_class)
                        .filter(|c| super::registry::has_class(c))
                    {
                        self.global_instance_classes.insert(name.to_string(), cls);
                    }
                }
            }
        }

        // GENERATOR-valued init: `const it = g()` where `g` is a generator. Mark
        // `it` so `it.next()`/`.return()`/`.throw()` route to `GENERATOR_*`. LAZY →
        // bind the raw `Int64` GenState handle; EAGER → bind the `__gen_buf` ARRAY
        // word + record the array shape (so `[...it]`/for-of/`it.length` work too).
        if let Some(is_lazy) = self.gen_call_kind(init) {
            let val = self.lower_expr(module, init)?;
            if is_lazy {
                let h = self.coerce(val, Repr::Int64)?;
                let var = self.builder.declare_var(cl_type(Repr::Int64));
                self.builder.def_var(var, h);
                self.locals.insert(
                    name.to_string(),
                    Local {
                        var,
                        repr: Repr::Int64,
                    },
                );
            } else {
                self.bind_tagged_local(name, val);
                self.local_shapes.insert(name.to_string(), HeapShape::Array);
            }
            self.generator_locals.insert(name.to_string(), is_lazy);
            return Ok(());
        }

        // Object/array literal initializers: lower specially and RECORD the
        // local's proven heap shape, so later `obj.key` / `arr[i]` / `arr.length`
        // resolve to constant-slot `VEC_GET`/`VEC_SET`. The local rides `Tagged`
        // (an i64 register holding the boxed object/array handle word).
        let shape = match &init.kind {
            HirExprKind::Object(fields) => {
                let (val, shape_id, lit_class) = self.lower_object_literal(module, fields)?;
                self.bind_tagged_local(name, val);
                // P5.15: a method-bearing literal recorded a synthesized literal-class;
                // record it on the local so `obj.method()` static-dispatches and
                // `${obj}`/`obj + 1`/`String(obj)` run the toString/valueOf chain.
                if let Some(class) = lit_class {
                    self.local_classes.insert(name.to_string(), class);
                }
                // A literal with COMPUTED keys (`{ [k]: v }`, the `\0`-prefixed
                // fields) has runtime-appended keys its static shape does not
                // track — recording the shape would statically prove a computed
                // key ABSENT. Leave the local dynamic (runtime slot-0 shape reads).
                if fields.iter().any(|(k, _)| k.starts_with('\0')) {
                    None
                } else {
                    Some(HeapShape::Object(shape_id))
                }
            }
            HirExprKind::Array(elems) => {
                let val = self.lower_array_literal(module, elems)?;
                self.bind_tagged_local(name, val);
                Some(HeapShape::Array)
            }
            // `let re = /pat/flags`: a regex LITERAL initializer (P5.12). Compile to
            // a RegExp instance word and record the local's static class so
            // `re.test(s)` / `re.source` / `re instanceof RegExp` dispatch.
            HirExprKind::Raw(_) if super::regex::is_regex_literal(init) => {
                let (pattern, flags) =
                    super::regex::regex_literal_parts(init).expect("is_regex_literal proved parts");
                let val = self.lower_regex_literal(module, &pattern, &flags)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes
                    .insert(name.to_string(), super::regex::REGEX_CLASS.to_string());
                return Ok(());
            }
            // `let a = new Array(n)`: the built-in Array constructor → an array
            // local (HeapShape::Array), NOT a class instance.
            HirExprKind::New { class, args } if self.is_builtin_array_ctor(class) => {
                let val = self.lower_new_array(module, args)?;
                self.bind_tagged_local(name, val);
                Some(HeapShape::Array)
            }
            // `let m = new Map()` / `new Set()` / `new Error(..)` / wrapper (P5.3):
            // a RUNTIME/Registry class instance. Record its static class in
            // `global_instance_classes` so `m.method()` / `m instanceof C` dispatch.
            HirExprKind::New { class, args } if self.is_global_class_ctor(class) => {
                let (val, class_name) = self.lower_new_global_class(module, class, args)?;
                self.bind_tagged_local(name, val);
                // A ctor whose spec declares an ARRAY return (`new Uint8Array(..):
                // number[]`, the Vec-backed typed arrays): the instance IS a plain
                // JS array — record the ARRAY shape (index/length/methods ride the
                // array surface), NOT the class (class dispatch would reject
                // `.subarray`/index writes).
                let is_array_instance = super::registry::class_ctors(&class_name)
                    .iter()
                    .any(|c| c.ret_is_array_handle);
                // A typed array constructed OVER an ArrayBuffer is a level-B live
                // VIEW (a keyed object sharing the buffer) — the PROVEN-array fast
                // paths (VEC_LEN/VEC_GET) would read its internal slots. Detected
                // statically by the ctor arg's class; the local keeps the CLASS
                // (its spec carries set/subarray/length) and every index/length
                // goes through the tag-dispatched dynamic path.
                let is_buffer_view = is_array_instance
                    && args.first().is_some_and(|a| {
                        matches!(
                            self.global_instance_class(a).as_deref(),
                            Some("ArrayBuffer") | Some("SharedArrayBuffer")
                        )
                    });
                if is_buffer_view {
                    // A level-B view: record the CLASS (its spec carries
                    // set/subarray/length dispatch) AND, when the element kind is
                    // known, a `TypedArrayView` shape so `a[i]` lowers to an inline
                    // native load/store (step 5b) instead of the dynamic path.
                    if let Some((elem_log2, signed, float)) =
                        super::lower::typed_array_kind(&class_name)
                    {
                        self.local_shapes.insert(
                            name.to_string(),
                            HeapShape::TypedArrayView {
                                elem_log2,
                                signed,
                                float,
                            },
                        );
                        // Compute (base_ptr, elem_count) ONCE here at the ctor site
                        // and cache them (step 5b hoisting). This point dominates
                        // every `a[i]`, so the access emitters read the cached
                        // Variables with `use_var` instead of a locked runtime call
                        // per element.
                        let view_word = self.load_local_word(name);
                        self.cache_ta_view_base_len(module, name, view_word)?;
                    }
                    self.global_instance_classes
                        .insert(name.to_string(), class_name);
                } else if is_array_instance {
                    self.local_shapes.insert(name.to_string(), HeapShape::Array);
                } else {
                    self.global_instance_classes
                        .insert(name.to_string(), class_name);
                }
                // No object FIELD shape (the instance is an opaque runtime handle);
                // return early so the generic `let` tail does not run.
                return Ok(());
            }
            // `let x = new F(args)` where `F` is a FUNCTION used as a constructor
            // (Phase 2/3): the result is opaque (F's object-return or the fresh
            // instance), so bind a plain Tagged local with no proven shape/class.
            HirExprKind::New { class, args } if self.is_fn_ctor(class) => {
                let val = self.lower_new_fn_ctor(module, class, args)?;
                self.bind_tagged_local(name, val);
                return Ok(());
            }
            // `let c = new G(args)` where `G` is a LOCAL holding a runtime
            // class-VALUE (`const G = globalThis.Box; const c = new G(5)`): invoke
            // through the new-thunk. The result is opaque (no static class/shape).
            HirExprKind::New { class, args }
                if self.local(class).is_some()
                    && self.local_class_refs.get(class).is_none()
                    && self.classes.get(class).is_none() =>
            {
                let val = self.lower_new_value(module, class, args)?;
                self.bind_tagged_local(name, val);
                return Ok(());
            }
            // `const f = new Function(p…, body)` — the PRIMORDIAL dynamic-function
            // ctor (unless a user class shadows the name): a function VALUE, so
            // NO class/shape is recorded (`.call`/`.apply` dispatch through the
            // fn-value path, not the class-method path).
            HirExprKind::New { class, args }
                if class == "Function" && self.classes.get(class).is_none() =>
            {
                let val = self.lower_new_function(module, args)?;
                self.bind_tagged_local(name, val);
                return Ok(());
            }
            // `let c = new C(args)`: build the instance, record the local's CLASS
            // (for static `c.method()` dispatch) and OBJECT shape (for `c.field`).
            HirExprKind::New { class, args } => {
                let (val, class_name, shape_id) = self.lower_new(module, class, args)?;
                self.bind_tagged_local(name, val);
                self.local_classes.insert(name.to_string(), class_name);
                Some(HeapShape::Object(shape_id))
            }
            // `const a = buf.slice(0,4)`: a registry instance-method whose spec
            // return type is a NAMED registered class (`slice(): ArrayBuffer`).
            // Record the result's class in `global_instance_classes` so a chained
            // `a.byteLength` / `a.method()` dispatches (the registry-instance form,
            // parallel to the user-class arm below which records `local_classes`).
            HirExprKind::MethodCall { object, method, args }
                if self
                    .registry_method_ret_class(object, method, args.len())
                    .is_some() =>
            {
                let class = self
                    .registry_method_ret_class(object, method, args.len())
                    .expect("guarded by the match arm");
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes.insert(name.to_string(), class);
                return Ok(());
            }
            // `const c = a + b` where `+` resolves the Rust-style OPERATOR
            // OVERLOAD (`a.add(b)`) and the method's return type is a known
            // class: record `c`'s class so `c.describe()` / `c == f` dispatch.
            HirExprKind::Bin { op, lhs, .. }
                if self.overload_ret_class(lhs, *op).is_some() =>
            {
                let class = self
                    .overload_ret_class(lhs, *op)
                    .expect("guarded by the match arm");
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                if let Some(desc) = self.classes.get(&class) {
                    let shape_id = self.shapes.intern(&desc.fields);
                    self.local_shapes
                        .insert(name.to_string(), HeapShape::Object(shape_id));
                }
                self.local_classes.insert(name.to_string(), class);
                return Ok(());
            }
            // `const sp = u.searchParams`: a registry-instance PROPERTY read whose
            // spec return is a NAMED registered class (`searchParams:
            // URLSearchParams`) — same tracking as the method-call arm above.
            HirExprKind::Member { object, prop }
                if self.registry_method_ret_class(object, prop, 0).is_some() =>
            {
                let class = self
                    .registry_method_ret_class(object, prop, 0)
                    .expect("guarded by the match arm");
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes.insert(name.to_string(), class);
                return Ok(());
            }
            // `const s = Symbol("x")`: the PRIMORDIAL `Symbol` call form returns a
            // symbol instance. Record `s`'s class as "Symbol" (in
            // `global_instance_classes`) so `s.description` dispatches the registry
            // instance getter. (Engine may name Symbol — it is primordial.)
            HirExprKind::Call { callee, args: _ }
                if matches!(&callee.kind, HirExprKind::Ident(n) if n == "Symbol")
                    && self.local("Symbol").is_none() =>
            {
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.global_instance_classes
                    .insert(name.to_string(), "Symbol".to_string());
                return Ok(());
            }
            // `const dc = structuredClone(d)` where `d`'s class is known (a
            // registry/user class instance): the clone PRESERVES the type, so
            // propagate the arg's classification to the receiver — `dc.getUTC*`
            // / `.has` / `.size` static-dispatch like the original. Only the
            // ident-arg form (the provable case); anything else stays dynamic.
            HirExprKind::Call { callee, args }
                if matches!(&callee.kind, HirExprKind::Ident(n) if n == "structuredClone")
                    && self.local("structuredClone").is_none()
                    && args.first().is_some_and(|a| {
                        self.global_instance_class(a).is_some()
                            || matches!(&a.kind,
                                HirExprKind::Ident(v) if self.local_classes.contains_key(v.as_str()))
                    }) =>
            {
                let arg = args.first().expect("guarded by the match arm");
                let g_class = self.global_instance_class(arg);
                let l_class = match &arg.kind {
                    HirExprKind::Ident(v) => self.local_classes.get(v.as_str()).cloned(),
                    _ => None,
                };
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                if let Some(c) = g_class {
                    self.global_instance_classes.insert(name.to_string(), c);
                } else if let Some(c) = l_class {
                    self.local_classes.insert(name.to_string(), c);
                }
                return Ok(());
            }
            // `const c2 = c.inc()` / `const r = make()`: a CALL/METHOD-CALL whose
            // result class is statically provable (`ret_class` — `return this` /
            // `return new C`). Record the local's CLASS so a later `c2.method()`
            // static-dispatches (the fluent-builder via-variable form). No object
            // FIELD shape is recorded: the result may be `this` (receiver's shape) OR
            // a fresh `new C` (a different shape), which we cannot tell apart here, so
            // `c2.field` stays dynamic/bails (sound) while `c2.method()` works.
            HirExprKind::MethodCall { .. } | HirExprKind::Call { .. }
                if self.static_instance_class(init).is_some() =>
            {
                let class = self
                    .static_instance_class(init)
                    .expect("guarded by the match arm");
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.local_classes.insert(name.to_string(), class);
                return Ok(());
            }
            // `const C = Box`: bind the local to the reified class VALUE (a
            // `TAG_FUNCTION` new-thunk word) AND record that `C` REFERENCES class
            // `Box`, so a later `new C(args)` constructs `Box` on the static path.
            // Guarded so a local of the same name shadows the class (then it is not
            // a class reference). Reassignment clears the ref (the removes below).
            HirExprKind::Ident(c)
                if self.local(c).is_none() && self.classes.get(c).is_some() =>
            {
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.local_class_refs.insert(name.to_string(), c.clone());
                return Ok(());
            }
            // `const G = globalThis.X` where `globalThis.X` is a statically-tracked
            // class (every `globalThis.X = <Class>` agrees, via the all-agree
            // pre-pass): bind the read value AND record `G` as a class reference, so
            // `new G(args)` constructs `<Class>` on the static path → the result is
            // method-dispatchable. Reuses the slice-1 `local_class_refs` machinery;
            // a poisoned/untracked key falls through to the dynamic slice-2 path.
            HirExprKind::Member { object, prop }
                if self.is_globalthis_receiver(object)
                    && self.globalthis_class_refs.contains_key(prop) =>
            {
                let class = self.globalthis_class_refs[prop].clone();
                let val = self.lower_expr(module, init)?;
                self.bind_tagged_local(name, val);
                self.local_class_refs.insert(name.to_string(), class);
                return Ok(());
            }
            _ => None,
        };
        if let Some(shape) = shape {
            self.local_shapes.insert(name.to_string(), shape);
            return Ok(());
        }

        let val = self.lower_expr(module, init)?;

        // An array-VALUED expression (e.g. `let b = a.slice(1, 3)`) produces a
        // TAG_OBJECT array word: bind it as a Tagged local AND record the array
        // shape, so `b.length` / `b[i]` / `b.method(..)` resolve like a literal.
        // Array-VALUED either by the lowered KIND (`a.slice(..)` tags `Array`) OR by
        // a statically array-returning call/method (`const vs = m.values()` — the
        // declared `T[]` return, via `is_array_valued`'s call/method arms). Both bind
        // a Tagged local + record the array shape so `vs.length`/`vs[i]`/`for-of`
        // resolve like a literal array.
        if matches!(val.kind, super::lower::JsKind::Array) || self.is_array_valued(init) {
            self.bind_tagged_local(name, val);
            self.local_shapes.insert(name.to_string(), HeapShape::Array);
            return Ok(());
        }

        // A STRING-valued initializer (`let s = "x"`, `let s = a + b` over strings,
        // a string-returning method): record `name` as a proven string so a later
        // `s.method(...)` dispatches on `class String` (the `.ts` primitive-method
        // library) exactly like a string literal — not just the handful of methods
        // in the dynamic table. The local rides Tagged (the boxed string word); only
        // the proven-string FACT is recorded. (Mirrors the array-shape branch above.)
        if matches!(val.kind, super::lower::JsKind::Str) {
            self.bind_tagged_local(name, val);
            self.string_locals.insert(name.to_string());
            return Ok(());
        }

        // The local's repr is the numeric annotation when the INITIALIZER ITSELF is
        // unboxed-numeric; a Tagged initializer keeps its `Tagged` repr even under a
        // numeric annotation. This is the soundness seam: `let a: number = null`
        // (or a bare `let a = null`/`= undefined`) evaluates to a Tagged singleton
        // PolyValue word, and forcing it into a `Float64`/`Int` slot would reinterpret
        // the singleton bits as a number (reading back NaN/0 instead of `null`/
        // `undefined`). Only widen to the annotation when the value can actually live
        // there unboxed.
        // The local's repr: widen to the (unboxed) annotation only when it does NOT
        // DEMOTE the value. A `Float64` initializer must NEVER land in an integer
        // slot — JS `number` is one type and `/` always yields a real number, so
        // `const r = 44100 / 48000` is `0.91875`, not `0`. The HIR may infer the
        // const's type as an integer (both operands are int literals), but coercing
        // the genuine `Float64` result to `Int` truncates the fraction (the bug:
        // `ratio` became 0 → an audio resampler read frame 0 forever = silence).
        // So an `Int*` annotation over a `Float64` value keeps `Float64`.
        let annotated = repr_of(ty);
        let demotes_float =
            matches!(val.repr, Repr::Float64) && matches!(annotated, Repr::Int32 | Repr::Int64);
        // A `Bool` value must NEVER be widened to a numeric annotation: `bool` and
        // `number` are distinct JS types, and forcing a bool into an Int slot loses
        // its boolean identity (`typeof` flips to `"number"`, the value prints `1`
        // instead of `true`). The HIR sometimes mis-infers a `boolean`-result init
        // as `number` (e.g. `new Date(0) instanceof Date` was typed numeric, so a
        // `const x = …instanceof…` became a number `1`). Keep the proven `Bool`.
        let widens_bool_to_num = matches!(val.repr, Repr::Bool)
            && matches!(annotated, Repr::Int32 | Repr::Int64 | Repr::Float64);
        let repr = if annotated.is_unboxed()
            && val.repr.is_unboxed()
            && !demotes_float
            && !widens_bool_to_num
        {
            annotated
        } else {
            val.repr
        };
        // FLOAT-ACCUMULATOR promotion (pre-scan): `let s = 0` later accumulated
        // with a heap-read number (`s = s + p.age`) binds `Float64` — an `Int*`
        // binding would truncate every fractional carry at the assign's coerce.
        // JS numbers are f64, so widening an int init is always sound.
        let repr =
            if matches!(repr, Repr::Int32 | Repr::Int64) && self.float_promoted.contains(name) {
                Repr::Float64
            } else {
                repr
            };

        let coerced = self.coerce(val, repr)?;
        let var = self.builder.declare_var(cl_type(repr));
        self.builder.def_var(var, coerced);
        self.locals.insert(name.to_string(), Local { var, repr });
        // No per-branch shape/class removal needed: `clear_local_classifications`
        // already dropped every prior classification of `name` at the top of the
        // (re)binding, and this numeric tail records none.
        Ok(())
    }

    /// Drop EVERY compile-time classification of local `name` (proven shape, object/
    /// string kind, user/registry class, class-ref, generator). Called once at the
    /// top of a (re)binding so a re-`let` to a different kind cannot leave a stale
    /// entry in a map another branch does not touch. Does NOT touch `locals` (the
    /// Cranelift variable binding itself), which the binding branch replaces.
    fn clear_local_classifications(&mut self, name: &str) {
        self.local_shapes.remove(name);
        self.object_locals.remove(name);
        self.string_locals.remove(name);
        self.local_classes.remove(name);
        self.local_class_refs.remove(name);
        self.global_instance_classes.remove(name);
        self.generator_locals.remove(name);
    }

    /// Bind `name` to a fresh `Tagged` local holding `val.v` (used for
    /// object/array literal locals, whose value is a boxed handle word). The
    /// caller records the heap shape separately.
    pub(super) fn bind_tagged_local(&mut self, name: &str, val: Val) {
        let var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.builder.def_var(var, val.v);
        self.locals.insert(
            name.to_string(),
            Local {
                var,
                repr: Repr::Tagged,
            },
        );
    }
}
