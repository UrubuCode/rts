//! `_readableState` / `_writableState` — the internal state objects real Node
//! exposes on every stream, and that library code reads directly.
//!
//! # Why a crate full of public properties needs these at all
//!
//! Everything here is already tracked, under the PUBLIC name: `readableEnded`,
//! `readableLength`, `writableFinished`, and the rest (`common.rs`'s doc states
//! the plain-data-property convention). What was missing is the name real
//! libraries use. `ws` 8.22 reads `socket._readableState.endEmitted` and
//! `socket._readableState.length` in `socketOnClose`, and
//! `receiver._writableState.errorEmitted`/`.finished` right after — so closing a
//! WebSocket died with `Cannot read properties of undefined (reading
//! 'endEmitted')` on a socket that was otherwise complete. The failure is at
//! CLOSE, which is why a stream suite can be green while every real WebSocket
//! client crashes on its last breath.
//!
//! # A computed view, not a second copy
//!
//! Each name is an accessor on the prototype that builds a fresh object from the
//! public properties on every read. Rejected: a stored `_readableState` object
//! kept in sync beside the public properties — that is two tables for one
//! question (`reuse-check` section 3), and every future `set_bool(…,
//! "readableEnded", …)` site would have to remember the second one. The cost of
//! the view is that WRITES through it are not observed: `rs.length = 0` changes
//! nothing. Node's own internals write there, library code does not, and the
//! three readers in `ws` are reads. When a program is found that writes, the
//! answer is a setter per field on the view, not a stored copy.
//!
//! `common.rs` says accessor machinery is not reachable from a hand-written
//! native module here. That was stale: `entry::define_accessor_in` takes the
//! `&mut Context` every one of these prototype builders already holds.
//!
//! # Which side goes on which prototype
//!
//! Measured against Node 22.23.2, not assumed: a `Readable` has NO
//! `_writableState` and a `Writable` has NO `_readableState` — both are
//! `undefined` — while a `Duplex`, a `net.Socket` and a `tls.TLSSocket` carry
//! both. `ws`'s `Receiver` and `Sender` extend `Writable`, so they inherit the
//! writable half from that prototype.

use std::cell::RefCell;
use std::collections::HashSet;

use rts_core::entry::{self, Context};

use super::common::{get_bool, get_num, get_value};

/// Which halves a prototype's instances have.
#[derive(Clone, Copy)]
pub(crate) enum Sides {
    /// A `Readable`: the readable half alone.
    Readable,
    /// A `Writable`: the writable half alone.
    Writable,
    /// A `Duplex`, and every socket: both.
    Both,
}

thread_local! {
    /// Prototype names already carrying the accessors.
    ///
    /// Every `prototype()` builder in this crate is called again on each
    /// `construct`, and `entry::make_prototype` memoises by name — so without
    /// this, each construction would redefine the accessor and invalidate the
    /// property caches of a prototype that already had it. `&'static str` keys
    /// and no JS values, so there is nothing here for the collector to trace
    /// (`rts-core/README.md` rule 10).
    static INSTALLED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
}

/// Installs the state-view accessors on `prototype`, once per prototype name.
pub(crate) fn install(context: &mut Context, prototype: u64, name: &'static str, sides: Sides) {
    let fresh = INSTALLED.with(|set| set.borrow_mut().insert(name));
    if !fresh {
        return;
    }
    if matches!(sides, Sides::Readable | Sides::Both) {
        entry::define_accessor_in(context, prototype, "_readableState", readable_state, None);
    }
    if matches!(sides, Sides::Writable | Sides::Both) {
        entry::define_accessor_in(context, prototype, "_writableState", writable_state, None);
    }
}

/// `stream._readableState` — the readable half, as Node names its fields.
///
/// `endEmitted` is `readableEnded`: Node sets both when `'end'` has fired.
/// `ended` is the push-side flag (`push(null)` seen), which this crate keeps as
/// `__ended__` — the two differ in Node too, and collapsing them would make
/// `socketOnClose` read a close as an already-emitted end.
extern "C" fn readable_state(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Every property is read BEFORE the borrow is taken: `get_bool`/`get_num`
    // each take the runtime borrow themselves, so reading one inside
    // `with_runtime` panicked with `RefCell already borrowed` on the first call
    // (`rts-core/README.md` rule 4, and the panic is why this is written in two
    // phases rather than inline).
    let fields: [(&str, u64); 4] = [
        ("endEmitted", entry::boolean_value(get_bool(this, "readableEnded"))),
        ("ended", entry::boolean_value(get_bool(this, "__ended__"))),
        ("objectMode", entry::boolean_value(get_bool(this, "readableObjectMode"))),
        ("errored", get_value(this, "errored")),
    ];
    let length = get_num(this, "readableLength");
    let high_water_mark = get_num(this, "readableHighWaterMark");
    let flowing = get_value(this, "readableFlowing");
    let encoding = get_value(this, "readableEncoding");
    let destroyed = entry::boolean_value(get_bool(this, "destroyed"));
    entry::with_runtime(|context| {
        let view = entry::make_object(context);
        for (name, held) in fields {
            entry::put_member(context, view, name, held);
        }
        let length = entry::make_number(length);
        entry::put_member(context, view, "length", length);
        let high_water_mark = entry::make_number(high_water_mark);
        entry::put_member(context, view, "highWaterMark", high_water_mark);
        entry::put_member(context, view, "flowing", flowing);
        entry::put_member(context, view, "encoding", encoding);
        entry::put_member(context, view, "destroyed", destroyed);
        view
    })
}

/// `stream._writableState` — the writable half, as Node names its fields.
///
/// `errorEmitted` is derived from `errored` rather than tracked separately:
/// this crate has one error slot and `ws` reads the flag only to decide whether
/// a close already reported a failure. A stream that errored and has not yet
/// emitted reads `true` here where Node would read `false` — named rather than
/// hidden, and no reader in `ws` can tell the difference, since both arms end
/// the receiver.
extern "C" fn writable_state(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Read before the borrow — same two-phase shape [`readable_state`] documents.
    let errored = get_value(this, "errored");
    // `to_boolean` rather than a comparison against `null_in`/`undefined`: both of
    // those read the live context, and taking that borrow inside the one
    // `with_runtime` below is the panic [`readable_state`] records. `null` and
    // `undefined` are the only falsy values this slot ever holds.
    let error_emitted = entry::boolean_value(entry::to_boolean(errored));
    let fields: [(&str, u64); 6] = [
        ("finished", entry::boolean_value(get_bool(this, "writableFinished"))),
        ("ended", entry::boolean_value(get_bool(this, "writableEnded"))),
        ("needDrain", entry::boolean_value(get_bool(this, "writableNeedDrain"))),
        ("objectMode", entry::boolean_value(get_bool(this, "writableObjectMode"))),
        ("destroyed", entry::boolean_value(get_bool(this, "destroyed"))),
        ("closed", entry::boolean_value(get_bool(this, "closed"))),
    ];
    let length = get_num(this, "writableLength");
    let corked = get_num(this, "writableCorked");
    let high_water_mark = get_num(this, "writableHighWaterMark");
    entry::with_runtime(|context| {
        let view = entry::make_object(context);
        for (name, held) in fields {
            entry::put_member(context, view, name, held);
        }
        entry::put_member(context, view, "errorEmitted", error_emitted);
        entry::put_member(context, view, "errored", errored);
        let length = entry::make_number(length);
        entry::put_member(context, view, "length", length);
        let corked = entry::make_number(corked);
        entry::put_member(context, view, "corked", corked);
        let high_water_mark = entry::make_number(high_water_mark);
        entry::put_member(context, view, "highWaterMark", high_water_mark);
        view
    })
}
