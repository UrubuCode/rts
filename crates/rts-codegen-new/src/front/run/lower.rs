//! Whole-program HIR → Cranelift lowering: the Tagged/polymorphic path.
//!
//! Where [`crate::front::hir_lower`] is the strict numeric-only fast path (used
//! by the increment-3 tests, unchanged), this lowerer adds the `Tagged`
//! [`crate::value::PolyValue`] path so a REAL program — top-level code, string
//! literals, `console.log`, `typeof`, `===`, cross-function calls — runs end to
//! end. Proven-numeric operands still use native `iadd`/`fadd`/… (the fast path
//! is NOT regressed); a value boxes to a `PolyValue` only at a Tagged boundary
//! (a `console.log` arg, a `+` on a string/mixed pair, a Tagged local, a Tagged
//! call argument), via the pure `value::emit_*` IR helpers and the generic
//! `__rtsn_*` runtime ops.
//!
//! Every unmodeled construct is an EXPLICIT [`Unsupported`] bail (design pilar 2):
//! the program runner refuses to run a partially-lowered module rather than emit
//! a wrong value.
//!
//! Submodules: this file holds the driver + function shell + local/coercion
//! scaffolding; [`super::expr`] lowers expressions, [`super::stmt`] statements +
//! control flow. Each stays well under 500 lines.

use std::collections::HashMap;

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;

use rts_hir::{HirFunc, HirStmt};

use crate::repr::Repr;
use crate::shape::{ShapeId, ShapeTable};
use crate::value;

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::sig::FnSig;

/// The Cranelift type a value of `repr` is carried in: `Float64` in an `f64`,
/// every unboxed-integer / `Bool` / `Tagged` value in an `i64` register.
pub fn cl_type(repr: Repr) -> types::Type {
    match repr {
        Repr::Float64 => types::F64,
        _ => types::I64,
    }
}

/// A proven JS *type-kind* hint, finer than [`Repr`] for the cases where the
/// HIR has thrown away operator distinctions the engine must respect soundly.
///
/// Concretely: swc maps BOTH `==` and `===` to one HIR op, and BOTH unary `+`
/// and `!` to one HIR op — so the engine cannot tell them apart from the HIR.
/// Strict equality and ToBoolean-not are only sound to lower when the operand
/// kinds make `==`/`===` (resp. `+`/`!`) agree. `JsKind` records the kind where
/// it is statically provable (literals, comparison results) so the lowering can
/// keep the sound cases and BAIL the ambiguous ones — never emit a wrong value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JsKind {
    Number,
    Bool,
    Str,
    Null,
    Undefined,
    /// A whole ARRAY value (a `TAG_OBJECT` PolyValue over a real `Entry::Vec`),
    /// produced by an array-returning method (e.g. `arr.slice(..)`). Lets a `let`
    /// record [`HeapShape::Array`] so the new local supports `.length`/`[i]`/
    /// array methods.
    Array,
    /// A FUNCTION value (a `TAG_FUNCTION` PolyValue over a reified `Entry::Function`
    /// holding the thunk address). Produced by reifying a user-fn ident or an
    /// extracted arrow (P4.6); makes `f(args)` lower to the indirect invoke path
    /// and `typeof f` fold to `"function"`.
    Function,
    /// A class-instance OBJECT value (a `TAG_OBJECT` PolyValue over the instance
    /// Vec). Produced by `new C(args)` (P4.9); makes a `let` record the local's
    /// class + OBJECT shape and routes `console.log` through the object inspect.
    Object,
    /// Not statically provable (a Tagged value from a variable/call/etc.).
    Unknown,
}

/// An SSA value tagged with the representation it carries + a static kind hint.
#[derive(Clone, Copy)]
pub(crate) struct Val {
    pub v: Value,
    pub repr: Repr,
    pub kind: JsKind,
}

impl Val {
    /// A value whose kind follows from its repr (number/bool reprs are
    /// self-describing; a Tagged value's kind is `Unknown` unless set explicitly).
    pub(crate) fn new(v: Value, repr: Repr) -> Val {
        let kind = match repr {
            Repr::Float64 | Repr::Int32 | Repr::Int64 => JsKind::Number,
            Repr::Bool => JsKind::Bool,
            _ => JsKind::Unknown,
        };
        Val { v, repr, kind }
    }

    /// A Tagged value with an explicit proven kind (string/null/undefined literal).
    pub(crate) fn tagged_kind(v: Value, kind: JsKind) -> Val {
        Val {
            v,
            repr: Repr::Tagged,
            kind,
        }
    }

    /// A value carrying an explicit repr AND an explicit proven kind (used where a
    /// runtime trampoline returns a Tagged word whose JS kind the lowering knows —
    /// e.g. a Map `.size` is a number, an Error `.message` is a string).
    pub(crate) fn new_with_kind(v: Value, repr: Repr, kind: JsKind) -> Val {
        Val { v, repr, kind }
    }
}

/// One active enclosing loop's break/continue jump targets (P5.10). Pushed on the
/// [`Lowerer::loop_stack`] when a loop body is lowered, popped after; a `break`
/// jumps to `exit`, a `continue` to `continue_target`.
#[derive(Clone)]
pub(crate) struct LoopCtx {
    /// The block a `break` jumps to (the loop's exit / continuation).
    pub exit: cranelift_codegen::ir::Block,
    /// The block a `continue` jumps to (header / update / advance step).
    pub continue_target: cranelift_codegen::ir::Block,
    /// The source LABEL of this loop (`outer: for …`), if any — the target of a
    /// `break label` / `continue label`. `None` for an unlabeled loop/switch.
    pub label: Option<String>,
    /// The `finally_stack` depth at the point this loop's body starts. A `break`/
    /// `continue` targeting this loop must run every `finally` pushed AFTER this
    /// depth (the finalizers of `try/finally` blocks the jump escapes), innermost
    /// first, before jumping — JS runs `finally` on an abrupt loop exit too.
    pub finally_depth: usize,
}

/// One active enclosing `try` (P5.13): the block a pending error routes to. For a
/// `try`/`catch` it is the `catch` handler block; for a `try`/`finally` with no
/// `catch` it is the `finally` block (which re-checks the pending flag and lets a
/// still-pending error propagate after running the finalizer).
#[derive(Clone, Copy)]
pub(crate) struct TryCtx {
    /// The block a pending error jumps to (innermost `catch`, or `finally` when
    /// there is no `catch`).
    pub on_error: cranelift_codegen::ir::Block,
}

/// A local binding: its Cranelift variable + the repr it holds. A local has a
/// single stable repr for the whole function; a `let` that would need two reprs
/// on different paths uses `Tagged` (decided at declaration).
#[derive(Clone, Copy)]
pub(crate) struct Local {
    pub var: Variable,
    pub repr: Repr,
}

/// The statically-proven heap shape of a local that holds an object or array
/// literal — what makes `obj.key` / `arr[i]` / `arr.length` lowerable to a
/// constant-slot `VEC_GET`/`VEC_SET`. A local without an entry here holds an
/// opaque value (param, call return, reassigned); an access on it BAILS.
/// The element kind `(elem_log2, signed, float)` of a typed-array CLASS, or
/// `None` for a class that is not one of the 8 numeric typed arrays. Mirrors the
/// per-class `ta_ctor!` registrations in `rts-adapters::value::taops` — the ONE
/// place both the runtime and the native lowering agree on the layout.
pub(crate) fn typed_array_kind(class: &str) -> Option<(u8, bool, bool)> {
    Some(match class {
        "Uint8Array" => (0, false, false),
        "Int8Array" => (0, true, false),
        "Uint16Array" => (1, false, false),
        "Int16Array" => (1, true, false),
        "Uint32Array" => (2, false, false),
        "Int32Array" => (2, true, false),
        "Float32Array" => (2, false, true),
        "Float64Array" => (3, false, true),
        _ => return None,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum HeapShape {
    /// An object literal with a known compile-time [`ShapeId`] (key→slot map).
    Object(ShapeId),
    /// An array literal: indices are dense `0..len`; `.length` is `VEC_LEN`.
    Array,
    /// A level-B typed-array VIEW over an `ArrayBuffer`, proven at the ctor site
    /// (`new Uint8Array(ab)`). Carries the element kind as compile constants so
    /// `a[i]` lowers to an inline `base + (i << elem_log2)` load/store instead of
    /// a locked runtime call (step 5b). `elem_log2 ∈ {0,1,2,3}` for byte widths
    /// 1/2/4/8; `signed`/`float` select the load/store instruction.
    TypedArrayView {
        elem_log2: u8,
        signed: bool,
        float: bool,
    },
}

/// The per-function lowering context.
pub(crate) struct Lowerer<'a, 'b, 'c> {
    pub builder: &'a mut FunctionBuilder<'b>,
    /// Name → local binding (flat; re-`let` reuses the slot).
    pub locals: HashMap<String, Local>,
    /// Name → the proven heap shape of a local holding an object/array literal.
    /// Absent ⇒ the local's value is opaque; a property/index access on it bails.
    pub local_shapes: HashMap<String, HeapShape>,
    /// Names PROVEN to hold a keyed OBJECT value but whose exact compile-time
    /// shape (key→slot map) is not known — e.g. an object local reassigned to a
    /// DIFFERENT object literal. A property/index access on such a local routes
    /// through the DYNAMIC trampolines (`__rtsadp_obj_get`/`_set`), which read the
    /// shape-id at runtime, instead of bailing. Distinct from `local_shapes`: a
    /// name in `local_shapes` (a known shape) uses the faster constant-slot path.
    pub object_locals: std::collections::HashSet<String>,
    /// Names PROVEN to hold a native `string` value — a param declared `: string`
    /// (mirrors how an array param records [`HeapShape::Array`] in `local_shapes`).
    /// A member/index/method access on such a name takes the native string
    /// fast-path (`.length` / `s[i]` / string methods) instead of bailing on an
    /// unknown shape. The local keeps its Tagged ABI/repr (the i64 register holds
    /// the boxed string word); only the proven-string fact is recorded here.
    pub string_locals: std::collections::HashSet<String>,
    /// FUNCTION-LOCAL CELLS (#195): names in the CURRENT function whose local
    /// Variable holds a 1-slot-`Vec` cell HANDLE because a closure CAPTURES and
    /// MUTATES them. A read routes through `emit_cell_get`, a write through
    /// `emit_cell_set`, and the declaration (`let`) allocates the cell. Owned (not
    /// shared) — the per-function set computed by [`super::funcval`] and passed into
    /// [`Self::lower_function`]. The closure thunk receives the same names as its
    /// captured leading PARAMS (already holding the handle), so it shares the set.
    pub cell_locals: std::collections::HashSet<String>,
    /// Cell-locals HOISTED by the prologue (fn-valued declarations only — the
    /// mutual-forward-ref set). Their `let`/`const` stmt SETs into the
    /// pre-allocated box instead of re-allocating; every other cell-local keeps
    /// the alloc-at-decl behavior (a loop-body `let` needs a FRESH cell per
    /// pass for the per-iteration capture semantics).
    pub hoisted_cells: std::collections::HashSet<String>,
    /// Name → the statically-known CLASS of a local/param holding a class instance
    /// (a `new C()` result, a `: C`-annotated param, or `this` inside a method).
    /// Drives static `instance.method(args)` dispatch; absent ⇒ method calls bail.
    pub local_classes: HashMap<String, String>,
    /// Name → the user CLASS a local holds as a VALUE (`const C = Box` → `C`→`Box`).
    /// Distinct from `local_classes` (which holds an INSTANCE's class): here the
    /// local IS the class/constructor itself, so `new C(args)` constructs `Box`
    /// statically. Same insert/remove lifecycle as `local_classes` (cleared on any
    /// rebind of the name). Absent ⇒ `new <local>()` is not a static class value.
    pub local_class_refs: HashMap<String, String>,
    /// GENERATOR locals: `const it = g()` where `g` is a generator. `true` = LAZY
    /// (GenState handle, `Int64`); `false` = EAGER (`__gen_buf` ARRAY word).
    /// `it.next()`/`.return()`/`.throw()` route to `GENERATOR_*` by this kind.
    pub generator_locals: HashMap<String, bool>,
    /// Name → the statically-known RUNTIME/Registry class of a local holding a
    /// `new Map()`/`new Set()`/`new Error()`/… instance (P5.3). Distinct from
    /// `local_classes` (user classes): these dispatch through the global-class
    /// metadata table, not the user `ClassTable`.
    pub global_instance_classes: HashMap<String, String>,
    /// Interns object-literal shapes (compile-time key→slot maps) for this fn.
    pub shapes: ShapeTable,
    /// The function's return repr, or `None` for a `void` body (`__rtsn_main`).
    pub ret: Option<Repr>,
    /// True once the current block has emitted a terminator.
    pub block_terminated: bool,
    /// Active enclosing loops (innermost last) for `break`/`continue` (P5.10). Each
    /// entry names the block `break` jumps to (the loop exit) and the block
    /// `continue` jumps to (the header for a `while`, the update step for a C-`for`,
    /// the per-iteration advance for for-of/for-in). A `break`/`continue` with no
    /// active loop, or with a label, BAILS.
    pub loop_stack: Vec<LoopCtx>,
    /// A pending source LABEL (`outer:`) set by [`Self::lower_labeled`] just before
    /// lowering the labeled statement; the next loop/switch lowering `take`s it into
    /// its [`LoopCtx`] so a `break`/`continue label` can target it. Cleared if the
    /// labeled statement is not a loop/switch (a labeled plain block).
    pub pending_label: Option<String>,
    /// Active enclosing `try` blocks (innermost last) for the manual-unwind
    /// exception model (P5.13). Each entry is the Cranelift block a pending error
    /// routes to — the innermost `catch` handler (or, for a `try`/`finally` with no
    /// `catch`, the `finally` block). A `throw` or a post-call error-check while
    /// this stack is non-empty branches to `.last()`; while empty it RETURNS a
    /// sentinel from the current function (propagation to the caller).
    pub try_stack: Vec<TryCtx>,
    /// Active enclosing `finally` BODIES (innermost last) for NON-error exits. A
    /// `return` inside a `try`/`finally` must run every enclosing finalizer (in
    /// JS order: value-expr first, THEN finalizers innermost-first) before the
    /// real `return`. The error path already routes through the finally block via
    /// `try_stack`; this carries the finalizer STATEMENTS so `lower_return` can
    /// inline them on the return edge. Pushed for the body+catch of a
    /// try-with-finally, popped before the finalizer itself lowers (so a `return`
    /// inside a finalizer only runs the OUTER finalizers).
    pub finally_stack: Vec<Vec<HirStmt>>,
    /// Every user function's frozen ABI signature, for cross-fn calls. Keyed by
    /// name; shared (read-only) across all functions in the module.
    pub sigs: &'c HashMap<String, FnSig>,
    /// Each user function's uniform-ABI THUNK FuncId, for reifying a function
    /// referenced as a VALUE (`func_addr` of the thunk → `__rtsadp_fn_reify`).
    pub thunks: &'c HashMap<String, cranelift_module::FuncId>,
    /// Each user function's REAL (native-signature) FuncId — for taking the bare
    /// code address of a function whose native ABI matters (the generator
    /// state-fn passed to `__RTS_GEN_SM_NEW`, called by the runtime as
    /// `extern "C" fn(u64)->i64`). Distinct from `thunks` (uniform-ABI wrappers).
    pub ids: &'c HashMap<String, cranelift_module::FuncId>,
    /// synthesized CLOSURE fn name → ordered captured outer-local names (P5.7).
    /// When reifying such a name as a function VALUE, the lowerer snapshots each
    /// captured local's current value into a fresh env array (`__rtsadp_fn_reify`
    /// stores the env on the function entry).
    pub captures: &'c HashMap<String, Vec<String>>,
    /// MODULE-LEVEL MUTABLE GLOBALS (epic #195): top-level `let` name → cell id. A
    /// name in this map resolves (read/write) through `__RTS_FN_NS_GC_GCELL_GET/SET`
    /// by its id, NOT a Cranelift Variable — checked AFTER `self.local` so a
    /// same-named param/local in the current function still shadows it.
    pub gcells: &'c HashMap<String, u32>,
    /// The gcell ids that are NEVER written after their top-level initializer
    /// (`funcval::immutable_gcells`) — a module `const` promoted to a cell only
    /// because some function READS it. Their value is fixed once `__rtsn_main`
    /// has run, so this function may load each ONCE and reuse it
    /// ([`Self::gcell_cache`]) instead of an extern `GCELL_GET` per access.
    pub immutable_gcells: &'c std::collections::HashSet<u32>,
    /// Per-function memo of already-loaded IMMUTABLE gcells: id → a Cranelift
    /// VARIABLE holding the loaded word, defined in the entry block.
    ///
    /// A `Variable` (not the raw call `Value`): the load is an effectful `call`,
    /// which lives in the egraph's skeleton, and reusing its result directly
    /// across blocks made Cranelift panic — "something has gone very wrong if we
    /// are elaborating effectful instructions". Going through the builder's SSA
    /// construction (`def_var` once at entry, `use_var` at each read) is the
    /// supported way to make a value live across blocks; the builder inserts
    /// whatever block params it needs.
    pub gcell_cache: HashMap<u32, cranelift_frontend::Variable>,
    /// Per-function memoization of `module.declare_func_in_func(fid, …)`. Each
    /// call to that helper imports a FRESH `SigRef` + `FuncRef` into the current
    /// `ir::Function` — it does NOT dedup — so N call sites of the same callee
    /// emitted N redundant `sigK =`/`fnK =` preamble entries (measured: 1947
    /// decls for 229 distinct targets on a one-line `console.log`). A `FuncRef`
    /// is reusable across every `call` in the same function, so cache it by
    /// `FuncId` and reuse. Reset per function (fresh `Lowerer`), which is exactly
    /// the FuncRef validity scope. Use via [`Self::func_ref`].
    pub func_ref_cache: HashMap<cranelift_module::FuncId, cranelift_codegen::ir::FuncRef>,
    /// A proven typed-array VIEW local (`const a = new Uint8Array(ab)`) → the
    /// `(base_ptr, elem_count)` Variables computed ONCE at its declaration (step
    /// 5b, hoisting). Each `a[i]` reads them with `use_var` instead of calling
    /// `__rtsadp_ta_view_base_len` (which locks the buffer shard) per access. The
    /// computation sits at the ctor site, which dominates every access, so SSA
    /// construction is sound. Sound to cache because the view's buffer is never
    /// resized (no resizable ArrayBuffer) and a reassignment of the local drops
    /// its `TypedArrayView` shape (accesses then go dynamic, bypassing this).
    pub ta_view_base_len: HashMap<String, (cranelift_frontend::Variable, cranelift_frontend::Variable)>,
    /// Top-level SINGLETON-INSTANCE globals (`const X = new Y()`) → class `Y`. Lets
    /// `static_instance_class` recover the receiver class of a gcell-promoted
    /// singleton in any scope (the gcell erases the `local_classes` entry).
    pub gcell_classes: &'c HashMap<String, String>,
    /// `globalThis.<key>` → the user CLASS statically held there, when EVERY
    /// assignment `globalThis.<key> = <Class>` in the whole program agrees on one
    /// known class (the all-agree pre-pass `infer_globalthis_classes`). Lets
    /// `const G = globalThis.X` record `G` as a class reference so `new G(args)`
    /// constructs on the static path and the result is method-dispatchable.
    /// Program-wide read-only (shared across all function lowerings).
    pub globalthis_class_refs: &'c HashMap<String, String>,
    /// The program's user classes (descriptors: fields → shape slots, methods →
    /// functions). Read-only, shared across all functions. Drives `new C(args)`,
    /// `this.field`, and static `instance.method(args)`.
    pub classes: &'c super::class::ClassTable,
    /// PRIVACY GATE: whether the function being lowered came from the engine's
    /// PRELUDE (embedded TS includes). Only a prelude-origin function may name the
    /// PRIVATE `engine.*` ambient global; in a user function (incl. `__rtsn_main`)
    /// any `engine.*` reference bails explicitly (see [`super::engineobj`]).
    pub is_prelude: bool,
    /// Whether THIS function is the synthesized `__rtsn_main`. A module-level
    /// `let` in main initializes its promoted GCELL; the same-spelled `let`
    /// inside ANY function declares a fresh LOCAL that shadows the global
    /// (without this a prelude-internal `let out` clobbered a user gcell).
    pub is_main: bool,
    /// The NAME of the function being lowered (`__rtsn_main`, `__rtsn_ctor_C`,
    /// `__rtsn_method_C_m`, `__rtsn_static_C_m`, …). Drives the TS access-surface
    /// re-checks: a `readonly` field write is allowed only inside a
    /// `__rtsn_ctor_*`, and a static method's enclosing class is recovered from
    /// the `__rtsn_static_<C>_` prefix (see `enclosing_class`).
    pub current_fn: String,
    /// BUILTIN-IMPORT bindings (M1b): a LOCAL name imported from `rts:<ns>` →
    /// `(ns, member)`. A `name(args)` call whose `name` is here lowers to the real
    /// `__RTS_FN_NS_*` symbol via the generic Registry marshal (see [`super::call`]).
    /// Shared (read-only) across all functions; empty for a single-source program.
    pub builtins: &'c HashMap<String, (String, String)>,
    /// Locals that ACCUMULATE a heap-read number (`s = s + p.age`) — pre-scanned
    /// per function (`floatscan`); an `Int*`-proven `let` of such a name binds
    /// `Float64` instead so the accumulation never truncates the fraction.
    pub float_promoted: std::collections::HashSet<String>,
    /// Singleton-instance METHOD OVERRIDES seen so far in THIS function
    /// (`console.table = fn`): `(gcell name, prop)`. A later `name.prop(..)`
    /// call in the same function dispatches DYNAMICALLY (own-prop fn word if
    /// set, else the class method). Per-function by design — the test corpus
    /// overrides and calls at the top level (one `__rtsn_main` pass).
    pub singleton_overrides: std::collections::HashSet<(String, String)>,
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Import `fid` as a `FuncRef` in the current function, memoized by `FuncId`
    /// ([`Self::func_ref_cache`]). Cranelift's `declare_func_in_func` imports a
    /// fresh `SigRef` + `FuncRef` on every call (no dedup); routing the hot
    /// per-call-site declaration sites through here collapses the redundant
    /// preamble to one entry per distinct callee. The first import of each callee
    /// goes through [`value::emit_marshal::declare_func_dedup`], which also reuses
    /// an existing `SigRef` of equal signature (so the `sigK =` preamble collapses
    /// to the few distinct shapes, not one per callee).
    pub fn func_ref(
        &mut self,
        module: &mut dyn Module,
        fid: cranelift_module::FuncId,
    ) -> cranelift_codegen::ir::FuncRef {
        if let Some(&fref) = self.func_ref_cache.get(&fid) {
            return fref;
        }
        let fref = value::emit_marshal::declare_func_dedup(module, self.builder.func, fid);
        self.func_ref_cache.insert(fid, fref);
        fref
    }

    /// Lower one user function (or the synthesized main) into `builder`, whose
    /// `Function` already carries the signature from [`FnSig::to_cranelift`].
    #[allow(clippy::too_many_arguments)]
    pub fn lower_function(
        module: &mut dyn Module,
        builder: &'a mut FunctionBuilder<'b>,
        func: &HirFunc,
        sig: &FnSig,
        sigs: &'c HashMap<String, FnSig>,
        thunks: &'c HashMap<String, cranelift_module::FuncId>,
        ids: &'c HashMap<String, cranelift_module::FuncId>,
        classes: &'c super::class::ClassTable,
        captures: &'c HashMap<String, Vec<String>>,
        gcells: &'c HashMap<String, u32>,
        immutable_gcells: &'c std::collections::HashSet<u32>,
        gcell_classes: &'c HashMap<String, String>,
        globalthis_class_refs: &'c HashMap<String, String>,
        this_class: Option<&str>,
        is_prelude: bool,
        builtins: &'c HashMap<String, (String, String)>,
        cell_locals: std::collections::HashSet<String>,
        param_classes: HashMap<String, String>,
    ) -> FrontResult<()> {
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut ctx = Lowerer {
            builder,
            locals: HashMap::new(),
            local_shapes: HashMap::new(),
            object_locals: std::collections::HashSet::new(),
            string_locals: std::collections::HashSet::new(),
            cell_locals,
            hoisted_cells: std::collections::HashSet::new(),
            local_classes: HashMap::new(),
            local_class_refs: HashMap::new(),
            generator_locals: HashMap::new(),
            global_instance_classes: HashMap::new(),
            shapes: ShapeTable::new(),
            ret: sig.ret,
            block_terminated: false,
            loop_stack: Vec::new(),
            pending_label: None,
            try_stack: Vec::new(),
            finally_stack: Vec::new(),
            sigs,
            thunks,
            ids,
            captures,
            gcells,
            immutable_gcells,
            gcell_cache: HashMap::new(),
            func_ref_cache: HashMap::new(),
            ta_view_base_len: HashMap::new(),
            gcell_classes,
            globalthis_class_refs,
            classes,
            is_prelude,
            is_main: func.name == "__rtsn_main",
            current_fn: func.name.clone(),
            builtins,
            float_promoted: super::floatscan::float_promoted_locals(&func.body),
            singleton_overrides: std::collections::HashSet::new(),
        };

        // Bind each parameter to a fresh local Variable carrying its ABI repr.
        for (i, (p, &repr)) in func.params.iter().zip(&sig.params).enumerate() {
            let block_val = ctx.builder.block_params(entry)[i];
            let var = ctx.builder.declare_var(cl_type(repr));
            ctx.builder.def_var(var, block_val);
            ctx.locals.insert(p.name.clone(), Local { var, repr });
            // An object-typed param (`o: {x: number}`) is a PROVEN keyed OBJECT
            // whose exact compile-time shape is not known to the callee — route
            // `o.key` / `o[k]` through the DYNAMIC property access (P5.5) instead of
            // bailing on an unknown shape. (P5.6: rts-hir now distinguishes the
            // anonymous-object type `HirType::Object` from `any`/`Unknown`.)
            if matches!(p.ty, rts_hir::HirType::Object) {
                ctx.object_locals.insert(p.name.clone());
            }
            // An array-typed param (`xs: number[]`) is a PROVEN array. Record its
            // heap shape so `xs[i]` / `xs.length` / `xs.method(..)` lower through
            // the array access path (F1). The param keeps its Tagged ABI/repr (the
            // i64 register holds the boxed array word); only the shape is recorded.
            if matches!(p.ty, rts_hir::HirType::Array(_)) {
                ctx.local_shapes.insert(p.name.clone(), HeapShape::Array);
            }
            // A string-typed param (`sub: string`) is a PROVEN native string.
            // Record it so `sub[j]` / `sub.length` / `sub.charCodeAt(i)` take the
            // native string fast-path (`Self::receiver_is_proven_string`). The param
            // keeps its Tagged ABI/repr (the i64 register holds the boxed string
            // word); only the proven-string fact is recorded.
            if matches!(p.ty, rts_hir::HirType::Str) {
                ctx.string_locals.insert(p.name.clone());
            }
            // A USER-CLASS-typed param (`a: Animal`, recovered from the AST since
            // rts-hir drops a bare class-name annotation to `Unknown`). Record its
            // static class so `a.field` resolves against the class shape and
            // `a.method(..)` dispatches — STATIC when monomorphic, VIRTUAL (by the
            // instance's runtime shape) when the method is overridden in the
            // subtree. The param keeps its Tagged ABI/repr (the instance word).
            if let Some(class) = param_classes.get(&p.name) {
                if let Some(desc) = classes.get(class) {
                    let shape_id = ctx.shapes.intern(&desc.fields);
                    ctx.local_shapes
                        .insert(p.name.clone(), HeapShape::Object(shape_id));
                    ctx.local_classes.insert(p.name.clone(), class.clone());
                }
            }
        }

        // The implicit receiver `this` of a constructor/method: bind its class so
        // `this.field` resolves against the class shape and `this.method()`
        // dispatches statically.
        if let Some(class) = this_class {
            if let Some(desc) = classes.get(class) {
                let shape_id = ctx.shapes.intern(&desc.fields);
                ctx.local_shapes
                    .insert(super::class::THIS.to_string(), HeapShape::Object(shape_id));
                ctx.local_classes
                    .insert(super::class::THIS.to_string(), class.to_string());
            }
        }

        // DEFAULT-PARAM PROLOGUE (callee-side): for each user param with a default
        // initializer, replace an `undefined` argument with the default. Runs in the
        // CALLEE scope (earlier params are already bound — correct JS semantics) and
        // for plain functions, methods, AND constructors (all reach here).
        ctx.fill_default_params(module, func)?;

        // IMMUTABLE-GCELL PROLOGUE: load, ONCE and here in the entry block, every
        // never-written module global this body reads. Emitting them here (rather
        // than memoizing at first use) is what makes the memo dominance-safe — a
        // value defined inside a loop could not be reused after it.
        //
        // Without this, a module `const` read in a hot loop cost an extern
        // `GCELL_GET` per ITERATION, and every thread hammered the one shared cell:
        // measured 2.2x slower than the identical loop over a local, and parallel
        // scaling went NEGATIVE (8 workers slower than 1). See
        // `funcval::immutable_gcells`.
        if !ctx.immutable_gcells.is_empty() {
            let mut bound: std::collections::HashSet<String> =
                func.params.iter().map(|p| p.name.clone()).collect();
            bound.extend(super::funcval::scan::declared_names(&func.body));
            let mut reads: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut scope = bound;
            for s in &func.body {
                super::funcval::scan::collect_free_stmt(s, &mut scope, &mut reads);
            }
            // Deterministic order: the emitted IR must not depend on HashSet
            // iteration order (that non-determinism produced broken AOT objects).
            let mut preload: Vec<(String, u32)> = gcells
                .iter()
                .filter(|(name, id)| ctx.immutable_gcells.contains(*id) && reads.contains(*name))
                .map(|(n, i)| (n.clone(), *i))
                .collect();
            preload.sort();
            for (_, id) in preload {
                let _ = ctx.emit_gcell_get(module, id)?;
            }
        }

        // CELL-HOISTING prologue: allocate up front the cell of every
        // FN-VALUED cell-local of THIS function (bound to `undefined`), so a
        // closure reified BEFORE the declaring `const` runs (a
        // mutually-recursive nested fn's FORWARD reference) snapshots a real
        // cell handle — the decl later SETs the live fn into the same box.
        // Scoped to fn-valued decls only: a plain loop-body `let` cell keeps
        // its alloc-at-decl (fresh box per pass, the per-iteration semantics).
        {
            let hoist: Vec<String> = super::funcval::fn_decl_names(&func.body)
                .intersection(&ctx.cell_locals)
                .cloned()
                .collect();
            for name in hoist {
                if ctx.local(&name).is_some() {
                    continue; // a param of the same name owns the slot
                }
                let undef = ctx
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                let handle = ctx.emit_cell_alloc(module, undef);
                ctx.bind_tagged_local(&name, Val::new(handle, Repr::Tagged));
                ctx.hoisted_cells.insert(name);
            }
        }

        // METHOD-TABLE prologue (main only): register every class method's
        // (instance shape-id, name) → thunk addr in the runtime table, so the
        // dynamic property read reifies a class method as a VALUE (obj_get miss
        // → `__rtsadp_method_table_add` counterpart in rts-adapters).
        if ctx.is_main {
            ctx.emit_method_table_registrations(module)?;
        }

        ctx.lower_block(module, &func.body)?;

        // Fall-through at the end of the body. JS: a function with no `return`
        // returns `undefined`. A `void`/untyped function carries a `Tagged` return
        // repr, so emit the implicit `return undefined`. A function PROVEN to return
        // an UNBOXED numeric/bool that can fall through is genuinely ill-formed (an
        // undefined can't inhabit that slot) → keep the explicit bail.
        if !ctx.block_terminated {
            match ctx.ret {
                None => {
                    ctx.builder.ins().return_(&[]);
                }
                Some(Repr::Tagged) => {
                    let undef = ctx
                        .builder
                        .ins()
                        .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                    ctx.builder.ins().return_(&[undef]);
                }
                Some(repr) => {
                    // The fall-through here is genuinely UNREACHABLE: the function
                    // returns on every real path (e.g. through every try/catch/finally
                    // arm, or a conditional `throw`), but the linear `block_terminated`
                    // analysis cannot prove it across those edges. Emit a typed ZERO
                    // return so the block carries a terminator; this block has no live
                    // predecessors, so Cranelift DCEs it. A zero filler (never a crash)
                    // replaces the former whole-program bail — the proven return slot
                    // can hold it, and it is never actually returned. (A genuinely
                    // ill-formed fall-through of a numeric fn would return 0, not
                    // undefined — a rare TS type error, sound vs a wrong value.)
                    let zero = if matches!(repr, Repr::Float64) {
                        ctx.builder.ins().f64const(0.0)
                    } else {
                        ctx.builder.ins().iconst(cl_type(repr), 0)
                    };
                    ctx.builder.ins().return_(&[zero]);
                }
            }
        }
        Ok(())
    }

    /// Emit the default-param prologue: for each user param with a DEFAULT
    /// initializer, replace an `undefined` value with the lowered default. JS fires a
    /// default ONLY on `undefined` (NOT `null`), so we test the param word against the
    /// `undefined` singleton only. Uses the same block structure as
    /// [`Self::lower_nullish_coalesce`]: a guarded merge where the param Variable is
    /// `def_var`'d to the default on the undefined branch and kept otherwise; both
    /// paths jump to a fresh open block so body lowering resumes normally.
    ///
    /// The default is lowered in the CALLEE scope (earlier params bound), coerced to
    /// the param's repr (`Tagged` for any fillable param), and stored back into the
    /// param's Variable, so the body reads the resolved value on every path.
    fn fill_default_params(&mut self, module: &mut dyn Module, func: &HirFunc) -> FrontResult<()> {
        for p in &func.params {
            let default = match &p.default_expr {
                Some(e) => e,
                None => continue,
            };
            let local = match self.local(&p.name) {
                Some(l) => l,
                None => continue,
            };
            // Current param value as a PolyValue word (a fillable param is Tagged, so
            // this is already the boxed word — `box_value` is a no-op on Tagged).
            let cur = self.builder.use_var(local.var);
            let cur_word = self.box_value(Val::new(cur, local.repr));

            let undef_w = self
                .builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
            let is_undef = self.builder.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                cur_word,
                undef_w,
            );

            let fill_blk = self.builder.create_block();
            let join_blk = self.builder.create_block();
            self.builder
                .ins()
                .brif(is_undef, fill_blk, &[], join_blk, &[]);

            // undefined → lower the default, coerce to the param repr, store it.
            self.builder.switch_to_block(fill_blk);
            self.builder.seal_block(fill_blk);
            let dv = self.lower_expr(module, default)?;
            let coerced = self.coerce(dv, local.repr)?;
            self.builder.def_var(local.var, coerced);
            // The default lowering may itself branch (e.g. `??` in the initializer);
            // only jump from the CURRENT block if it is still open.
            if !self.block_terminated {
                self.builder.ins().jump(join_blk, &[]);
            }
            self.block_terminated = false;

            // present → keep the param value (already in the Variable); continue.
            self.builder.switch_to_block(join_blk);
            self.builder.seal_block(join_blk);
        }
        Ok(())
    }

    /// Lower a statement block (control flow is statement-driven; this does not
    /// open a Cranelift block). Stops once the block is terminated.
    pub(super) fn lower_block(
        &mut self,
        module: &mut dyn Module,
        stmts: &[HirStmt],
    ) -> FrontResult<()> {
        for s in stmts {
            if self.block_terminated {
                // The block already emitted a terminator (`return`/`throw`/`break`/
                // `continue`). Remaining statements are genuinely UNREACHABLE in JS
                // (dead code after `throw e; ...` / `return x; ...` is legal source);
                // skipping them is sound — they can never run. Emitting them would
                // append instructions to an already-terminated Cranelift block.
                break;
            }
            self.lower_stmt(module, s)?;
        }
        Ok(())
    }

    /// Look up a local binding by name.
    pub(super) fn local(&self, name: &str) -> Option<Local> {
        self.locals.get(name).copied()
    }

    // ---- coercions ----

    /// Coerce `val` to `target`, inserting box/unbox/widening as needed. The
    /// legal coercions:
    /// - numeric widening `Int* → Float64` (`fcvt_from_sint`);
    /// - the `Int32`/`Int64` relabel (same i64 register);
    /// - native → `Tagged` (BOX, pure IR);
    /// - `Tagged` → native number (UNBOX, pure IR) — used at a numeric call
    ///   boundary; the program lowering only does this when the target is proven
    ///   numeric.
    pub(super) fn coerce(&mut self, val: Val, target: Repr) -> FrontResult<Value> {
        if val.repr == target {
            return Ok(val.v);
        }
        match (val.repr, target) {
            // numeric widening to double
            (Repr::Int32, Repr::Float64) | (Repr::Int64, Repr::Float64) => {
                Ok(self.builder.ins().fcvt_from_sint(types::F64, val.v))
            }
            // int relabel (same register)
            (Repr::Int32, Repr::Int64) | (Repr::Int64, Repr::Int32) => Ok(val.v),
            // Bool ToNumber: a Bool is already an i64 0/1, so → Int is a pure relabel
            // (same register) and → Float64 is a signed-int convert. (`true + 1`,
            // `-true`, `~true`, `true * 2.0`.)
            (Repr::Bool, Repr::Int32) | (Repr::Bool, Repr::Int64) => Ok(val.v),
            (Repr::Bool, Repr::Float64) => Ok(self.builder.ins().fcvt_from_sint(types::F64, val.v)),
            // native double → native int: truncate toward zero (JS `ToInteger` /
            // array-index / `i64`-typed binding from a `number` expression). Uses the
            // saturating conversion so an out-of-range / NaN double yields a defined
            // value instead of a Cranelift trap (the proven-numeric path is in range;
            // saturation only guards the edge). Both Int32/Int64 ride the i64 register
            // here (the int relabel above shows they share it), so one cvt serves both.
            (Repr::Float64, Repr::Int32) | (Repr::Float64, Repr::Int64) => {
                Ok(self.builder.ins().fcvt_to_sint_sat(types::I64, val.v))
            }
            // native → Tagged (box)
            (_, Repr::Tagged) => Ok(self.box_value(val)),
            // Tagged → native double: SOUND decode that accepts EITHER number
            // representation (a tagged int32 OR an inline double). A boxed double
            // (e.g. the `__rtsadp_add` result of `0 + arr[i]`) must not be read by
            // the int32-only unbox, and vice-versa — `emit_tagged_number_to_f64`
            // selects on the tag (pure IR the egraph folds when the repr is proven).
            (Repr::Tagged, Repr::Float64) => {
                Ok(value::emit_tagged_number_to_f64(self.builder, val.v))
            }
            // Tagged → native int: decode to a double (handling both number tags),
            // then truncate toward zero (JS `ToInt`/array-index/`| 0`-style). This
            // replaces the old `emit_unbox_int32`, which silently read 0 from a
            // boxed-double word (e.g. `let s = 0; s = s + arr[i]`).
            //
            // SATURATING (`fcvt_to_sint_sat`, like the Float64→Int arm above), NOT
            // the trapping `fcvt_to_sint`: a Tagged word that is NOT a number — an
            // object/string/undefined passed where an int is wanted (e.g. a heap
            // object handed to a `U64`-param builtin like `collections.map_get`) —
            // decodes to a NaN/out-of-range double, and the trapping convert emits a
            // Cranelift `trap` (an `ud2` → SIGILL). Saturation yields a DEFINED value
            // (0 for NaN) instead — never a crash. (Was the root cause of the
            // heap-value → `collections.*(U64)` SIGILL.)
            (Repr::Tagged, Repr::Int32) | (Repr::Tagged, Repr::Int64) => {
                let f = value::emit_tagged_number_to_f64(self.builder, val.v);
                Ok(self.builder.ins().fcvt_to_sint_sat(types::I64, f))
            }
            (from, to) => unsupported!("cannot coerce {from:?} to {to:?}"),
        }
    }

    /// BOX an unboxed value into a `Tagged` PolyValue word (pure IR). A `Bool`
    /// (i64 0/1) becomes the `false`/`true` singleton via `select`.
    pub(super) fn box_value(&mut self, val: Val) -> Value {
        match val.repr {
            Repr::Int32 => {
                let i32v = self.builder.ins().ireduce(types::I32, val.v);
                value::emit_box_int32(self.builder, i32v)
            }
            Repr::Int64 => {
                // A 64-bit int that must become Tagged boxes as a double (the
                // tagged int payload is only 48-bit; an i64 in JS-number range
                // round-trips exactly through f64 for the magnitudes here).
                let f = self.builder.ins().fcvt_from_sint(types::F64, val.v);
                value::emit_box_double(self.builder, f)
            }
            Repr::Float64 => value::emit_box_double(self.builder, val.v),
            Repr::Bool => {
                // false → SINGLETON_FALSE word, true → SINGLETON_TRUE word.
                let f_word = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::bool(false).raw() as i64);
                let t_word = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
                self.builder.ins().select(val.v, t_word, f_word)
            }
            Repr::Tagged => val.v,
            other => {
                // Ref kinds are not produced by the current program subset.
                unreachable!("box_value on unexpected repr {other:?}")
            }
        }
    }

    // ---- extern calls (runtime ops) ----

    /// Declare-import a runtime symbol by name and emit the call, with each call's
    /// Cranelift signature derived EXACTLY from the real-symbol descriptor
    /// ([`crate::value::abi_sig`]) so `StrPtr` splits into ptr+len and f64/i64
    /// slots are right (mis-marshaling → SIGILL). `args` are the already-marshaled
    /// Cranelift values, one per Cranelift slot. Returns the result for returning
    /// symbols, or `None` for void ones.
    ///
    /// This serves the codegen-owned `__rtsadp_*` generic operators (all `U64`
    /// slots — PolyValue words in/out); the StrPtr-bearing real symbols are called
    /// through the dedicated [`crate::value::emit_marshal`] helpers.
    pub(super) fn call_runtime(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[Value],
    ) -> FrontResult<Option<Value>> {
        let sig = crate::value::abi_sig::sig_of(name)
            .ok_or_else(|| Unsupported::new(format!("unknown runtime symbol `{name}`")))?;
        if sig.param_slot_count() != args.len() {
            return unsupported!(
                "runtime symbol `{name}` expects {} slots but {} args",
                sig.param_slot_count(),
                args.len()
            );
        }
        Ok(crate::value::emit_marshal::emit_call(
            module,
            self.builder,
            name,
            args,
        ))
    }
}
