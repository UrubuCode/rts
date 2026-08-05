//! Regular expressions: making one, and what it can be asked.
//!
//! # Why the literal is an entry point rather than a constant
//!
//! `/a+/g` is a **new object every time it is evaluated**, with its own
//! `lastIndex` — two passes of a loop that each make one must not share where
//! the next search starts. So it cannot be interned the way a string literal is,
//! and what the compiled code carries is the pattern and the flags as ordinary
//! string literals with a call over them.
//!
//! That is also why the compiler hands over *text* rather than a number naming a
//! pre-compiled pattern: `new RegExp(s)` builds one from a value, and a table of
//! compiled patterns would serve the literal and have nothing to say about the
//! other spelling. One path, reached two ways.
//!
//! # Why the methods are native callables on a prototype
//!
//! `re.test(s)` is a property read followed by a call, and this engine already
//! performs both. What it lacked was a callable whose code is Rust rather than
//! compiled JavaScript — and a compiled function's shape,
//! `extern "C" fn(env, this, a0..a3) -> value`, is a shape a Rust function can
//! have.
//!
//! So `test` and `exec` are ordinary properties of an ordinary object that every
//! regular expression inherits from. Nothing in the call path knows the
//! difference, which is the point: the alternative — teaching `call` about a
//! second kind of callee — would put a branch on every call in the program to
//! serve two of them.
//!
//! # What is deliberately absent
//!
//! `new RegExp(p, f)`, because there is no global object for `RegExp` to be a
//! property of. `str.match`, `str.replace` and `str.split` with a pattern,
//! because a string has no prototype here to hang them on. Both arrive with the
//! mechanism they wait for rather than with a special case that anticipates it.

mod compile;
mod methods;

use compile::{Engine, Flags};

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// A compiled pattern and how a match is driven over it.
///
/// `lastIndex` is **not** here. It is an ordinary property of the object,
/// because the language lets a program write it — `re.lastIndex = 0` is how a
/// stateful search is reset — and a copy kept beside the cell would be the one
/// the search reads while the program wrote the other.
pub(super) struct Regexp {
    engine: Engine,
    flags: Flags,
}

/// `/pattern/flags` — a new regular expression object.
///
/// Both arguments are strings, which is what lets one entry point serve the
/// literal and the constructor.
///
/// # Why a pattern that does not compile answers `undefined`
///
/// The language throws a `SyntaxError`. Throwing from here would end the
/// program: [`super::throw`] is the escaping path, and a handler in the function
/// that wrote the literal is exactly the case it cannot find. Answering
/// `undefined` is the same stated gap every other operation in this crate has
/// while unwinding through compiled frames is missing, and it fails where the
/// program uses the result rather than at an arbitrary later point.
#[rtse::entry]
pub fn regex_new(pattern: u64, flags: u64) -> u64 {
    with_current(|context| {
        let Some(source) = text_of(context, pattern) else {
            return undefined_of(context);
        };
        let Some(letters) = text_of(context, flags) else {
            return undefined_of(context);
        };
        make(context, &source, &letters)
    })
}

/// The object, from text that has already been read.
///
/// Shared by the literal and by the constructor, which is the whole reason the
/// entry point takes strings: `/a/g` and `new RegExp("a", "g")` are the same
/// operation, and rule 3 asks for one definition of it. The two differ only in
/// where the text came from, and that difference ends here.
pub(super) fn make(context: &mut Context, source: &str, letters: &str) -> u64 {
    {
        let Some(parsed) = Flags::parse(letters) else {
            return undefined_of(context);
        };
        let Some(engine) = Engine::compile(source, parsed) else {
            return undefined_of(context);
        };

        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        let Some(cell) = context.region.alloc(crate::heap::STRIDE, ty) else {
            // The region is full and there is no collector to ask — the same
            // answer `object_new` gives, and for the same reason.
            return undefined_of(context);
        };

        let prototype = prototype_of(context);
        context.set_prototype(cell, prototype);
        describe(context, cell, source, letters, parsed);
        context.regexes.set(cell, Regexp {
            engine,
            flags: parsed,
        });
        Value::from_slot(cell).bits()
    }
}

/// The text a value has, when it is genuinely a string.
fn text_of(context: &Context, value: u64) -> Option<String> {
    context.text_at(Value(value).as_slot()?)?.to_rust()
}

/// Writes the properties a regular expression answers about itself.
///
/// Real properties rather than answers the runtime invents, for the reason
/// [`super::array::set_length`] records: compiled code reading a property that
/// is in the layout never asks the runtime at all, so a value only the slow path
/// knows about is one the fast path disagrees with the moment it starts working.
fn describe(context: &mut Context, cell: u32, source: &str, letters: &str, flags: Flags) {
    let source_value = context.intern_value(Str::from_str(source)).bits();
    let flags_value = context.intern_value(Str::from_str(letters)).bits();
    let written: [(&str, u64); 6] = [
        ("source", source_value),
        ("flags", flags_value),
        ("global", Value::from_bool(flags.global).bits()),
        ("ignoreCase", Value::from_bool(flags.ignore_case).bits()),
        ("multiline", Value::from_bool(flags.multiline).bits()),
        // Zero even for a pattern that never reads it, because a program may
        // read it, and `undefined` is not what the language says is there.
        ("lastIndex", Value::from_f64(0.0).bits()),
    ];
    for (name, value) in written {
        let key = context.well_known(name);
        super::objects::put(context, cell, key, value);
    }
}

/// What every regular expression inherits from, made once.
///
/// Lazily rather than at context construction: a program with no regular
/// expression should not allocate two callables and an object to hold them, and
/// the region is fixed in size.
fn prototype_of(context: &mut Context) -> u64 {
    if let Some(made) = context.regexp_prototype {
        return made;
    }
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    let Some(cell) = context.region.alloc(crate::heap::STRIDE, ty) else {
        return undefined_of(context);
    };
    let object = Value::from_slot(cell).bits();
    for (name, code) in methods::NATIVES {
        let method = native(context, *code);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, method);
    }
    context.regexp_prototype = Some(object);
    object
}

/// `RegExp` itself, as the value the name reads.
///
/// A callable with a `prototype` property, exactly like a JavaScript function —
/// which is what makes `new RegExp(…)` link the right chain and
/// `re instanceof RegExp` answer true, both through machinery that already
/// exists and knows nothing about regular expressions.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = native(context, methods::construct);
    let prototype = prototype_of(context);
    if let Some(cell) = Value(callable).as_slot() {
        let key = context.well_known("prototype");
        super::objects::put(context, cell, key, prototype);
    }
    callable
}

/// A callable whose code is a Rust function.
///
/// The environment is `undefined`: a native method closes over nothing, and the
/// slot exists because every callable has one rather than because this uses it.
fn native(context: &mut Context, code: methods::Native) -> u64 {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    match context.region.alloc(crate::heap::STRIDE, ty) {
        Some(cell) => {
            let environment = undefined_of(context);
            context.mark_callable(cell, code as usize as u64, environment);
            Value::from_slot(cell).bits()
        }
        None => undefined_of(context),
    }
}

impl Context {
    /// The compiled pattern a cell holds, if it is a regular expression.
    pub(super) fn regexp_at(&self, cell: u32) -> Option<&Regexp> {
        self.regexes.get(cell)
    }
}

impl Regexp {
    /// Where the first match at or after `start` is, in bytes.
    pub(super) fn find_at(&self, subject: &str, start: usize) -> Option<compile::Spans> {
        let spans = self.engine.find_at(subject, start)?;
        // Sticky is not "search from here" — it is "match here". The engine has
        // no such mode, so a match that began later is rejected, which is what
        // `y` means and what separates it from `g`.
        if self.flags.sticky && spans[0]?.0 != start {
            return None;
        }
        Some(spans)
    }

    /// Whether a search resumes from `lastIndex` and advances it.
    pub(super) fn tracks_last_index(&self) -> bool {
        self.flags.tracks_last_index()
    }
}
