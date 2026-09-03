// node:domain — the call that kills the process: routing an 'error' event to
// the domain at all, through ANY of the three doors that end at
// `emit_error(domain, err)` (`domain.rs`'s own name): `d.add(emitter)` +
// `emitter.emit('error', …)`, or `d.intercept(cb)`'s error branch, or a bare
// `d.emit('error', …)`. All three call the identical function, so one
// minimal repro stands for all of them.
//
// Root cause, found by isolating each layer in turn (Domain's `emit`
// wrapper for an added emitter → still failed; `intercept`'s direct
// `emit_error` → still failed; a bare `d.on('error', cb); d.emit('error',
// err)` with NO other module involved → still failed): a `Domain` instance
// is never actually initialized as an `EventEmitter`.
//
// `crates/rts-node/src/events/mod.rs`'s `make_emitter` — the real `new
// EventEmitter()` constructor — is what sets the two own properties
// `__events__` and `__eventNames__` that every other EventEmitter method
// reads and writes (`events_object()` is a bare `get_indexed(this,
// "__events__")`, nothing lazy). `domain.rs`'s `fresh()` never calls it: it
// builds a `Domain` instance with `entry::make_instance(context,
// prototype)`, where `prototype` chains onto a same-named "EventEmitter"
// prototype for its METHODS (`on`, `emit`, …) — but chaining onto a
// prototype for METHODS is not the same as running that class's
// CONSTRUCTOR, and nothing else calls it. So a `Domain` instance's `.on()`
// silently writes into `get_indexed(undefined, event)` (its `__events__` is
// genuinely `undefined` — confirmed: `d.hasOwnProperty('__events__')` is
// `false`) and never persists a listener anywhere `.emit()` can find it.
//
// This is checked directly below with NO other module involved — no
// `add()`, no emitter, just `d.on('error', …)` followed by `d.emit('error',
// …)` on the SAME domain object — because that already reproduces it:
//
//   d.on("error", () => {});   // silently does nothing: __events__ is undefined
//   d.emit("error", new Error("boom"));  // finds zero listeners for 'error'
//
// `events/emit.rs`'s own no-listener-for-'error' fallback then fires — the
// same one a plain `new EventEmitter().emit('error', err)` hits — and in
// THIS engine that fallback is a hard process exit (`rts: uncaught 'error'
// event: an object`, exit code 1), not the catchable `TypeError` Node
// throws. Node itself was checked too (`node -e`): a plain `emitter.emit
// ('error', err)` with no listener THROWS there, catchable by an ordinary
// try/catch around it — it does not end the process. This engine's
// no-listener-for-'error' behavior being a hard abort rather than a
// catchable throw is a pre-existing gap in `node:events`, outside this
// task's assigned area — but it is what turns "the domain never gets the
// error" from a silent bug into a process-ending one for EVERY documented
// use of `domain.add()`/`domain.intercept()`'s error path.
//
// The practical result: `domain.add(emitter)` — the module's own headline
// feature, "an override that routes 'error' to the domain instead of
// crashing the process" — does not merely fail to route; it crashes the
// process in exactly the case it exists to prevent. `domain.intercept()`'s
// error branch is unusable for the identical reason. No test() body below
// runs — the panic-equivalent abort happens while evaluating this file's
// top-level code, before any test() call executes.
import domain from "node:domain";

const d = domain.create();
d.on("error", () => {
    console.log("unreachable — this domain listener is never actually stored");
});
d.emit("error", new Error("boom"));
console.log("also unreachable — the process exits before this line");
