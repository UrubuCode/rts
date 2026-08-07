//! `Session` and the `post` dispatch behind it.
//!
//! # Why this needs no endpoint
//!
//! In Node a `Session` talks to the engine's backend in process, and an attached
//! frontend is a second, optional consumer of the same backend — Node's own two
//! canonical examples (CPU and heap profiling through `Session`) never attach
//! one. So `post` here is a pure in-process dispatch and works whether or not
//! `open()` was ever called.
//!
//! # The allowlist, and what "not implemented" has to look like
//!
//! Every method outside the list below answers a protocol-shaped error, which is
//! what `ERR_INSPECTOR_COMMAND` is in Node. Never a crash, never a hang, and —
//! the one that matters — never a fabricated result. `Profiler.start` returning
//! an empty profile would be a wrong answer that runs, and this repository
//! refuses those in preference to admitting a gap.

use rts_core_rwk::entry::{self, Context};

/// What one method answered.
enum Answer {
    /// A result object built into the context in hand.
    Value(u64),
    /// A protocol-shaped failure, by its message.
    Refused(String),
    /// Source to compile and run, which cannot happen here: evaluating installs
    /// a context of its own, and this runs with one borrowed. Its own variant
    /// rather than a marker inside `Refused` — a string prefix understood by one
    /// caller is a protocol, and a protocol nobody named is a bug waiting.
    Evaluate(String),
}

/// Runs one protocol method.
///
/// # Why the whole dispatch takes a context
///
/// Every branch either builds an object or does not, and a branch that reached
/// for the ambient form would abort. Taking the context makes that
/// unrepresentable instead of a rule to remember — the shape this crate's other
/// modules converged on after nine aborts.
fn dispatch(context: &mut Context, method: &str, params: u64) -> Answer {
    match method {
        // Real acknowledgements with no state. Node code sends these
        // unconditionally before using a domain, so refusing them would make
        // every such program fail on its first call for no useful reason.
        "Runtime.enable" | "Runtime.disable" | "Debugger.enable" | "Debugger.disable"
        | "HeapProfiler.enable" | "HeapProfiler.disable" | "Profiler.enable"
        | "Profiler.disable" => Answer::Value(entry::make_object(context)),

        // Bridged to the host's evaluator — the same seam `node:vm` uses, and
        // reached rather than reimplemented. It answers only what can leave the
        // program it ran: a reference belongs to the region that made it.
        "Runtime.evaluate" => {
            let source = entry::get_member(context, params, "expression");
            let Some(text) = entry::string_in(context, source) else {
                return Answer::Refused("Runtime.evaluate needs an 'expression' string".to_owned());
            };
            // OUTSIDE the borrow: `entry::evaluate` compiles and runs a whole
            // program, which installs a context of its own. Calling it with this
            // one borrowed is the nested borrow that aborts.
            Answer::Evaluate(text)
        }

        // The heap primitive `node:v8` already exposes, read the same way. No
        // second heap-introspection path is built for this module.
        "Runtime.getHeapUsage" | "HeapProfiler.getSamplingProfile" => {
            let stride = u64::from(rts_core_rwk::heap::STRIDE);
            let total = u64::from(context.region.capacity()) * stride;
            let used = u64::from(context.region.used()) * stride;
            let object = entry::make_object(context);
            let used_value = entry::make_number(used as f64);
            entry::put_member(context, object, "usedSize", used_value);
            let total_value = entry::make_number(total as f64);
            entry::put_member(context, object, "totalSize", total_value);
            Answer::Value(object)
        }

        // The domains actually backed above, not Node's full list. Answering
        // Node's list would tell a frontend it can drive domains that refuse.
        "Schema.getDomains" => {
            let domains = ["Runtime", "Debugger", "HeapProfiler", "Schema"]
                .iter()
                .map(|name| {
                    let entry_object = entry::make_object(context);
                    let held = entry::make_string(context, name);
                    entry::put_member(context, entry_object, "name", held);
                    let version = entry::make_string(context, "1.3");
                    entry::put_member(context, entry_object, "version", version);
                    entry_object
                })
                .collect();
            let list = entry::make_array_in(context, domains);
            let object = entry::make_object(context);
            entry::put_member(context, object, "domains", list);
            Answer::Value(object)
        }

        // Named rather than swept into the default, because the reason is
        // specific and worth a program seeing: there is no sampling profiler in
        // this engine, and `node:v8`'s `startCpuProfile` documents the identical
        // gap. An empty profile would look like a working profiler.
        "Profiler.start" | "Profiler.stop" | "HeapProfiler.takeHeapSnapshot" => Answer::Refused(
            format!("{method} needs a sampling profiler, which this engine does not have"),
        ),

        _ => Answer::Refused(format!("'{method}' is not implemented by this inspector")),
    }
}

/// `session.post(method, params, callback)`.
///
/// The callback is `(error, result)`, Node's own order. It runs SYNCHRONOUSLY,
/// which diverges from Node, where the reply arrives on the next turn — the
/// module doc says so. A deferred reply needs a promise or a queue this
/// dispatch has nothing to wait for, and inventing a delay would make every
/// caller slower for no fidelity a program can observe.
pub(super) extern "C" fn post(_e: u64, this: u64, method: u64, params: u64, callback: u64, _d: u64) -> u64 {
    let _ = this;
    let absent = entry::undefined_value();
    // `post(method, callback)` — Node's two-argument overload, and the one a
    // program that passes no params writes. Without this the callback lands in
    // the params slot and is never called.
    let (params, callback) = crate::fs::options_and_listener(params, callback);
    let Some(name) = entry::text_of(method) else {
        return absent;
    };
    let answered = entry::with_runtime(|context| dispatch(context, &name, params));
    let (error, result) = match answered {
        Answer::Value(value) => (absent, value),
        // The one branch that has to leave the borrow: evaluating compiles and
        // runs a program, which installs a context of its own.
        Answer::Evaluate(source) => {
            let produced = entry::evaluate(&source);
            entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let value = produced.unwrap_or_else(|| entry::undefined_in(context));
                let wrapper = entry::make_object(context);
                entry::put_member(context, wrapper, "value", value);
                entry::put_member(context, object, "result", wrapper);
                (entry::undefined_in(context), object)
            })
        }
        Answer::Refused(reason) => entry::with_runtime(|context| {
            let object = entry::make_object(context);
            let message = entry::make_string(context, &reason);
            entry::put_member(context, object, "message", message);
            // Node's code for a method the backend refuses. A program that
            // switches on `err.code` sees the same value it would there.
            let code = entry::make_string(context, "ERR_INSPECTOR_COMMAND");
            entry::put_member(context, object, "code", code);
            (object, entry::undefined_in(context))
        }),
    };
    if callback != absent {
        entry::call(callback, this, error, result, absent, absent);
    }
    absent
}

/// `session.connect()` / `connectToMainThread()` / `disconnect()`.
///
/// Real state rather than a no-op: Node throws on a second `connect()` of the
/// same session, and a program that relies on that would otherwise get silence.
/// Nothing else here depends on being connected, because `post` talks to this
/// process and not to a socket.
pub(super) extern "C" fn connect(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let connected = entry::boolean_value(true);
        entry::put_member(context, this, "__connected", connected);
    });
    entry::undefined_value()
}

pub(super) extern "C" fn disconnect(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let connected = entry::boolean_value(false);
        entry::put_member(context, this, "__connected", connected);
    });
    entry::undefined_value()
}
