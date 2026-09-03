//! `AsyncResource`, and the three top-level reads of the scope it enters.
//!
//! # What an id here means, and what it does not
//!
//! An id names a scope THIS module entered. `executionAsyncId()` answers the
//! innermost `runInAsyncScope`'s id, and `0` when there is none — including
//! inside a promise continuation or a timer callback, where Node answers that
//! resource's own id. That gap is not closable from this crate (see [`super`]),
//! and `0` is the reference doc's own choice for "no scope" rather than a value
//! invented here.
//!
//! # Why the ids live on the instance
//!
//! A Rust-side table keyed by the instance would need to be told when the
//! instance dies, and nothing tells it. Two hidden own properties on the object
//! die with it, which is the same arrangement `AsyncLocalStorage`'s
//! `__default__` uses one module over.

use rts_core::entry::{Context, Provided};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

/// One entered resource scope.
struct Scope {
    async_id: u64,
    trigger: u64,
    resource: u64,
}

thread_local! {
    /// The scopes `runInAsyncScope` has entered and not left, innermost last.
    static SCOPES: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };

    /// The object `executionAsyncResource()` answers at top level.
    ///
    /// Made once, in [`install`], because a program compares it: two calls
    /// answering two fresh objects would make `a === b` false where Node makes
    /// it true. `0` until installed, which no member can observe since
    /// [`install`] runs while the namespace is built.
    static TOP_LEVEL: Cell<u64> = const { Cell::new(0) };

    /// `AsyncResource.prototype`, minted once by [`install`].
    ///
    /// Held for the reason written at the store: `make_prototype` decides
    /// "two modules registered different tables" by the caller's FILE, so
    /// asking for it again from here panicked. It is also the stronger
    /// invariant — one object, so every instance shares the prototype a
    /// program's `instanceof` compares against.
    static PROTOTYPE: Cell<u64> = const { Cell::new(0) };
}

/// The id counter. Process-wide: ids are compared across threads in a program
/// that passes a resource to a worker, and two per-thread counters would hand
/// out the same id for two different resources.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// What an `AsyncResource` can do. Static `bind` is attached separately in
/// [`install`] — see its own doc for why.
const METHODS: &[(&str, Provided)] = &[
    ("runInAsyncScope", run_in_async_scope),
    ("emitDestroy", emit_destroy),
    ("asyncId", async_id),
    ("triggerAsyncId", trigger_async_id_method),
    ("bind", bind),
];

/// Links the prototype and mints the top-level resource object.
pub(super) fn install(context: &mut Context, namespace: u64) {
    // Kept, and not re-made in `construct`. `make_prototype` is keyed by NAME
    // and guards against two modules registering different tables under one —
    // and it decides "two modules" by the CALLER'S FILE, so this file asking for
    // the same table `mod.rs` already registered read as a collision and
    // PANICKED. Twelve files of Node's own `async_hooks` suite died that way,
    // measured 2026-08-24, and the panic message is what named the cause.
    //
    // Holding the object is the fix rather than silencing the guard: there is
    // one prototype, minted once, and every instance is linked to that same
    // one — which is what `resource instanceof AsyncResource` asks.
    let prototype = super::attach(context, namespace, "AsyncResource", METHODS);
    PROTOTYPE.with(|held| held.set(prototype));
    let placeholder = rts_core::entry::make_object(context);
    TOP_LEVEL.with(|held| held.set(placeholder));
    // Static `AsyncResource.bind(fn, type?, thisArg?)` — attached to the
    // CONSTRUCTOR rather than listed in [`METHODS`], because it is not
    // dispatched through an instance: it MAKES one, then delegates to the
    // instance method above.
    let constructor = rts_core::entry::get_member(context, namespace, "AsyncResource");
    let static_bind_fn = rts_core::entry::make_callable(context, static_bind);
    rts_core::entry::put_member(context, constructor, "bind", static_bind_fn);
}

/// The innermost entered scope's id, or `0`.
fn current_id() -> u64 {
    SCOPES.with(|scopes| scopes.borrow().last().map_or(0, |scope| scope.async_id))
}

/// `new AsyncResource(type, options?)`.
///
/// `options` is read as Node's legacy number form (`{ triggerAsyncId: n }`
/// written as just `n`) only when it is genuinely a number and not an object —
/// asking `number_of` alone would answer for anything numeric-looking, and the
/// distinction between the two forms is exactly what decides where the trigger
/// id comes from.
pub(super) extern "C" fn construct(
    _e: u64,
    this: u64,
    type_label: u64,
    options: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let inherited = current_id();
    let (instance, label, trigger) = rts_core::entry::with_runtime(|context| {
        let prototype = PROTOTYPE.with(Cell::get);
        let instance = match rts_core::entry::is_object(context, this) {
            true => this,
            false => rts_core::entry::make_instance(context, prototype),
        };
        // `string_in`, not `text_in`: `type` is a label a hook reports, and a
        // coercion would report `"undefined"` for the one-argument call as
        // though the program had named its resource that.
        let label = rts_core::entry::string_in(context, type_label).unwrap_or_default();
        let given = match rts_core::entry::is_object(context, options) {
            true => rts_core::entry::get_member(context, options, "triggerAsyncId"),
            false => options,
        };
        let trigger = rts_core::entry::number_of(given)
            .filter(|held| *held >= 0.0)
            .map_or(inherited, |held| held as u64);
        let stored = rts_core::entry::make_number(id as f64);
        rts_core::entry::put_member(context, instance, "__asyncId__", stored);
        let stored = rts_core::entry::make_number(trigger as f64);
        rts_core::entry::put_member(context, instance, "__triggerId__", stored);
        let stored = rts_core::entry::make_string(context, &label);
        rts_core::entry::put_member(context, instance, "__type__", stored);
        (instance, label, trigger)
    });
    super::hooks::fire_init(id, &label, trigger, instance);
    instance
}

/// One of the two ids a resource carries, read back off the instance.
fn id_on(instance: u64, field: &str) -> u64 {
    rts_core::entry::with_runtime(|context| {
        let held = rts_core::entry::get_member(context, instance, field);
        rts_core::entry::number_of(held).map_or(0, |value| value as u64)
    })
}

/// Enters `resource`'s scope, fires `before`, calls `target`, fires `after`,
/// leaves the scope — the body [`run_in_async_scope`] and the closures
/// [`bind`]/[`static_bind`] hand back all share, so the fire/push/pop
/// sequence exists in exactly one place.
///
/// `after` fires even when `target` did nothing useful, which is Node's rule
/// that it fires "even if it threw" as closely as this engine allows: a
/// native cannot observe an exception, because [`rts_core::entry::call`]
/// cannot unwind into one.
fn enter_and_call(resource: u64, target: u64, this_arg: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let id = id_on(resource, "__asyncId__");
    let trigger = id_on(resource, "__triggerId__");
    SCOPES.with(|scopes| {
        scopes.borrow_mut().push(Scope {
            async_id: id,
            trigger,
            resource,
        })
    });
    super::hooks::fire_id("before", id);
    let result = rts_core::entry::call(target, this_arg, a0, a1, a2, a3);
    super::hooks::fire_id("after", id);
    SCOPES.with(|scopes| {
        scopes.borrow_mut().pop();
    });
    result
}

/// `resource.runInAsyncScope(fn, thisArg, ...args)` — the first extra argument
/// is forwarded, the rest are not; see [`super`]'s refusal list.
extern "C" fn run_in_async_scope(
    _e: u64,
    this: u64,
    body: u64,
    this_arg: u64,
    first: u64,
    _d: u64,
) -> u64 {
    let undefined = rts_core::entry::undefined_value();
    enter_and_call(this, body, this_arg, first, undefined, undefined, undefined)
}

/// `resource.emitDestroy()` — fires `destroy` once and answers `this`.
///
/// A second call is reported to stderr instead of throwing the error Node
/// throws, for the reason [`super`]'s list gives. What it must not do is fire
/// `destroy` twice, which is the half of that contract observable from here.
extern "C" fn emit_destroy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let id = id_on(this, "__asyncId__");
    let already = rts_core::entry::with_runtime(|context| {
        let held = rts_core::entry::get_member(context, this, "__destroyed__");
        let already = rts_core::entry::number_of(held).is_some();
        if !already {
            let mark = rts_core::entry::make_number(1.0);
            rts_core::entry::put_member(context, this, "__destroyed__", mark);
        }
        already
    });
    match already {
        true => eprintln!(
            "node:async_hooks: emitDestroy() called twice on the same AsyncResource (async id {id}) — real Node throws here; this engine cannot, so the second call did nothing"
        ),
        false => super::hooks::fire_id("destroy", id),
    }
    this
}

/// `resource.asyncId()`.
extern "C" fn async_id(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core::entry::make_number(id_on(this, "__asyncId__") as f64)
}

/// `resource.triggerAsyncId()`.
extern "C" fn trigger_async_id_method(
    _e: u64,
    this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    rts_core::entry::make_number(id_on(this, "__triggerId__") as f64)
}

/// `async_hooks.executionAsyncId()`.
pub(super) extern "C" fn execution_async_id(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    rts_core::entry::make_number(current_id() as f64)
}

/// `async_hooks.triggerAsyncId()`.
pub(super) extern "C" fn trigger_async_id(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let trigger = SCOPES.with(|scopes| scopes.borrow().last().map_or(0, |scope| scope.trigger));
    rts_core::entry::make_number(trigger as f64)
}

/// `async_hooks.executionAsyncResource()` — the innermost entered resource, or
/// the one shared top-level object.
pub(super) extern "C" fn execution_async_resource(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let entered = SCOPES.with(|scopes| scopes.borrow().last().map(|scope| scope.resource));
    match entered {
        Some(resource) => resource,
        None => match TOP_LEVEL.with(Cell::get) {
            0 => rts_core::entry::undefined_value(),
            placeholder => placeholder,
        },
    }
}

/// `resource.bind(fn, thisArg?)` — see the module doc: this crate's earlier
/// refusal named `entry::make_callable`'s fixed function pointer as the wall;
/// [`rts_core::entry::closure_new`] takes a code address AND an environment,
/// which is exactly the "remember this resource, this target, this receiver"
/// capture `bind` needs. A non-callable `fn` answers `undefined` — the
/// reference doc's `TypeError` is a throw this surface cannot raise.
extern "C" fn bind(_e: u64, this: u64, target: u64, this_arg: u64, _c: u64, _d: u64) -> u64 {
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, target)) {
        return rts_core::entry::undefined_value();
    }
    // Default is the resource itself. The reference doc calls this "the
    // caller's `this`", and for the ordinary `resource.bind(fn)` call form
    // that IS the resource — a method call's own receiver — so there is no
    // second, different "caller" to read.
    let bound_this = match this_arg == rts_core::entry::undefined_value() {
        true => this,
        false => this_arg,
    };
    let environment = rts_core::entry::make_array(vec![this, target, bound_this]);
    rts_core::entry::closure_new(bound as *const () as usize as i64, environment)
}

/// The wrapper both [`bind`] and [`static_bind`] hand back. Every argument
/// slot is free for forwarding — the resource, target and receiver all live
/// in `environment` instead — so a bound function loses none of Node's
/// `...args`, unlike [`run_in_async_scope`]'s direct call form.
extern "C" fn bound(environment: u64, _call_this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let resource = rts_core::entry::element_at(environment, rts_core::entry::make_number(0.0));
    let target = rts_core::entry::element_at(environment, rts_core::entry::make_number(1.0));
    let bound_this = rts_core::entry::element_at(environment, rts_core::entry::make_number(2.0));
    enter_and_call(resource, target, bound_this, a0, a1, a2, a3)
}

/// `AsyncResource.bind(fn, type?, thisArg?)` (static) — makes a resource bound
/// to the CURRENT context (the same "inherit unless a number was given" rule
/// [`construct`] already applies with no explicit `triggerAsyncId`), then
/// delegates to [`bind`] exactly as the reference doc describes it: "creates a
/// new `AsyncResource`… and returns `fn` bound to it."
extern "C" fn static_bind(_e: u64, _this: u64, target: u64, type_label: u64, this_arg: u64, _d: u64) -> u64 {
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, target)) {
        return rts_core::entry::undefined_value();
    }
    // `.filter` rather than a bare `unwrap_or_else` on `given`: Node's own rule
    // is `type || fn.name || 'bound-anonymous'`, and `||` treats an explicit
    // empty string as falsy too — it falls through to `fn.name` rather than
    // winning outright.
    let given = rts_core::entry::with_runtime(|context| rts_core::entry::string_in(context, type_label))
        .filter(|text| !text.is_empty());
    let label = given.unwrap_or_else(|| {
        let name = rts_core::entry::with_runtime(|context| {
            rts_core::entry::get_member(context, target, "name")
        });
        rts_core::entry::with_runtime(|context| rts_core::entry::string_in(context, name))
            .unwrap_or_default()
    });
    let label = match label.is_empty() {
        true => "bound-anonymous".to_string(),
        false => label,
    };
    let label_value = rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, &label));
    let undefined = rts_core::entry::undefined_value();
    let resource = construct(0, undefined, label_value, undefined, 0, 0);
    bind(0, resource, target, this_arg, 0, 0)
}
