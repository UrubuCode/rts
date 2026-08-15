//! `MessageChannel` and `MessagePort` — two ends of one queue, on one thread.
//!
//! # Reuse-check: what was searched, and what was found
//!
//! - **`rts-core::entry::loops`** — the answer to "this must happen later, on the
//!   program's own thread". A delivery is not synchronous: `port2.postMessage(v)`
//!   returns before `port1.onmessage` runs, which is the whole difference between
//!   a channel and a function call. `declare_loop_source` is what
//!   `AbortSignal.timeout` next door already uses, and a second timing mechanism
//!   here would be a second answer to what holds a program open.
//! - **`rts-core::entry::external`** — the answer to "a native is holding a value
//!   after its call returned". A queued message lives in a Rust `VecDeque` on the
//!   Rust heap, which no root scan of ours reaches, so a collection between the
//!   post and the delivery would sweep it out from under the queue. `hold_current`
//!   registers it where `entry::roots` looks and `release_current` gives it back.
//!   This is the same hole `entry::rooted`'s module doc measured for lists a
//!   native builds; that type is `pub(in crate::entry)` and this is the form
//!   reachable from a host crate.
//! - **`rts-core::entry::deep_copy`** — `structuredClone`'s walk, called rather
//!   than approximated. A message is a COPY, and writing a second copier here
//!   would be a second answer to what a cycle, a `Map` and a `Date` become.
//! - **`rts-node/src/worker_threads/`** — read in full. Its `MessagePort` is a
//!   different object for a different job: it crosses THREADS, so what travels is
//!   its `portable::Portable` — a Rust enum that names what can be rebuilt in
//!   another region — and it is an `EventEmitter` (`port.on('message')`) rather
//!   than an `EventTarget`. Its own module doc names "`MessageChannel`/
//!   `MessagePort` between threads" as absent and the local pair as same-thread
//!   only. Sharing an implementation would mean one object answering to two
//!   listener contracts and two value representations, which is what this module
//!   tree's doc already refuses for `EventEmitter` and `EventTarget`.
//!
//! # A port is always started, and `start()` is inert
//!
//! The specification queues messages at an unstarted port and delivers them when
//! `start()` is called — and assigning `onmessage` starts it implicitly. Implicit
//! start needs a property SETTER, which is `#[rtse::class]`'s and not reachable
//! from a host crate; this module tree's doc records that same wall for
//! `signal.aborted`. So delivery is enabled from construction.
//!
//! The divergence, named: a port that only ever called `addEventListener('message',
//! …)` receives here, where the specification requires an explicit `start()`
//! first. Nothing is ever LOST by it — a message arriving at a port with no
//! listener is dropped in both, since the queue that would have held it is the
//! unstarted one this does not have.
//!
//! # `MessageEvent` has a prototype and is not a global
//!
//! `event instanceof Event` holds — the prototype chains onto `Event.prototype`,
//! the way `CustomEvent`'s does — and the name `MessageEvent` is NOT installed on
//! the global object. That is this repository's rule about `rts-codegen`'s
//! `PROVIDED` list read from the value's side: a name goes on that list in the
//! change that gives it a value, and `new MessageEvent(type, init)` is a
//! constructor nothing here builds. A program reaches the event through its
//! handler, which is how every program actually reaches one.
//!
//! # Not implemented, by name
//!
//! `transfer` — the second argument of `postMessage`, and `options.transfer`. A
//! transfer moves an `ArrayBuffer` rather than copying it and DETACHES the
//! original; `structuredClone` here already does exactly that for its own
//! `transfer` list (`entry::clone`), so this is a wiring gap rather than a
//! missing mechanism. `port.onmessageerror` — nothing here can fail to
//! deserialise, since nothing is serialised. `MessagePort` as a global name and
//! `new MessagePort()`, which the specification makes a `TypeError` anyway.
//! `ref`/`unref`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use rts_core::entry::{self, Context, Pending, Provided};

/// The other end of a channel. Each port names its peer and nothing else does.
const PEER: &str = "__peer__";

/// Whether `close()` has been called on this port.
const CLOSED: &str = "__closed__";

/// The property a message is delivered through when no listener is registered.
const HANDLER: &str = "onmessage";

thread_local! {
    /// What has been posted and not yet delivered, oldest first.
    ///
    /// Per thread and not one table, for the reason `abort`'s `DEADLINES` states
    /// and `node:timers` measured: a port is a cell in the region of the thread
    /// that made it, so a shared queue would let one thread deliver into a region
    /// this thread does not have.
    static QUEUE: RefCell<VecDeque<Delivery>> = const { RefCell::new(VecDeque::new()) };
}

/// One posted message, named by what holds it alive.
///
/// Two identifiers rather than two words: a `u64` sitting in this queue is
/// invisible to the collector, and both the destination port and the message are
/// values that a collection between the post and the delivery would otherwise
/// take. See the module doc's reuse-check on `entry::external`.
struct Delivery {
    /// The port the message is for.
    port: u32,
    /// The message itself, already copied.
    message: u32,
}

const PORT_METHODS: &[(&str, Provided)] = &[
    ("postMessage", post_message),
    ("close", close),
    ("start", start),
];

/// Builds the class and answers the `MessageChannel` constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    // Registered by this module at install time, never by the host — the rule
    // `entry::loops` was written to end.
    entry::declare_loop_source(context, "rts-std:message-channel", pump);
    let prototype = entry::make_prototype(context, "MessageChannel", &[]);
    super::class_ctor(context, construct, prototype)
}

/// `MessagePort.prototype`, linked to `EventTarget.prototype`.
///
/// The link is what makes `port.addEventListener('message', …)` work: it is found
/// by the ordinary chain walk, so there is one `EventTarget` implementation
/// rather than a copy of three methods on a second prototype.
fn port_prototype(context: &mut Context) -> u64 {
    let base = super::target::prototype(context);
    let made = entry::make_prototype(context, "MessagePort", PORT_METHODS);
    entry::set_prototype_in(context, made, base);
    made
}

/// `MessageEvent.prototype`, linked to `Event.prototype`.
///
/// Nothing is installed on it: `preventDefault` and the rest are found by the
/// chain walk, which is also what makes `event instanceof Event` true — the same
/// shape `CustomEvent` takes next door.
fn event_prototype(context: &mut Context) -> u64 {
    let base = super::event::prototype(context);
    let made = entry::make_prototype(context, "MessageEvent", &[]);
    entry::set_prototype_in(context, made, base);
    made
}

/// `new MessageChannel()` — two ports, each naming the other.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "MessageChannel", &[]);
        let channel = super::self_or_new(context, this, prototype);
        let one = fresh_port(context);
        let two = fresh_port(context);
        // The cycle is deliberate and is what a channel IS: neither port is
        // useful without the other, and the pair dies together.
        entry::put_member(context, one, PEER, two);
        entry::put_member(context, two, PEER, one);
        entry::put_member(context, channel, "port1", one);
        entry::put_member(context, channel, "port2", two);
        channel
    })
}

/// A port with its listener store, its peer still unset.
fn fresh_port(context: &mut Context) -> u64 {
    let prototype = port_prototype(context);
    let port = entry::make_instance(context, prototype);
    super::target::prepare(context, port);
    entry::put_member(context, port, CLOSED, entry::boolean_value(false));
    // `null` rather than `undefined`, which is what the specification says a
    // never-assigned event handler reads as, and what `port.onmessage === null`
    // tests for.
    let null = entry::null_in(context);
    entry::put_member(context, port, HANDLER, null);
    port
}

/// `port.postMessage(value)` — queued for the peer, delivered on a later turn.
///
/// A COPY, through `structuredClone`'s own walk: the specification serialises,
/// and a receiver that got the sender's object would see a mutation the sender
/// made afterwards. Copying here rather than at delivery is what makes that
/// faithful — the value is captured as it was when it was posted.
extern "C" fn post_message(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if super::flag(this, CLOSED) {
        return super::absent();
    }
    let peer = super::get(this, PEER);
    if peer == super::absent() {
        return super::absent();
    }
    // Outside every borrow: the walk allocates, and it is an entry point that
    // takes the runtime for itself.
    let copied = entry::deep_copy(value);
    let delivery = Delivery {
        port: entry::hold_current(peer),
        message: entry::hold_current(copied),
    };
    QUEUE.with(|queue| queue.borrow_mut().push_back(delivery));
    super::absent()
}

/// `port.close()` — nothing more is delivered to it, and nothing it already
/// posted is delivered either.
///
/// The second half is what the specification says and is easy to get wrong by
/// omission: closing disentangles the pair, so a message still in the queue for
/// the peer is dropped rather than arriving after the close.
extern "C" fn close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::put(this, CLOSED, entry::boolean_value(true));
    super::absent()
}

/// `port.start()` — inert, because a port here is already started; see the
/// module doc for why implicit start is not available to a host crate.
extern "C" fn start(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::absent()
}

/// Delivers what has been posted, and says whether anything is still waiting.
///
/// [`Pending::In`] with no delay rather than [`Pending::Blocked`], and that is
/// the difference that makes the whole module work: a posted message DOES hold
/// the program open — `channel.port2.postMessage(v)` as the last statement of a
/// script still reaches `port1.onmessage`, in this engine as in Node — where a
/// pending `AbortSignal.timeout` deliberately does not.
///
/// The queue's borrow is released before anything is delivered, because a
/// handler may post again and would re-enter it.
fn pump() -> Pending {
    let due: Vec<Delivery> = QUEUE.with(|queue| queue.borrow_mut().drain(..).collect());
    for delivery in due {
        let port = entry::release_current(delivery.port);
        let message = entry::release_current(delivery.message);
        let (Some(port), Some(message)) = (port, message) else {
            continue;
        };
        if super::flag(port, CLOSED) {
            continue;
        }
        deliver(port, message);
    }
    // Non-empty here means a handler posted while this pass was delivering, which
    // is an ordinary ping-pong and must be asked about again.
    match QUEUE.with(|queue| queue.borrow().is_empty()) {
        true => Pending::Idle,
        false => Pending::In(Duration::ZERO),
    }
}

/// Fires one `'message'` event at one port.
///
/// `onmessage` is read as a plain property and called AFTER the registered
/// listeners, which is the same divergence `signal.onabort` states next door and
/// has the same cause: the specification registers the handler as a listener at
/// the moment it is assigned, and reproducing that needs a property setter.
fn deliver(port: u64, message: u64) {
    let event = entry::with_runtime(|context| {
        let prototype = event_prototype(context);
        let event = entry::make_instance(context, prototype);
        super::event::init(context, event, "message", &super::event::Flags::INERT);
        entry::put_member(context, event, "data", message);
        // Both are `""` for a port, which is what the specification says and what
        // a program reading `ev.origin` finds in Node.
        let empty = entry::make_string(context, "");
        entry::put_member(context, event, "origin", empty);
        entry::put_member(context, event, "lastEventId", empty);
        let null = entry::null_in(context);
        entry::put_member(context, event, "source", null);
        let none = entry::make_array_in(context, Vec::new());
        entry::put_member(context, event, "ports", none);
        // The engine generated it, which is the only case the reference marks as
        // trusted.
        entry::put_member(context, event, "isTrusted", entry::boolean_value(true));
        event
    });
    super::target::dispatch_event(port, event);
    let handler = super::get(port, HANDLER);
    if entry::with_runtime(|context| entry::is_callable_in(context, handler)) {
        let absent = super::absent();
        entry::call(handler, port, event, absent, absent, absent);
    }
}
