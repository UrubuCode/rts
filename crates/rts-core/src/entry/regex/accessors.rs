//! What a regular expression says about ITSELF, and where those answers live.
//!
//! # Why these are accessors on the prototype and not properties on the object
//!
//! They were properties, written into every regular expression by
//! [`super::describe`], and that is wrong in four ways a program sees:
//!
//! - `RegExp.prototype.source` is `"(?:)"` and `RegExp.prototype.flags` is `""`.
//!   With the answers on the INSTANCE the prototype had neither, so
//!   `Object.getOwnPropertyDescriptor(RegExp.prototype, "flags").get` read
//!   `.get` off `undefined` and killed the program on the first line that
//!   introspects.
//! - `Object.keys(/a/)` answered ten names. The language says a regular
//!   expression has exactly ONE own property, `lastIndex`.
//! - `re.source = "x"` wrote, and then `re.exec` searched for something else. An
//!   accessor with no setter refuses, which is what the language says.
//! - `RegExp.prototype.source.call({})` is a `TypeError`, and there was nothing
//!   to call.
//!
//! The reason it was data in the first place is [`super::describe`]'s: a value
//! only the slow path knows about is one the fast path disagrees with once
//! `cached_get` warms. An accessor does not have that problem, and for the
//! inverse reason — `super::super::accessor` keeps an accessor OUT of the
//! layout, so `cache_resolve` answers negative and every read reaches the
//! runtime by construction. The same argument `Map.prototype.size` was moved
//! under.
//!
//! # Why the receiver has three cases and not two
//!
//! `RegExp.prototype` is itself not a regular expression — it has no compiled
//! pattern — and it is the one non-pattern receiver these getters ANSWER for,
//! with `undefined` for each flag and `"(?:)"` for the source. Everything else
//! is a `TypeError`. Collapsing the prototype into "not a pattern" makes
//! `String(RegExp.prototype)` throw, which no runtime does; collapsing it into
//! "a pattern" makes `({}).source` answer instead of throwing.

use super::super::native::Native;
use super::super::objects::{read_property, undefined_of};
use super::super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// The eight boolean flags, each with the letter it contributes to `flags`.
///
/// One table rather than eight functions and a ninth list: the ORDER here is the
/// order `RegExp.prototype.flags` spells them in, so the getters and the string
/// cannot come to disagree about which letter goes where.
const FLAGS: [(&str, char); 8] = [
    ("hasIndices", 'd'),
    ("global", 'g'),
    ("ignoreCase", 'i'),
    ("multiline", 'm'),
    ("dotAll", 's'),
    ("unicode", 'u'),
    ("unicodeSets", 'v'),
    ("sticky", 'y'),
];

/// Hangs every one of them on `RegExp.prototype`.
pub(super) fn install(context: &mut Context, prototype: u32) {
    super::super::native::getter(context, prototype, "source", source);
    super::super::native::getter(context, prototype, "flags", flags);
    for (name, letter) in FLAGS {
        super::super::native::getter(context, prototype, name, code_for(letter));
    }
}

/// Which of the three receivers a getter was called on.
///
/// Decided inside the borrow and acted on outside it, because the third arm
/// RAISES and building an error borrows the context again — rule 8's shape, and
/// the re-entrant `RefCell` that shape exists to keep out of an `extern "C"`
/// frame.
enum Receiver {
    /// A genuine pattern: what it was compiled from, and with.
    Pattern(String, String),
    /// `RegExp.prototype` itself.
    Prototype,
    /// Anything else — a `TypeError`.
    Foreign,
}

/// Which one `this` is.
fn receiver(context: &Context, this: u64) -> Receiver {
    let Some(cell) = Value(this).as_slot() else {
        return Receiver::Foreign;
    };
    if let Some(pattern) = context.regexp_at(cell) {
        return Receiver::Pattern(pattern.source().to_owned(), pattern.flags().to_owned());
    }
    match context.regexp_prototype == Some(this) {
        true => Receiver::Prototype,
        false => Receiver::Foreign,
    }
}

/// The `TypeError` a foreign receiver owes, raised after the borrow has ended.
fn refuse(name: &str) -> u64 {
    super::super::throw::type_error(&format!(
        "RegExp.prototype.{name} getter called on non-RegExp object"
    ));
    with_current(|context| undefined_of(context))
}

/// The getter for one letter.
///
/// A `match` over the letter rather than a function pointer in [`FLAGS`]: an
/// `extern "C"` function per flag is what the accessor machinery takes, and
/// there is nothing to close the letter over — `super::super::native` states
/// that a native closes over nothing as the reason its environment is
/// `undefined`.
fn code_for(letter: char) -> Native {
    match letter {
        'd' => has_indices,
        'g' => global,
        'i' => ignore_case,
        'm' => multiline,
        's' => dot_all,
        'u' => unicode,
        'v' => unicode_sets,
        _ => sticky,
    }
}

/// Whether the receiver's own pattern carries `letter`.
///
/// `undefined` — not `false` — for `RegExp.prototype`, which is what separates
/// the prototype from an ordinary pattern that simply lacks the flag.
fn flag_of(this: u64, letter: char, name: &str) -> u64 {
    let answered = with_current(|context| match receiver(context, this) {
        Receiver::Pattern(_, letters) => Some(Value::from_bool(letters.contains(letter)).bits()),
        Receiver::Prototype => Some(undefined_of(context)),
        Receiver::Foreign => None,
    });
    match answered {
        Some(value) => value,
        None => refuse(name),
    }
}

macro_rules! flag_getter {
    ($function:ident, $letter:literal, $name:literal) => {
        extern "C" fn $function(
            _environment: u64,
            this: u64,
            _a0: u64,
            _a1: u64,
            _a2: u64,
            _a3: u64,
        ) -> u64 {
            flag_of(this, $letter, $name)
        }
    };
}

flag_getter!(has_indices, 'd', "hasIndices");
flag_getter!(global, 'g', "global");
flag_getter!(ignore_case, 'i', "ignoreCase");
flag_getter!(multiline, 'm', "multiline");
flag_getter!(dot_all, 's', "dotAll");
flag_getter!(unicode, 'u', "unicode");
flag_getter!(unicode_sets, 'v', "unicodeSets");
flag_getter!(sticky, 'y', "sticky");

/// `re.source` — the pattern, escaped so it round-trips between slashes.
extern "C" fn source(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let written = with_current(|context| source_text(context, this));
    match written {
        Some(text) => with_current(|context| context.intern_value(Str::from_str(&text)).bits()),
        None => refuse("source"),
    }
}

/// `re.flags` — the letters, in the one order the language spells them.
extern "C" fn flags(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let letters = with_current(|context| flags_text(context, this));
    match letters {
        Some(text) => with_current(|context| context.intern_value(Str::from_str(&text)).bits()),
        None => refuse("flags"),
    }
}

/// The text `source` answers, or `None` for a receiver that owes a `TypeError`.
///
/// Shared with [`to_string`], which the specification defines over these two
/// getters rather than over the compiled pattern.
fn source_text(context: &Context, this: u64) -> Option<String> {
    match receiver(context, this) {
        Receiver::Pattern(written, _) => Some(super::escaped_source(&written)),
        // Not a pattern, and still the source that round-trips into an empty
        // literal — `//` is a comment, so `(?:)` is what the language answers.
        Receiver::Prototype => Some("(?:)".to_owned()),
        Receiver::Foreign => None,
    }
}

/// The text `flags` answers, which is never a refusal.
///
/// `RegExp.prototype.flags` is defined over the PROPERTIES of its receiver and
/// not over a compiled pattern: it is the one getter here that answers for a
/// plain object, which is how a program describes a pattern without having one.
/// `None` is reserved for a receiver that is not an object at all.
fn flags_text(context: &mut Context, this: u64) -> Option<String> {
    match receiver(context, this) {
        // Already canonical — see `compile::Flags::canonical`.
        Receiver::Pattern(_, letters) => Some(letters),
        Receiver::Prototype => Some(String::new()),
        Receiver::Foreign => {
            let cell = Value(this).as_slot()?;
            let mut letters = String::new();
            for (name, letter) in FLAGS {
                let key = context.well_known(name);
                let held = read_property(context, cell, key).is_some_and(|value| {
                    super::super::primitives::to_boolean_in(context, value.bits())
                });
                if held {
                    letters.push(letter);
                }
            }
            Some(letters)
        }
    }
}

/// `String(re)` — `/source/flags`, which is the literal a program can paste.
///
/// It answered `[object RegExp]`, the plain-object fallback, because
/// `RegExp.prototype` had no `toString` at all — so every template literal and
/// every `"" + re` printed a type name instead of the pattern.
///
/// Defined over the two getters above rather than over the compiled pattern,
/// which is what the specification says and what makes
/// `RegExp.prototype.toString.call({ source: "a", flags: "g" })` answer `/a/g`.
pub(super) extern "C" fn to_string(
    _environment: u64,
    this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let printed = with_current(|context| {
        let written = match source_text(context, this) {
            Some(text) => text,
            None => text_member(context, this, "source")?,
        };
        // `this.flags` as a PROPERTY for a receiver that is not a pattern, and
        // not the eight-boolean reconstruction [`flags_text`] does: `toString`
        // reads the property, so `{ source: "a", flags: "g" }` prints `/a/g`
        // where the reconstruction would have printed `/a/`.
        let letters = match receiver(context, this) {
            Receiver::Foreign => text_member(context, this, "flags").unwrap_or_default(),
            _ => flags_text(context, this)?,
        };
        Some(format!("/{written}/{letters}"))
    });
    match printed {
        Some(text) => with_current(|context| context.intern_value(Str::from_str(&text)).bits()),
        None => refuse("toString"),
    }
}

/// One property of a receiver that is not a pattern, as text.
///
/// Only the shape `toString` can meet here without calling user code: a string.
/// A number or an object would need the coercion protocol, which calls user code
/// and cannot run inside this borrow — so it is left out rather than half-done,
/// and the caller raises instead of inventing an answer.
fn text_member(context: &mut Context, this: u64, name: &str) -> Option<String> {
    let cell = Value(this).as_slot()?;
    let key = context.well_known(name);
    let value = read_property(context, cell, key)?;
    context.text_at(value.as_slot()?)?.to_rust()
}
