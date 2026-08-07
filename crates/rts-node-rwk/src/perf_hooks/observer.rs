//! `PerformanceObserver` and `PerformanceObserverEntryList`.
//!
//! # Why the loop source, and not the microtask queue
//!
//! The reference doc (§5.3) asks for delivery on the microtask drain. That is
//! not reachable from a host module: `entry::drain_microtasks` runs the queue,
//! it does not let a module add a step to it, and `queueMicrotask`'s own
//! implementation in `rts-std-rwk` gets in only by attaching a reaction to a
//! settled promise — which would deliver each notification through a callback
//! this module would then have to own the identity of.
//!
//! [`rts_core_rwk::entry::declare_loop_source`] is the seam that exists, and
//! its module doc is explicit that it is THE answer to "a module with work
//! left after the program's last statement" — written precisely because six
//! modules had grown six copies of the same idea. So an observer's callback
//! runs on the program's own thread, asynchronously with respect to the
//! `mark()` that produced the entry, batched (every entry recorded since the
//! observer's last notification arrives in one list), which is the contract
//! §4 states.
//!
//! # What that costs, named
//!
//! Delivery happens when the loop is pumped, not at the end of the current
//! microtask checkpoint. A program that marks and then blocks synchronously
//! forever is never notified — but neither is it in Node, where the same work
//! is queued behind the same never-yielded loop.
//!
//! [`source`] answers [`Pending::Idle`] and therefore does NOT hold a program
//! open. An observer that kept the process alive would make every program that
//! ever constructed one hang, and Node's observers do not keep the loop alive
//! either.

use rts_core_rwk::entry::{self, Context, Pending, Provided};
use std::cell::RefCell;

use super::timeline;

/// One active subscription.
///
/// `object` is a JavaScript value held in a Rust static across calls, which is
/// this crate's established shape (`domain`'s `STACK`, `async_hooks`'s frames)
/// and carries this crate's one added assumption, stated where `fs/watch`
/// states it: it is never dereferenced as an address and is read back only by
/// the thread that stored it — guaranteed here because the table is
/// `thread_local!`.
struct Watch {
    object: u64,
    /// The entry types subscribed to. Names Node accepts but this engine never
    /// produces (`'gc'`, `'http'`, …) are kept rather than rejected: the
    /// subscription is legitimate and simply never matches, which is what a
    /// program can observe and reason about.
    types: Vec<String>,
    /// How far into the timeline this observer has already been told about.
    cursor: usize,
}

thread_local! {
    static OBSERVERS: RefCell<Vec<Watch>> = const { RefCell::new(Vec::new()) };
}

/// `PerformanceObserver.supportedEntryTypes`.
///
/// The set this engine actually produces, not the set Node lists. Reporting
/// `'gc'` here would tell a program that subscribing to it will yield
/// something.
pub(super) const SUPPORTED: &[&str] = &["mark", "measure", "function"];

const METHODS: &[(&str, Provided)] = &[
    ("observe", observe),
    ("disconnect", disconnect),
    ("takeRecords", take_records),
];

pub(super) fn install(context: &mut Context, namespace: u64) {
    let prototype = entry::make_prototype(context, "PerformanceObserver", METHODS);
    let constructor = entry::make_callable(context, construct);
    entry::put_member(context, constructor, "prototype", prototype);
    let supported: Vec<u64> = SUPPORTED
        .iter()
        .map(|name| entry::make_string(context, name))
        .collect();
    let supported = entry::make_array_in(context, supported);
    entry::put_member(context, constructor, "supportedEntryTypes", supported);
    entry::put_member(context, namespace, "PerformanceObserver", constructor);
    entry::make_prototype(
        context,
        "PerformanceObserverEntryList",
        timeline::LIST_METHODS,
    );
}

/// The property the callback is kept under.
///
/// On the instance rather than in [`Watch`], so that an observer constructed
/// and never `observe`d holds its own callback without this module keeping a
/// record of an object nothing subscribed.
const CALLBACK: &str = "__callback__";

/// `new PerformanceObserver(callback)`.
///
/// A non-callable `callback` is not stored, so `observe` later finds nothing
/// and subscribes nothing — Node throws `ERR_INVALID_ARG_TYPE`, refused here
/// for the reason the crate-module doc gives for every throw.
extern "C" fn construct(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "PerformanceObserver", METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        // `is_callable_in` and not `to_boolean`: the question is what the value
        // IS, and every object is truthy.
        if entry::is_callable_in(context, callback) {
            entry::put_member(context, instance, CALLBACK, callback);
        }
        instance
    })
}

/// The strings of a JavaScript array, read by probing indices until one is
/// absent.
///
/// # Why probing and not a length read
///
/// Nothing on the host entry surface reads an array's `length` or an element
/// from inside a borrow, and the ambient `get_indexed` cannot be called from
/// inside one. So the elements are read ambiently, outside every borrow, and
/// the end of the array is where an element reads back `undefined` — which for
/// an `entryTypes` array is exactly the end, since a hole in it would not be a
/// type name either. The cap is what stops a hostile or accidental
/// enormous array from being walked; it is stated rather than silent.
fn string_elements(array: u64) -> Vec<String> {
    const CAP: usize = 32;
    let absent = entry::undefined_value();
    let mut found = Vec::new();
    for index in 0..CAP {
        let item = entry::get_indexed(array, entry::make_number(index as f64));
        if item == absent {
            break;
        }
        // `string_in` and not `text_of`: a non-string in `entryTypes` is not a
        // type name, and coercing it would subscribe to `"[object Object]"`.
        match entry::with_runtime(|context| entry::string_in(context, item)) {
            Some(text) => found.push(text),
            None => break,
        }
    }
    found
}

/// `observer.observe(options)`.
///
/// `options.type` and `options.entryTypes` are mutually exclusive; given both
/// or neither, nothing is subscribed (Node throws — see the crate-module doc).
extern "C" fn observe(_e: u64, this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let (single, many, buffered) = entry::with_runtime(|context| {
        let kind = entry::get_member(context, options, "type");
        let single = entry::string_in(context, kind);
        let many = entry::get_member(context, options, "entryTypes");
        let many = entry::is_array_in(context, many).then_some(many);
        // `to_boolean_in` is a coercion, and here that is correct rather than
        // sloppy: `buffered` is documented as a flag and Node tests it for
        // truthiness. The coercion-as-type-test rule is about asking WHAT a
        // value is, which this is not.
        let flag = entry::get_member(context, options, "buffered");
        let buffered = entry::to_boolean_in(context, flag);
        (single, many, buffered)
    });
    // Outside the borrow: `string_elements` reads elements ambiently.
    let types = match (single, many) {
        (Some(one), None) => vec![one],
        (None, Some(array)) => string_elements(array),
        _ => Vec::new(),
    };
    if types.is_empty() {
        return entry::undefined_value();
    }
    let cursor = match buffered {
        true => 0,
        false => timeline::count(),
    };
    OBSERVERS.with(|held| {
        let mut watches = held.borrow_mut();
        match watches.iter_mut().find(|watch| watch.object == this) {
            // Re-observing replaces the subscription rather than adding a
            // second one, which is what Node's single-observer-per-instance
            // model means: two would deliver every entry twice.
            Some(watch) => {
                watch.types = types;
                watch.cursor = cursor;
            }
            None => watches.push(Watch {
                object: this,
                types,
                cursor,
            }),
        }
    });
    entry::undefined_value()
}

/// `observer.disconnect()` — idempotent, because removing a subscription that
/// is not there removes nothing.
extern "C" fn disconnect(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    OBSERVERS.with(|held| held.borrow_mut().retain(|watch| watch.object != this));
    entry::undefined_value()
}

/// Timeline positions this watch has not been told about yet.
fn pending(watch: &Watch) -> Vec<usize> {
    timeline::with_entries(|entries| {
        entries
            .iter()
            .enumerate()
            .skip(watch.cursor)
            .filter(|(_, record)| watch.types.iter().any(|kind| kind == record.kind))
            .map(|(index, _)| index)
            .collect()
    })
}

/// `observer.takeRecords()` — the queued-but-undelivered entries, and the
/// buffer is emptied by the same act (the cursor advances), so an immediate
/// second call answers `[]`.
extern "C" fn take_records(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let indices = OBSERVERS.with(|held| {
        let mut watches = held.borrow_mut();
        let watch = watches.iter_mut().find(|watch| watch.object == this)?;
        let indices = pending(watch);
        watch.cursor = timeline::count();
        Some(indices)
    });
    entry::with_runtime(|context| match indices {
        Some(indices) => timeline::build_indices(context, &indices),
        None => entry::make_array_in(context, Vec::new()),
    })
}

/// Delivers every observer's outstanding entries.
///
/// # The two borrows this function must not hold across a call
///
/// The [`OBSERVERS`] table is copied out and released before anything is
/// built, because a callback may call `observe` or `disconnect` — which would
/// be a nested `RefCell` borrow. And the context borrow is released before
/// [`entry::call`], because the callback is user code whose first act is a
/// runtime call. Both are the same abort: an `extern "C"` frame cannot unwind
/// past the panic.
pub(super) fn source() -> Pending {
    let due: Vec<(u64, Vec<usize>)> = OBSERVERS.with(|held| {
        let mut watches = held.borrow_mut();
        let mark = timeline::count();
        watches
            .iter_mut()
            .filter_map(|watch| {
                let indices = pending(watch);
                watch.cursor = mark;
                (!indices.is_empty()).then_some((watch.object, indices))
            })
            .collect()
    });
    for (object, indices) in due {
        let count = indices.len();
        let prepared = entry::with_runtime(|context| {
            let callback = entry::get_member(context, object, CALLBACK);
            if !entry::is_callable_in(context, callback) {
                return None;
            }
            let prototype = entry::make_prototype(
                context,
                "PerformanceObserverEntryList",
                timeline::LIST_METHODS,
            );
            let list = entry::make_instance(context, prototype);
            let built = timeline::build_indices(context, &indices);
            entry::put_member(context, list, timeline::LIST_HELD, built);
            let length = entry::make_number(count as f64);
            entry::put_member(context, list, timeline::LIST_COUNT, length);
            Some((callback, list))
        });
        let Some((callback, list)) = prepared else {
            continue;
        };
        let absent = entry::undefined_value();
        entry::call(callback, absent, list, object, absent, absent);
    }
    Pending::Idle
}
