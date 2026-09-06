//! `Proxy` — an object whose reads and writes are answered by a handler.
//!
//! # Why nothing in the fast path had to change
//!
//! A compiled property access asks `cache_resolve` for a byte OFFSET and loads
//! at it, and that answer only ever encodes an OWN slot: a cell whose shape does
//! not have the key answers `-1` and the site falls to the entry point. A proxy
//! has no own properties at all, so every access to one structurally misses —
//! which is where the traps live, and why teaching the cache about proxies would
//! have been paying for them in programs that have none.
//!
//! # What is here
//!
//! The lookup every trap starts from, and what a proxy IS: a cell recorded in
//! `Context::proxies` beside the target and the handler it stands for.
//!
//! The traps themselves are each in the file that owns the operation they
//! intercept — the four named-property ones in [`property`], the key and
//! descriptor family in [`keys`], `apply` and `construct` in [`calls`], and the
//! prototype and extensibility pair in [`prototype`]. A handler without the trap
//! forwards to the target, which is the specification's own default and is what
//! makes a partial handler useful rather than a hole.
//!
//! `Reflect.get`, `Reflect.set`, `Reflect.has` and `Reflect.deleteProperty` go
//! through the traps for free: they forward to the same entry points a compiled
//! access reaches, and the interception is in those.
//!
//! # What a handler is not allowed to say
//!
//! [`invariant`] is the layer that asks. It exists because a trap may lie about
//! anything EXCEPT a fact the program could already observe through the target,
//! and it raises the `TypeError` the language raises when one does.
//!
//! # The two rules every trap here obeys
//!
//! 1. **The lookup ends its borrow before the call.** A trap is user code, so it
//!    may call anything, including back into the runtime — the discipline
//!    `get_property` already follows for a getter, and the one a second borrow
//!    inside an `extern "C"` frame punishes by aborting the process rather than
//!    unwinding.
//! 2. **The answer is not read until the throw is.** Rule 8 of this crate's
//!    README: `functions::call` answers `undefined` for a call that did not run,
//!    and `undefined` is a value — so a trap that threw would otherwise have its
//!    non-answer compared against the target and produce a SECOND, invented
//!    `TypeError` on the way out.

mod calls;
mod invariant;
mod keys;
mod property;
mod prototype;

pub(in crate::entry) use calls::{apply, construct};
pub(in crate::entry) use keys::{define, describe, enumerable_keys, own_keys};
pub(in crate::entry) use property::{delete, get, has, set, set_verdict};
pub(in crate::entry) use prototype::{
    extensible, prevent_extensions, prototype_of, set_prototype_verdict,
};

use super::{Context, with_current};
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// `new Proxy(target, handler)`.
#[rtse::class("Proxy")]
impl Proxy {
    /// Records what this proxy stands for.
    ///
    /// The cell is the one the `new` machinery already made, and it keeps no
    /// properties of its own — that is what sends every access to the slow path
    /// where the traps are.
    #[construct]
    fn build(this: u64, target: u64, handler: u64) -> u64 {
        if !objects_only(target, handler) {
            return absent();
        }
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot() {
                context.set_proxy(cell, target, handler);
            }
            this
        })
    }

    /// `Proxy.revocable(target, handler)` — the proxy, and the switch that
    /// turns it off.
    ///
    /// # Where the revoked state is kept
    ///
    /// In the proxy's own record, as a handler that is not an object. Nothing
    /// else can produce that: [`objects_only`] refuses a construction whose
    /// target or handler is not one, which is the language's own rule and is
    /// what makes "recorded as a proxy, and its handler is not an object" mean
    /// revoked and nothing else.
    ///
    /// The alternative was a table beside `Context::proxies` holding one flag
    /// per proxy. It was rejected because it is a second record of one fact:
    /// the collector already traces the pair, `collect_cycle` already drops it,
    /// and a flag in a second table would have to be kept in step with both.
    #[stat]
    fn revocable(target: u64, handler: u64) -> u64 {
        if !objects_only(target, handler) {
            return absent();
        }
        let (made, revoke) = with_current(|context| {
            let Some(cell) = super::native::plain(context) else {
                return (absent_in(context), absent_in(context));
            };
            let proxy = Value::from_slot(cell).bits();
            context.set_proxy(cell, target, handler);
            // The environment slot, for the reason `promise::state::settler`
            // states: a native closes over nothing, and two revoke functions are
            // the same code that must revoke different proxies. It is passed
            // through untouched, it is traced as a value, and no JavaScript can
            // read it.
            let code = revoked as super::native::Native;
            let revoke = super::native::callable(context, code);
            if let Some(cell) = Value(revoke).as_slot() {
                context.mark_callable(cell, code as usize as u64, proxy);
            }
            super::native::name_of(context, revoke, "revoke");
            (proxy, revoke)
        });
        let held = super::objects::object_new(0);
        with_current(|context| {
            if let Some(cell) = Value(held).as_slot() {
                let key = context.well_known("proxy");
                super::objects::put(context, cell, key, made);
                let key = context.well_known("revoke");
                super::objects::put(context, cell, key, revoke);
            }
        });
        held
    }
}

/// `revoke()` — the proxy in this function's environment stops answering.
///
/// The record is kept rather than removed, and that is the whole mechanism: a
/// cell dropped from `Context::proxies` would stop being a proxy and start
/// answering as the empty object it is, where a revoked proxy must refuse every
/// operation. Both halves become `null`, which also lets the collector stop
/// holding the target and the handler alive — which is what makes `revoke` a
/// way to release them rather than only a way to refuse.
extern "C" fn revoked(environment: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(environment).as_slot() {
            let nothing = Value::from_singleton(context.singletons.null).bits();
            context.set_proxy(cell, nothing, nothing);
        }
        super::objects::undefined_of(context)
    })
}

/// Whether both halves of a proxy are objects, raising the `TypeError` if not.
///
/// The language's own rule, and it is load-bearing here beyond conformance:
/// [`Proxy::revocable`] records revocation as a handler that is not an object,
/// and that only means revocation if nothing else can produce it.
fn objects_only(target: u64, handler: u64) -> bool {
    if Value(target).as_slot().is_some() && Value(handler).as_slot().is_some() {
        return true;
    }
    super::throw::type_error("Cannot create proxy with a non-object as target or handler");
    false
}

/// What a trap needs before it can be called.
pub(super) struct Trap {
    /// The handler's function for this operation, absent when the handler does
    /// not define it — which is what forwards to the target.
    pub(super) callee: Option<u64>,
    /// The handler itself, which is what a trap is called with as `this`.
    pub(super) handler: u64,
    /// The object the operation was really about.
    pub(super) target: u64,
    /// Whether the operation must stop without touching the target.
    ///
    /// A throw is already recorded by the time this is true, so the caller
    /// answers its "nothing to say" value and the compiled site above re-raises.
    /// It was named `revoked` and covered one cause; there are three, and they
    /// are the same to every caller: the proxy was revoked, reading the trap off
    /// the handler threw, or the handler defines the trap as something that is
    /// not callable. Forwarding to the target for the last two would answer
    /// where the language raises.
    pub(super) refused: bool,
}

/// Whether this value is a proxy at all, asked WITHOUT running anything.
///
/// [`trap_for`] and the `Option`-answering entry points are the usual way to
/// ask, and they are the wrong way for a caller that has to decide which of two
/// walks to perform: each of them calls a handler, so asking "is this a proxy?"
/// with `own_keys` reports one `ownKeys` the program never wrote. This reads
/// the cell and calls nothing.
pub(in crate::entry) fn is_proxy(object: u64) -> bool {
    with_current(|context| {
        Value(object)
            .as_slot()
            .is_some_and(|cell| context.proxy_at(cell).is_some())
    })
}

/// Looks a trap up, without holding a borrow for the call that follows.
///
/// `None` when the object is not a proxy at all, which is what tells every call
/// site to carry on as it always did.
/// # Why the trap is read through `[[Get]]` and not off the handler's slot
///
/// Because `GetMethod(handler, name)` is what the specification performs, and
/// this read `objects::read_property` — the walk that answers a data property
/// and runs nothing. Two shapes the language allows were therefore invisible:
/// a handler that DEFINES a trap as a getter, and a handler that is itself a
/// Proxy. The second is not a curiosity — it is how a program observes which
/// traps an operation asks for, and it is what
/// `claude2-proxy-handler-trap-name-reads` measures: every trap name came back
/// empty, because the inner proxy's `get` was never reached.
///
/// The read runs user code, so it happens outside the borrow, and a throw from
/// it stops the operation rather than being read as "no such trap" — which
/// would forward to the target and answer where the language raises. That is
/// what [`Trap::refused`] carries.
pub(super) fn trap_for(object: u64, name: &str) -> Option<Trap> {
    let (target, handler, revoked) = with_current(|context| {
        let cell = Value(object).as_slot()?;
        let (target, handler) = context.proxy_at(cell)?;
        // Revoked — see [`Proxy::revocable`] for why a handler that is not an
        // object is what revocation means.
        Some((target, handler, Value(handler).as_slot().is_none()))
    })?;
    if revoked {
        super::throw::type_error(&format!(
            "Cannot perform '{name}' on a proxy that has been revoked"
        ));
        return Some(Trap {
            callee: None,
            handler,
            target,
            refused: true,
        });
    }
    let key = with_current(|context| context.well_known_text(name));
    let found = super::computed::get_indexed(handler, key);
    // Rule 8: the read may have run a getter or an outer proxy's own `get`.
    if super::throw::in_flight() {
        return Some(Trap {
            callee: None,
            handler,
            target,
            refused: true,
        });
    }
    // `GetMethod` treats `undefined` and `null` as absent and refuses anything
    // else that is not callable — a handler whose `get` is `3` is malformed, and
    // forwarding to the target instead would answer for it.
    let (nullish, callable) = with_current(|context| {
        (
            super::objects::nullish(context, found).is_some(),
            super::modules::is_callable_in(context, found),
        )
    });
    if !nullish && !callable {
        super::throw::type_error(&format!("proxy handler's '{name}' is not a function"));
        return Some(Trap {
            callee: None,
            handler,
            target,
            refused: true,
        });
    }
    Some(Trap {
        callee: (!nullish).then_some(found),
        handler,
        target,
        refused: false,
    })
}

/// The property, as the string a trap is handed.
///
/// A key is a NUMBER everywhere else, because that is the point of numbering it
/// — but a trap is user code and reads `prop` as a string, so this is the one
/// direction the interner has to answer.
pub(super) fn property_of(key: Key) -> u64 {
    with_current(|context| {
        let text = Str::from_str(&spelled_in(context, key));
        context.intern_value(text).bits()
    })
}

/// The property, as the text an invariant's message names it by.
pub(super) fn spelled(key: Key) -> String {
    with_current(|context| spelled_in(context, key))
}

/// [`spelled`] from a context already in hand.
fn spelled_in(context: &Context, key: Key) -> String {
    match key {
        Key::Index(index) => index.to_string(),
        // A key nothing interned cannot happen through a source property — the
        // compiler mints from the same registry and the host seeds the texts —
        // so this is the empty string rather than a panic in a runtime that
        // would take the process with it.
        Key::Name(named) => context
            .interner
            .text(named)
            .and_then(Str::to_rust)
            .unwrap_or_default(),
    }
}

/// `undefined`, for a path with no answer to give.
pub(super) fn absent() -> u64 {
    with_current(|context| super::objects::undefined_of(context))
}

/// [`absent`] from a context already in hand.
fn absent_in(context: &Context) -> u64 {
    super::objects::undefined_of(context)
}
