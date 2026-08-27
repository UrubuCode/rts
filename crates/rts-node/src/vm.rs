//! `node:vm` — compile-and-run source text, against `entry::evaluate`'s
//! **fresh, disconnected** program rather than V8's isolate/context model.
//!
//! # Reuse-check
//!
//! Per `.claude/skills/reuse-check`: `rts-cranelift` owns nothing
//! vm/context-shaped (it is the machine, not the language). The one thing
//! this module is built on already exists and is not reproduced here:
//! `rts_core::entry::evaluate`, installed by the host
//! (`rts-host`'s `run.rs`, `evaluate_source`) as the one seam that turns
//! source text into a value. `entry::declare_evaluator`/`entry::evaluate`
//! are the whole of it — nothing about compiling, placing or running a
//! program is re-derived here.
//!
//! # THE limit, read before anything below
//!
//! `entry::evaluate(source)` compiles and runs `source` in a **brand-new
//! program**, with its own region, its own key registry, its own literal
//! table. It shares **nothing** with the caller: no variable the caller has
//! in scope is visible to the evaluated source, and no declaration the
//! evaluated source makes is visible back. Only a value that needs no
//! region can cross — a number, a boolean, a singleton — and `evaluate`
//! answers `None` for anything else (an object, including a function),
//! **and also** for a source that failed to compile. The two are one
//! answer on purpose (see `evaluate`'s own doc) and the `run` helper below
//! inherits that limitation.
//!
//! The context-aware VM methods use `evaluate_in_scope_with_receiver`
//! instead. They compile a fresh program against an environment in the
//! running region, so completion objects such as an `ArrayBuffer` remain
//! usable by the caller. This still does not create a separate intrinsic
//! realm: the scope is isolated by its environment object, not by a second
//! heap or global implementation.
//!
//! # `Context` objects: a scope with explicit limits
//!
//! `vm.createContext(obj)` marks and returns `obj`. Code run through
//! `runInContext` or `Script.runInContext` uses that same object for free-name
//! lookup and as `this`, so property reads and writes stay in the running
//! region. The evaluator does not implement every V8 context feature: it does
//! not create a separate intrinsic realm, and completion values that require
//! unsupported syntax still follow the host's normal failure path.
//!
//! # Not implemented, by name
//!
//! - **Separate V8 realms for `vm.runInThisContext` and `vm.runInContext`** —
//!   these methods use the active runtime region and preserve object identity,
//!   but they do not create a separate intrinsic realm. V8-only realm controls
//!   remain outside the scope of this implementation.
//! - **`vm.Module` / `vm.SourceTextModule` / `vm.SyntheticModule`** — the
//!   whole family needs a *dynamic*, user-linked module record; this
//!   engine resolves imports statically at compile time
//!   (`crates/rts-node/src/vm.rs`'s own compile path is `entry::evaluate`,
//!   which has no module-record concept at all). Refused rather than faked
//!   with a record nothing can actually link.
//! - **`vm.measureMemory`** — no heap-measurement entry point crosses the
//!   `entry` boundary; refused rather than answering a fabricated number.
//! - **`Script.createCachedData` / `cachedData` / `cachedDataRejected`** —
//!   this engine has no bytecode cache; there is nothing to serialize.
//! - **`timeout` / `breakOnSigint` / `microtaskMode`** on every run option
//!   object — `entry::evaluate` runs synchronously to completion with no
//!   interrupt hook; the options are accepted (so a caller's object literal
//!   is not a type error) and ignored.
//! - **`vm.constants.USE_MAIN_CONTEXT_DEFAULT_LOADER` / `importModuleDynamically`**
//!   — dynamic `import()` inside evaluated source has nothing to resolve
//!   against; not offered.
//! - **`vm.constants.DONT_CONTEXTIFY`** — this implementation always uses the
//!   ordinary marked context object, so the V8 distinction this constant draws
//!   is omitted from `vm.constants`.

use rts_core::entry::{self, Context, Provided};

/// The instance methods `Script` carries.
const SCRIPT_METHODS: &[(&str, Provided)] = &[
    ("runInNewContext", script_run_new_context),
    ("runInContext", script_run_context),
    ("runInThisContext", script_run_this_context),
];

/// The namespace `node:vm` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("runInNewContext", run_in_new_context),
        ("runInContext", run_in_context),
        ("runInThisContext", run_in_this_context),
        ("createContext", create_context),
        ("isContext", is_context),
        ("compileFunction", compile_function),
    ];
    let namespace = entry::make_namespace(context, members);
    let prototype = entry::make_prototype(context, "Script", SCRIPT_METHODS);
    let constructor = entry::make_callable(context, script_ctor);
    entry::put_member(context, constructor, "prototype", prototype);
    entry::put_member(context, namespace, "Script", constructor);
    namespace
}

/// `vm.runInNewContext(code[, contextObject[, options]])`.
///
/// When a context object is supplied, its properties are used as the evaluated
/// program's environment and as its `this` value. Options remain accepted but
/// unsupported execution controls are ignored, as documented above.
extern "C" fn run_in_new_context(_e: u64, _this: u64, code: u64, context_obj: u64, _options: u64, _d: u64) -> u64 {
    run_with_context(code, context_obj)
}

/// `vm.runInContext(code, contextifiedObject[, options])`.
///
/// The context object supplies both the evaluated program's environment and
/// its `this` value; unsupported V8 realm features remain outside the scope of
/// this implementation.
extern "C" fn run_in_context(_e: u64, _this: u64, code: u64, context_obj: u64, _options: u64, _d: u64) -> u64 {
    run_with_context(code, context_obj)
}

/// `vm.runInThisContext(code[, options])`.
///
/// The current global object supplies both the evaluated program's environment
/// and its `this` value. It is still compiled through the host's live evaluator,
/// rather than through V8's separate realm machinery.
extern "C" fn run_in_this_context(_e: u64, _this: u64, code: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let global = entry::with_runtime(|context| entry::global_object(context));
    run_in_context_object(code, global)
}

/// `new vm.Script(code[, options])`.
///
/// Stores `code` on the instance rather than compiling now: real Node
/// compiles at construction and runs later, and a caller checking
/// `script.runInNewContext !== undefined` before ever running it should not
/// pay for (or fail on) a compile it has not asked for yet.
extern "C" fn script_ctor(_e: u64, this: u64, code: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        // Idempotent by name (see `entry::make_prototype`'s own doc) — this
        // answers the SAME prototype `namespace` already registered rather
        // than building a second one.
        let prototype = entry::make_prototype(context, "Script", SCRIPT_METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, instance, "__code__", code);
        instance
    })
}

/// `script.runInNewContext([contextObject[, options]])`.
extern "C" fn script_run_new_context(_e: u64, this: u64, context_obj: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let code = entry::with_runtime(|context| entry::get_member(context, this, "__code__"));
    run_with_context(code, context_obj)
}

/// `script.runInContext(contextifiedObject[, options])`.
extern "C" fn script_run_context(_e: u64, this: u64, context_obj: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let code = entry::with_runtime(|context| entry::get_member(context, this, "__code__"));
    run_in_context_object(code, context_obj)
}

/// `script.runInThisContext([options])`.
extern "C" fn script_run_this_context(_e: u64, this: u64, _options: u64, _c: u64, _d: u64, _e2: u64) -> u64 {
    let code = entry::with_runtime(|context| entry::get_member(context, this, "__code__"));
    let global = entry::with_runtime(|context| entry::global_object(context));
    run_in_context_object(code, global)
}

/// Compiles and runs `code`, answering `entry::evaluate`'s value or
/// `undefined` when nothing could cross — see the module doc.
fn run(code: u64) -> u64 {
    let Some(source) = entry::text_of(code) else {
        return entry::undefined_value();
    };
    match entry::evaluate(&source) {
        Some(value) => value,
        None => entry::undefined_value(),
    }
}

/// Evaluates against a caller-supplied context, creating an empty one when the
/// optional context was omitted. The empty object keeps completion values in the
/// running region, which is required for a returned `ArrayBuffer` to remain a
/// usable `BufferSource` even though this engine has no separate intrinsic realm.
fn run_with_context(code: u64, context_obj: u64) -> u64 {
    let absent = entry::undefined_value();
    let environment = if context_obj == absent {
        entry::with_runtime(|context| entry::make_object(context))
    } else {
        context_obj
    };
    run_in_context_object(code, environment)
}

/// Evaluates source text against an object that acts as the environment.
fn run_in_context_object(code: u64, environment: u64) -> u64 {
    let Some(source) = entry::text_of(code) else {
        return entry::undefined_value();
    };
    match entry::evaluate_in_scope_with_receiver(&source, environment, environment) {
        Some(value) => value,
        None => entry::undefined_value(),
    }
}

/// `vm.createContext([contextObject])` — a marker, not a scope; see the
/// module doc.
extern "C" fn create_context(_e: u64, _this: u64, context_obj: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let object = if context_obj == absent {
            entry::make_object(context)
        } else {
            context_obj
        };
        entry::put_member(context, object, "__rts_vm_context__", entry::boolean_value(true));
        object
    })
}

/// `vm.isContext(object)` — `true` only for what [`create_context`] marked.
extern "C" fn is_context(_e: u64, _this: u64, object: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let marker = entry::with_runtime(|context| entry::get_member(context, object, "__rts_vm_context__"));
    entry::boolean_value(entry::to_boolean(marker))
}

/// `vm.compileFunction(code[, params[, options]])`.
///
/// # What crosses and what does not
///
/// A genuine `Function` cannot cross `entry::evaluate`'s boundary — a
/// function value is an object, and an object belongs to the region that
/// made it (see the module doc). So this does not hand back a value the
/// evaluated program built; it hands back a **native** callable that,
/// on every call, re-runs `code` in a fresh `entry::evaluate` program,
/// wrapped as an immediately-invoked function over `params`.
///
/// # The honest limit within that: only primitive arguments bind
///
/// Binding an argument means splicing it into the wrapped source as a
/// literal — there is no other channel across this boundary. That works
/// for a number, a string, a boolean; it cannot work for an object, which
/// would need to already exist inside the fresh program's own region.
/// A non-primitive argument is passed as `undefined` rather than silently
/// dropped or made to throw with no protected region to throw into (the
/// same limit `crate::assert` documents elsewhere in this crate).
///
/// Capped at [`entry::ARGUMENT_SLOTS`] (four) parameters, which is also
/// this module's own four-argument ceiling — `params` beyond the fourth
/// are accepted (so a longer array is not a type error) and ignored.
extern "C" fn compile_function(_e: u64, _this: u64, code: u64, params: u64, _options: u64, _d: u64) -> u64 {
    let Some(body) = entry::text_of(code) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let names = param_names(context, params);
        let callable = entry::make_callable(context, invoke_compiled);
        let joined = entry::make_string(context, &names.join(","));
        entry::put_member(context, callable, "__params__", joined);
        let body_value = entry::make_string(context, &body);
        entry::put_member(context, callable, "__body__", body_value);
        callable
    })
}

/// The reader of `params` — an array-like of strings, read one element at a
/// time by numeric-string key, up to [`entry::ARGUMENT_SLOTS`].
fn param_names(context: &mut Context, params: u64) -> Vec<String> {
    let absent = entry::undefined_in(context);
    if params == absent {
        return Vec::new();
    }
    let mut names = Vec::new();
    for index in 0..entry::ARGUMENT_SLOTS {
        let element = entry::get_member(context, params, &index.to_string());
        if element == absent {
            break;
        }
        if let Some(text) = entry::text_in(context, element) {
            names.push(text);
        }
    }
    names
}

/// The callable [`compile_function`] hands back. Rebuilds `(function(p0,p1){body})(lit0,lit1)`
/// from what was stashed on itself (`this`), substituting each argument's
/// own text/number where it is a primitive and `undefined` where it is not
/// — see [`compile_function`]'s doc for why an object cannot bind.
extern "C" fn invoke_compiled(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let (params, body) = entry::with_runtime(|context| {
        (
            entry::get_member(context, this, "__params__"),
            entry::get_member(context, this, "__body__"),
        )
    });
    let Some(params) = entry::text_of(params) else {
        return entry::undefined_value();
    };
    let Some(body) = entry::text_of(body) else {
        return entry::undefined_value();
    };
    let names: Vec<&str> = params.split(',').filter(|name| !name.is_empty()).collect();
    let args = [a0, a1, a2, a3];
    let mut literals = Vec::new();
    for value in args.iter().take(names.len()) {
        literals.push(literal_of(*value));
    }
    let source = format!("(function({params}) {{ {body} }})({})", literals.join(","));
    run(entry::with_runtime(|context| entry::make_string(context, &source)))
}

/// One argument's own source-literal spelling, for [`invoke_compiled`]'s
/// splice — `"undefined"` for anything that is not a plain number, string
/// or boolean, per [`compile_function`]'s doc.
fn literal_of(value: u64) -> String {
    if let Some(number) = entry::number_of(value) {
        return number.to_string();
    }
    let is_bool = value == entry::boolean_value(true) || value == entry::boolean_value(false);
    if is_bool {
        return entry::to_boolean(value).to_string();
    }
    if let Some(text) = entry::text_of(value) {
        // A value that stringifies AND is not a number/bool is treated as
        // text — the same widening `entry::text_of` already performs for a
        // primitive. An object's `text_of` answers `None` (it would need to
        // run `toString`, which an entry point cannot do), so this branch
        // never sees one.
        return format!("{text:?}");
    }
    "undefined".to_owned()
}
