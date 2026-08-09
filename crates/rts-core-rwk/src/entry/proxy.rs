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
//! # What is here, and what is not
//!
//! `get`, `set`, `has` and `deleteProperty`. A handler without the trap forwards
//! to the target, which is what the specification's default behaviour is and is
//! also what makes a partial handler useful rather than a hole.
//!
//! Not here: `apply`, `construct`, `ownKeys`, `getOwnPropertyDescriptor`,
//! `defineProperty` and the rest. They are not "not written yet" in the same
//! sense — `ownKeys` needs enumeration to ask the runtime rather than read a
//! shape, and `defineProperty` needs descriptors, which `Reflect` does not have
//! either (`reflect.rs` says so). Each is a piece of its own.
//!
//! `Reflect.get`, `Reflect.set`, `Reflect.has` and `Reflect.deleteProperty` go
//! through the traps for free: they forward to the same entry points a compiled
//! access reaches, and the interception is in those.
//!
//! # The invariant a trap must not break
//!
//! A trap is user code, so it may call anything, including back into the
//! runtime. Every one of these therefore looks the trap up under a borrow, ENDS
//! the borrow, and only then calls — the discipline `get_property` already
//! follows for a getter, and the one a second borrow inside an `extern "C"`
//! frame punishes by aborting the process rather than unwinding.

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
        with_current(|context| {
            if let Some(cell) = Value(this).as_slot() {
                context.set_proxy(cell, target, handler);
            }
            this
        })
    }
}

/// What a trap needs before it can be called: the function, and what to pass it.
struct Trap {
    /// The handler's function, absent when the handler does not define it.
    callee: Option<u64>,
    /// The object the operation was really about.
    target: u64,
    /// The property, as the string a trap receives.
    property: u64,
}

/// Looks a trap up, without holding a borrow for the call that follows.
///
/// `None` when the object is not a proxy at all, which is what tells every call
/// site to carry on as it always did.
fn trap_for(object: u64, name: &str, key: Key) -> Option<Trap> {
    with_current(|context| {
        let cell = Value(object).as_slot()?;
        let (target, handler) = context.proxy_at(cell)?;
        let property = key_as_text(context, key);
        let callee = Value(handler).as_slot().and_then(|handler| {
            let named = context.well_known(name);
            super::objects::read_property(context, handler, named)
                .map(|found| found.bits())
                .filter(|&found| Value(found).as_slot().is_some())
        });
        Some(Trap {
            callee,
            target,
            property,
        })
    })
}

/// The property, as the string a trap is handed.
///
/// A key is a NUMBER everywhere else, because that is the point of numbering it
/// — but a trap is user code and reads `prop` as a string, so this is the one
/// direction the interner has to answer.
fn key_as_text(context: &mut Context, key: Key) -> u64 {
    let text = match key {
        Key::Index(index) => Str::from_str(&index.to_string()),
        Key::Name(named) => match context.interner.text(named) {
            Some(text) => text.clone(),
            // A key nothing interned cannot happen through a source property —
            // the compiler mints from the same registry and the host seeds the
            // texts — so this is the empty string rather than a panic in a
            // runtime that would take the process with it.
            None => Str::from_str(""),
        },
    };
    context.intern_value(text).bits()
}

/// `handler.get(target, prop, receiver)`, or the target's own answer.
pub(super) fn get(object: u64, key: Key) -> Option<u64> {
    let trap = trap_for(object, "get", key)?;
    Some(match trap.callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, absent, trap.target, trap.property, object, absent)
        }
        None => forwarded_read(trap.target, key),
    })
}

/// `handler.set(target, prop, value, receiver)`, or a write to the target.
///
/// Answers the value, because an assignment is an expression — the trap's own
/// answer is a success flag the specification only reads in strict mode, and
/// reporting it as the assignment's value would make `p.x = 1` evaluate to
/// `true`.
pub(super) fn set(object: u64, key: Key, value: u64) -> Option<u64> {
    let trap = trap_for(object, "set", key)?;
    match trap.callee {
        Some(callee) => {
            super::functions::call(callee, object, trap.target, trap.property, value, object);
        }
        // A target that is itself a proxy gets its own traps, for the reason
        // `forwarded_read` states.
        None if set(trap.target, key, value).is_some() => {}
        None => {
            with_current(|context| {
                if let Some(cell) = Value(trap.target).as_slot() {
                    super::objects::put(context, cell, key, value);
                }
            });
        }
    }
    Some(value)
}

/// `handler.has(target, prop)`, or whether the target has it.
pub(super) fn has(object: u64, key: Key) -> Option<bool> {
    let trap = trap_for(object, "has", key)?;
    Some(match trap.callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            let answered =
                super::functions::call(callee, object, trap.target, trap.property, absent, absent);
            super::primitives::to_boolean(answered)
        }
        None => match has(trap.target, key) {
            Some(answered) => answered,
            None => with_current(|context| {
                Value(trap.target)
                    .as_slot()
                    .and_then(|cell| super::objects::read_property(context, cell, key))
                    .is_some()
            }),
        },
    })
}

/// `handler.deleteProperty(target, prop)`, or a delete on the target.
pub(super) fn delete(object: u64, key: Key) -> Option<bool> {
    let trap = trap_for(object, "deleteProperty", key)?;
    Some(match trap.callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            let answered =
                super::functions::call(callee, object, trap.target, trap.property, absent, absent);
            super::primitives::to_boolean(answered)
        }
        // Forwarded through the entry point rather than reimplemented: deleting
        // rebuilds the shape without the key, and a second spelling of that is
        // a second answer to where a property lives.
        None => {
            let property = with_current(|context| key_as_text(context, key));
            super::computed::delete_property(trap.target, property)
        }
    })
}

/// Reads through to the target, for a handler that does not trap the read.
///
/// The target may itself be a proxy — `new Proxy(new Proxy(x, inner), {})` is
/// what a wrapper around a wrapper is — so this asks the traps again before
/// reading a shape. Without it the innermost handler was skipped and the read
/// answered `undefined`, which is what a chain of three proxies reported.
fn forwarded_read(target: u64, key: Key) -> u64 {
    if let Some(answered) = get(target, key) {
        return answered;
    }
    let found = with_current(|context| {
        Value(target)
            .as_slot()
            .and_then(|cell| super::objects::read_property(context, cell, key))
            .map(|value| value.bits())
    });
    match found {
        Some(value) => value,
        None => with_current(|context| super::objects::undefined_of(context)),
    }
}

/// A trap that takes only the target, looked up without a live borrow.
///
/// `get`/`set`/`has`/`delete` all name a property and this family does not, so
/// they share [`trap_for`]'s lookup without its `property`. Kept separate rather
/// than passing a placeholder key: a placeholder is a value a later reader has
/// to know is meaningless.
fn plain_trap(object: u64, name: &str) -> Option<(Option<u64>, u64)> {
    with_current(|context| {
        let cell = Value(object).as_slot()?;
        let (target, handler) = context.proxy_at(cell)?;
        let callee = Value(handler).as_slot().and_then(|handler| {
            let named = context.well_known(name);
            super::objects::read_property(context, handler, named)
                .map(|found| found.bits())
                .filter(|&found| Value(found).as_slot().is_some())
        });
        Some((callee, target))
    })
}

/// `handler.ownKeys(target)`, or the target's own keys.
///
/// What `Object.keys`, `for`-`in` and spread all reach. The forwarding case is
/// the target's own keys and not the proxy's, which have never existed: a proxy
/// cell holds no properties at all.
pub(super) fn own_keys(object: u64) -> Option<u64> {
    let (callee, target) = plain_trap(object, "ownKeys")?;
    Some(match callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, object, target, absent, absent, absent)
        }
        None => super::array::own_keys(target),
    })
}

/// `handler.getPrototypeOf(target)`, or the target's prototype.
pub(super) fn prototype_of(object: u64) -> Option<u64> {
    let (callee, target) = plain_trap(object, "getPrototypeOf")?;
    Some(match callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, object, target, absent, absent, absent)
        }
        None => super::chain::get_prototype(target),
    })
}

/// `handler.setPrototypeOf(target, proto)`, or a write to the target's.
pub(super) fn set_prototype_of(object: u64, prototype: u64) -> Option<u64> {
    let (callee, target) = plain_trap(object, "setPrototypeOf")?;
    match callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, object, target, prototype, absent, absent);
        }
        None => {
            super::chain::set_prototype(target, prototype);
        }
    }
    // The proxy, because `Object.setPrototypeOf` answers what it was given and
    // the caller wrote the proxy.
    Some(object)
}

/// `handler.apply(target, thisArg, args)`, or a call to the target.
///
/// A proxy is not callable in the sense `resolve` means — it has no code
/// address, and it must not: the address is what a program cannot be allowed to
/// choose. So calling one arrives here, at the point `invoke` finds nothing to
/// jump to, rather than at a check before every jump.
///
/// The arguments reach the trap as an ARRAY, which is what the specification
/// hands it. Four of them, the convention's own count, and a call written with
/// more already reaches `call_with_args`.
pub(super) fn apply(object: u64, this: u64, arguments: [u64; 4]) -> Option<u64> {
    let (callee, target) = plain_trap(object, "apply")?;
    Some(match callee {
        Some(callee) => {
            let listed = super::modules::make_array(arguments.to_vec());
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, object, target, this, listed, absent)
        }
        // The target may be a proxy of its own, and `call` is what asks that
        // question again — this is the forwarding every absent trap does.
        None => super::functions::call(
            target,
            this,
            arguments[0],
            arguments[1],
            arguments[2],
            arguments[3],
        ),
    })
}

/// `handler.construct(target, args, newTarget)`, or `new` on the target.
pub(super) fn construct(object: u64, arguments: [u64; 4]) -> Option<u64> {
    let (callee, target) = plain_trap(object, "construct")?;
    Some(match callee {
        Some(callee) => {
            let listed = super::modules::make_array(arguments.to_vec());
            super::functions::call(callee, object, target, listed, object, object)
        }
        None => super::functions::construct(
            target,
            arguments[0],
            arguments[1],
            arguments[2],
            arguments[3],
        ),
    })
}

/// `handler.defineProperty(target, prop, descriptor)`, or a define on the target.
///
/// Answers whether it was accepted, which is what `Reflect.defineProperty`
/// reports and what a trap returning `false` means. The forwarding case answers
/// true whenever it reached an object, the same thing `Reflect.set` can
/// establish and for the same reason: a refusal would be a non-configurable
/// property, and that is recorded per object rather than per key.
pub(super) fn define(object: u64, key: Key, descriptor: u64) -> Option<bool> {
    let trap = trap_for(object, "defineProperty", key)?;
    Some(match trap.callee {
        Some(callee) => {
            let answered = super::functions::call(
                callee,
                object,
                trap.target,
                trap.property,
                descriptor,
                descriptor,
            );
            super::primitives::to_boolean(answered)
        }
        None => {
            super::object_global::define(trap.target, trap.property, descriptor);
            Value(trap.target).as_slot().is_some()
        }
    })
}

/// `handler.getOwnPropertyDescriptor(target, prop)`, or the target's own.
pub(super) fn describe(object: u64, key: Key) -> Option<u64> {
    let trap = trap_for(object, "getOwnPropertyDescriptor", key)?;
    Some(match trap.callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            super::functions::call(callee, object, trap.target, trap.property, absent, absent)
        }
        None => super::object_global::describe_own(trap.target, trap.property),
    })
}

/// Whether a `setPrototypeOf` was accepted, for the caller that reports it.
///
/// Beside [`set_prototype_of`] rather than replacing it, because the two answer
/// different questions to different callers: `Object.setPrototypeOf` answers the
/// object it was given, and `Reflect.setPrototypeOf` answers whether it worked.
/// A handler that returns `false` is a refusal, and reporting success because
/// the call reached an object would be inventing the answer.
pub(super) fn set_prototype_verdict(object: u64, prototype: u64) -> Option<bool> {
    let (callee, target) = plain_trap(object, "setPrototypeOf")?;
    Some(match callee {
        Some(callee) => {
            let absent = with_current(|context| super::objects::undefined_of(context));
            let answered =
                super::functions::call(callee, object, target, prototype, absent, absent);
            super::primitives::to_boolean(answered)
        }
        None => {
            super::chain::set_prototype(target, prototype);
            Value(target).as_slot().is_some()
        }
    })
}

/// The keys `Object.keys` reports for a proxy: its own keys, filtered.
///
/// `Reflect.ownKeys` answers what the trap said and nothing else, and
/// `Object.keys` answers only the ENUMERABLE ones — so it asks
/// `[[GetOwnProperty]]` per key, which on a proxy is the
/// `getOwnPropertyDescriptor` trap or a forward to the target. A key the trap
/// invented that the target does not have has no descriptor, so it is not
/// enumerable, so it is dropped: `ownKeys: () => ["only"]` over `{a, b}` gives
/// `Object.keys` an empty list, which is what every other engine answers.
///
/// This is the one place the two spellings of enumeration legitimately differ,
/// and the difference is the filter rather than a second walk.
pub(super) fn enumerable_keys(object: u64) -> Option<u64> {
    let listed = own_keys(object)?;
    let keys: Vec<u64> = with_current(|context| {
        match Value(listed).as_slot().and_then(|cell| context.elements_at(cell)) {
            Some(elements) => elements.to_vec(),
            None => Vec::new(),
        }
    });

    let mut kept = Vec::with_capacity(keys.len());
    for key in keys {
        // Through the public spelling, so a proxy whose target is a proxy is
        // asked again — the forwarding every absent trap does.
        let described = super::object_global::describe_of(object, key);
        let enumerable = with_current(|context| {
            let named = context.well_known("enumerable");
            Value(described)
                .as_slot()
                .and_then(|cell| super::objects::read_property(context, cell, named))
                .map(|found| found.bits())
        });
        if let Some(enumerable) = enumerable
            && super::primitives::to_boolean(enumerable)
        {
            kept.push(key);
        }
    }
    Some(super::modules::make_array(kept))
}
