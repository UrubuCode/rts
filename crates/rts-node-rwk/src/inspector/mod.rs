//! `node:inspector` (+ `node:inspector/promises`) — the API shape, backed by
//! what this engine actually has.
//!
//! # The rule this module is built under
//!
//! In Node this is a thin binding over the JavaScript engine's own
//! debugger/profiler backend. There is no such backend here and this project
//! does not acquire one — `inspector.md` §5.1 states the no-V8 rule and picks
//! scope **(b)**: reproduce the shape, back each member with a real primitive
//! where an honest equivalent exists, and refuse by name everywhere else.
//!
//! So nothing in here emulates a protocol. What is real is real, and what is
//! not says so through `ERR_INSPECTOR_COMMAND` rather than through a fabricated
//! result.
//!
//! # What is genuinely backed
//!
//! - `open`/`close`/`url`/`waitForDebugger` — a real loopback `TcpListener` and
//!   a real HTTP discovery responder. `url()` answers an address something can
//!   reach; `waitForDebugger()` blocks on a real accepted connection.
//! - `Session.post` for `Runtime.evaluate` — through `entry::evaluate`, the
//!   same seam `node:vm` runs on, reached rather than reimplemented.
//! - `Session.post` for heap usage — the same `region.capacity()`/`used()`
//!   primitive `node:v8`'s `getHeapStatistics` reports, not a second one.
//! - `Session.post` for the enable/disable acknowledgements, so a program that
//!   sends them unconditionally is not stopped by them.
//! - `Schema.getDomains` — the domains actually backed, not Node's full list.
//!
//! # Not implemented, by name
//!
//! **No WebSocket upgrade and no JSON-RPC command loop.** The listener answers
//! discovery and nothing else, so a DevTools-class frontend that probes and then
//! tries to attach fails at the upgrade. That is `inspector.md` §5.1's named
//! deferral, and it is the whole difference between this and scope (a).
//!
//! `Profiler.start`/`stop` and `HeapProfiler.takeHeapSnapshot` — there is no
//! sampling profiler in this engine, the identical gap `node:v8`'s
//! `startCpuProfile` documents. They refuse; an empty profile would look like a
//! working profiler, which is worse than nothing.
//!
//! `connectToMainThread` (there is one inspector per process and no main-thread
//! session to join), `inspector.console`, `Network.*`/`NetworkResources.*`/
//! `DOMStorage.*` (documented as programmatic broadcast helpers into a
//! frontend fan-out; with no attachable frontend they would be a queue nothing
//! drains, and a queue nothing drains is a leak wearing an API's clothes),
//! `session.post` returning a Promise (`node:inspector/promises` resolves to
//! this same namespace — see below), and per-method notification events.
//!
//! `open()`'s `host` argument is accepted and IGNORED. The bind is always
//! loopback: widening it is a security decision, and honouring an argument that
//! widens it silently is how such a decision gets made by accident.

mod endpoint;
mod session;

use rts_core_rwk::entry::{self, Context, Provided};

/// The methods on `Session.prototype`.
const SESSION_METHODS: &[(&str, Provided)] = &[
    ("connect", session::connect),
    ("disconnect", session::disconnect),
    ("post", session::post),
];

/// The namespace `node:inspector` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("open", open),
        ("close", close),
        ("url", url),
        ("waitForDebugger", wait_for_debugger),
    ];
    let namespace = entry::make_namespace(context, members);

    let parent = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "InspectorSession", SESSION_METHODS);
    entry::set_prototype_in(context, prototype, parent);
    let constructor = entry::make_callable(context, construct);
    entry::put_member(context, constructor, "prototype", prototype);
    entry::put_member(context, namespace, "Session", constructor);
    namespace
}

/// `new inspector.Session()`.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let parent = entry::make_prototype(context, "EventEmitter", &[]);
        let prototype = entry::make_prototype(context, "InspectorSession", SESSION_METHODS);
        entry::set_prototype_in(context, prototype, parent);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let listeners = entry::make_object(context);
        entry::put_member(context, instance, "__events__", listeners);
        let connected = entry::boolean_value(false);
        entry::put_member(context, instance, "__connected", connected);
        instance
    })
}

/// `inspector.open([port[, host[, wait]]])`.
///
/// Node answers a `Disposable`. This answers `undefined`: a disposable needs
/// `Symbol.dispose`, which this crate cannot mint, and `close()` is the same
/// operation under a name that works today.
extern "C" fn open(_e: u64, _this: u64, port: u64, _host: u64, wait: u64, _d: u64) -> u64 {
    // Zero asks the operating system for a free port, which is also what a
    // program passing nothing means.
    let wanted = entry::number_of(port).unwrap_or(0.0) as u16;
    let waiting = wait == entry::boolean_value(true);
    match endpoint::open(wanted) {
        Ok(_) => {
            if waiting {
                endpoint::wait();
            }
        }
        // Refused rather than reported: `open` has no error channel in Node
        // either, and a second `open` throwing there is a difference this cannot
        // express without a throw. The state is readable through `url()`.
        Err(_) => {}
    }
    entry::undefined_value()
}

extern "C" fn close(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    endpoint::close();
    entry::undefined_value()
}

/// `inspector.url()` — the address, or `undefined` when nothing is open.
extern "C" fn url(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = endpoint::url();
    entry::with_runtime(|context| match held {
        Some(text) => entry::make_string(context, &text),
        None => entry::undefined_in(context),
    })
}

/// `inspector.waitForDebugger()` — blocks until something connects.
///
/// A no-op when nothing is open, which is what Node does. Blocking forever on an
/// endpoint that does not exist is a hang a program cannot diagnose.
extern "C" fn wait_for_debugger(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if endpoint::is_open() {
        endpoint::wait();
    }
    entry::undefined_value()
}
