//! `Event` and `CustomEvent` — the value a dispatch carries.
//!
//! # Why the state is data properties and not internal slots
//!
//! Because a host outside `rts-core` has no way to make anything else. The
//! attribute that builds a class with getters, `#[rtse::class]`, expands at
//! compile time inside that crate and cannot see an `impl` block out here, so
//! what is available is `make_prototype` plus ordinary properties. The cost is
//! named in this module tree's doc: every one of these is writable where the
//! specification makes it read-only.
//!
//! # Why `stopPropagation` does nothing, and why that is not a stub
//!
//! Node's does nothing either. `dispatchEvent` here — and there — delivers to
//! exactly one target and never walks a tree, so there is no propagation for it
//! to stop; `events.md` §4 states it as an inert API-shape member. A no-op that
//! matches the reference is an implementation, not a fake, and the member that
//! *does* have an effect in Node — `stopImmediatePropagation` — has one here.

use rts_core::entry::{self, Context, Provided};

/// What `EventInit` carries, read once at construction.
///
/// A struct rather than three `bool` parameters: the three are the same type in
/// a fixed order, and a call site that transposed two of them would compile and
/// be wrong at run time in a way no test names.
pub(super) struct Flags {
    /// `options.bubbles` — inert here and in Node; there is no tree.
    pub(super) bubbles: bool,
    /// `options.cancelable` — the one flag with an effect: it gates
    /// `preventDefault`.
    pub(super) cancelable: bool,
    /// `options.composed` — inert, as in Node.
    pub(super) composed: bool,
}

impl Flags {
    /// Reads the three from an options argument.
    ///
    /// Outside any borrow, by construction: each lookup is an ambient entry
    /// point that takes and releases the runtime borrow itself.
    fn of(options: u64) -> Self {
        Self {
            bubbles: super::option_flag(options, "bubbles"),
            cancelable: super::option_flag(options, "cancelable"),
            composed: super::option_flag(options, "composed"),
        }
    }

    /// The three an engine-generated event is built with: all `false`.
    pub(super) const INERT: Self = Self {
        bubbles: false,
        cancelable: false,
        composed: false,
    };
}

const METHODS: &[(&str, Provided)] = &[
    ("preventDefault", prevent_default),
    ("stopPropagation", stop_propagation),
    ("stopImmediatePropagation", stop_immediate_propagation),
];

/// The one `Event.prototype`, made on first ask.
///
/// `make_prototype` is idempotent by name, so every caller here — the
/// constructor, `CustomEvent`'s link, `AbortSignal`'s `'abort'` event — reaches
/// the same object. That sharing is the whole reason it is asked for by name
/// rather than held in a static: `e instanceof Event` and a method a program
/// adds to `Event.prototype` both depend on there being exactly one.
pub(super) fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Event", METHODS)
}

/// Builds the `Event` class and answers its constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_ctor(context, construct, prototype)
}

/// Builds the `CustomEvent` class and answers its constructor.
pub(super) fn install_custom(context: &mut Context) -> u64 {
    let prototype = custom_prototype(context);
    super::class_ctor(context, construct_custom, prototype)
}

/// `CustomEvent.prototype`, linked to `Event.prototype`.
///
/// Nothing is installed on it: `preventDefault` and the rest are found by the
/// ordinary chain walk, which is what makes `custom instanceof Event` true for
/// the same reason and by the same mechanism.
fn custom_prototype(context: &mut Context) -> u64 {
    let base = prototype(context);
    let made = entry::make_prototype(context, "CustomEvent", &[]);
    entry::set_prototype_in(context, made, base);
    made
}

/// `new Event(type, options?)`.
extern "C" fn construct(_e: u64, this: u64, kind: u64, options: u64, _c: u64, _d: u64) -> u64 {
    // Both reads are ambient entry points and must finish before the borrow
    // below opens: a second borrow inside the first aborts the process, and an
    // `extern "C"` frame cannot unwind to report it.
    let text = entry::text_of(kind).unwrap_or_default();
    let flags = Flags::of(options);
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let event = super::self_or_new(context, this, prototype);
        init(context, event, &text, &flags);
        event
    })
}

/// `new CustomEvent(type, options?)` — `Event` plus `detail`, defaulting to
/// `null` rather than `undefined`, which is what the specification says and what
/// a program checking `if (e.detail === null)` reads.
extern "C" fn construct_custom(
    _e: u64,
    this: u64,
    kind: u64,
    options: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let text = entry::text_of(kind).unwrap_or_default();
    let flags = Flags::of(options);
    let detail = detail_of(options);
    entry::with_runtime(|context| {
        let prototype = custom_prototype(context);
        let event = super::self_or_new(context, this, prototype);
        init(context, event, &text, &flags);
        entry::put_member(context, event, "detail", detail);
        event
    })
}

/// `options.detail`, or `null`.
fn detail_of(options: u64) -> u64 {
    let held = super::options_bag(options).map(|bag| super::get(bag, "detail"));
    match held {
        Some(value) if value != super::absent() => value,
        _ => entry::null_value(),
    }
}

/// Writes every property a freshly constructed event carries.
///
/// One function for both constructors and for the engine-generated `'abort'`
/// event, because three places writing the same eight properties is three
/// chances for one of them to forget `defaultPrevented` — and an event missing
/// it reads as `undefined`, which is falsy, so the omission would look correct
/// until a listener called `preventDefault`.
pub(super) fn init(context: &mut Context, event: u64, kind: &str, flags: &Flags) {
    let text = entry::make_string(context, kind);
    entry::put_member(context, event, "type", text);
    let pairs = [
        ("bubbles", flags.bubbles),
        ("cancelable", flags.cancelable),
        ("composed", flags.composed),
        ("defaultPrevented", false),
        ("isTrusted", false),
        (STOPPED, false),
    ];
    for (name, held) in pairs {
        entry::put_member(context, event, name, entry::boolean_value(held));
    }
    let nothing = entry::null_in(context);
    entry::put_member(context, event, "target", nothing);
    entry::put_member(context, event, "currentTarget", nothing);
    entry::put_member(context, event, "eventPhase", entry::make_number(NONE));
}

/// The property `stopImmediatePropagation` sets and a dispatch reads.
///
/// Underscored because it is bookkeeping a program should not need, and named as
/// a constant because both sides must agree on the spelling — a typo in one of
/// them is a listener loop that never stops early, which nothing would report.
pub(super) const STOPPED: &str = "__stopImmediate__";

/// `Event.NONE` — not dispatching.
pub(super) const NONE: f64 = 0.0;

/// `Event.AT_TARGET` — the only phase this engine has, as in Node.
pub(super) const AT_TARGET: f64 = 2.0;

/// `event.preventDefault()` — a no-op unless the event is cancelable, exactly as
/// specified.
extern "C" fn prevent_default(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if super::flag(this, "cancelable") {
        super::put(this, "defaultPrevented", entry::boolean_value(true));
    }
    super::absent()
}

/// `event.stopPropagation()` — inert, as in Node; see the module doc.
extern "C" fn stop_propagation(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::absent()
}

/// `event.stopImmediatePropagation()` — the remaining listeners for this
/// dispatch do not run.
extern "C" fn stop_immediate_propagation(
    _e: u64,
    this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    super::put(this, STOPPED, entry::boolean_value(true));
    super::absent()
}
