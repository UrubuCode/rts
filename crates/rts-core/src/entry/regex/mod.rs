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
//! # Named groups, and the three absences that are gone
//!
//! This paragraph listed three absences under one argument: each needs somewhere
//! to put a second collection of results, and none of them changes what a match
//! IS. All three arrived. `matchAll` is here, and named groups reach a
//! `groups` object through [`Regexp::named_groups`] — both engines expose
//! `capture_names` and always did; what was missing is that `Spans` is indexed
//! by POSITION and carries no name, so a named group reached the runtime
//! anonymous.
//!
//! The third is the `d` flag's `indices`, through [`indices::onto`], and it is
//! the clearest case of that one argument: the positions were in the same
//! `Spans` every match already produced and were thrown away one line later.
//! Nothing about MATCHING changed to answer them.
//!
//! The string methods that take a pattern are **not** here: they live on the
//! string, in [`super::string::pattern`], because the receiver is the string.
//! `"a-b".split("-")` and `"a-b".split(/-/)` are one method with two kinds of
//! separator, and splitting them across two modules is where they would come to
//! disagree about the empty one.

mod accessors;
mod compile;
pub(in crate::entry) mod indices;
pub(in crate::entry) mod methods;
mod translate;

pub(in crate::entry) use compile::{Engine, Flags};

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// Raises the `SyntaxError` a refused pattern owes, OUTSIDE any borrow.
///
/// # Why this is a second function and not two lines inside [`make`]
///
/// Because `make` runs inside a `with_current` closure at every call site, and
/// building an error borrows the context again — a re-entrant `RefCell` borrow,
/// which is an abort in an `extern "C"` frame and not a catchable fault. The
/// first version raised in place and the panic was immediate.
///
/// So the shape every raising native in this crate uses applies here too:
/// decide inside the borrow, answer, and raise after it ends. `make` answers
/// `undefined` for a pattern it could not build, and this turns that answer into
/// the throw the language owes — which matters because `undefined` from a
/// CONSTRUCTOR is not a quiet failure: `new` hands back the half-built `this`
/// instead, and the program dies later reading `.exec` off an object with no
/// pattern in it.
///
/// The two messages are kept apart because they are different faults with
/// different fixes for whoever reads one: an unknown FLAG LETTER is a typo in
/// the second argument, and an unbuildable PATTERN is the first.
pub(super) fn refusal(made: u64, source: &str, letters: &str) -> bool {
    let refused = with_current(|context| made == undefined_of(context));
    if !refused {
        return false;
    }
    match Flags::parse(letters) {
        None => super::throw::syntax_error(&format!(
            "invalid regular expression flags: {letters:?}"
        )),
        Some(_) => super::throw::syntax_error(&format!(
            "invalid regular expression: /{source}/{letters}"
        )),
    }
    true
}

/// A compiled pattern and how a match is driven over it.
///
/// `lastIndex` is **not** here. It is an ordinary property of the object,
/// because the language lets a program write it — `re.lastIndex = 0` is how a
/// stateful search is reset — and a copy kept beside the cell would be the one
/// the search reads while the program wrote the other.
pub(super) struct Regexp {
    engine: Engine,
    flags: Flags,
    /// The text `source`/`flags` answer — kept beside the compiled engine
    /// (rather than read back off the object's own `source`/`flags`
    /// properties, which a program can overwrite) so `new RegExp(existing)`
    /// always copies what the ORIGINAL pattern was compiled from.
    source: String,
    letters: String,
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
    let read = with_current(|context| {
        let source = text_of(context, pattern)?;
        let letters = text_of(context, flags)?;
        let made = make(context, &source, &letters);
        Some((made, source, letters))
    });
    let Some((made, source, letters)) = read else {
        return with_current(|context| undefined_of(context));
    };
    // Outside the borrow, for the reason [`refusal`] states.
    refusal(made, &source, &letters);
    made
}

/// The object, from text that has already been read.
///
/// Shared by the literal and by the constructor, which is the whole reason the
/// entry point takes strings: `/a/g` and `new RegExp("a", "g")` are the same
/// operation, and rule 3 asks for one definition of it. The two differ only in
/// where the text came from, and that difference ends here.
pub(super) fn make(context: &mut Context, source: &str, letters: &str) -> u64 {
    {
        // A pattern this engine cannot build answers `undefined`, and the
        // CALLER raises the `SyntaxError` after its borrow has ended — see
        // [`refusal`] for why the raise cannot happen here.
let Some(parsed) = Flags::parse(letters) else {
            return undefined_of(context);
        };
        let Some(engine) = compiled(context, source, parsed) else {
            return undefined_of(context);
        };

        // From here on the flags are the ones `RegExp.prototype.flags` names,
        // in its order — see [`Flags::canonical`]. The text the program WROTE
        // is not kept: nothing observable answers it, and keeping both would
        // be two answers to "which flags does this pattern have".
        let letters = &parsed.canonical(letters);

        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        // Through `alloc_or_die`, which COLLECTS before it gives up — and this
        // line called `region.alloc` directly, which does not.
        //
        // The difference is not speed. A regular expression built in a loop
        // grew the region and never once ran a cycle, so the whole reservation
        // filled with cells nothing had referred to for a long time and the
        // program died on `heap_exhausted`. Two lines reproduce it, and node
        // and bun both run them:
        //
        // ```js
        // let a = 0;
        // for (let i = 0; i < 300000; i++) { const r = /[0-9]/; if (r === r) a++; }
        // ```
        //
        // `new RegExp("[0-9]")` died the same way, and neither needed a match
        // to do it — merely making one was enough. Every other allocation in
        // this crate already goes through `super::alloc`; this was the one that
        // did not, which is why the collector looked like it worked.
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);

        // The prototype of the class `new` named, when there is one — so
        // `class Mine extends RegExp {}` produces something that reaches
        // `Mine.prototype` rather than something that only knows about
        // `RegExp.prototype`.
        let own = prototype_of(context);
        let prototype = super::functions::prototype_for_new(context, own);
        context.set_prototype(cell, prototype);
        // The CANONICAL order, not the written one — see `Flags::canonical`.
        let letters = &parsed.canonical(letters);
        describe(context, cell, source, letters, parsed);
        context.regexes.set(cell, Regexp {
            engine,
            flags: parsed,
            source: source.to_owned(),
            letters: letters.to_owned(),
        });
        Value::from_slot(cell).bits()
    }
}

/// How many compiled patterns are kept.
///
/// A cap and not a growing map, because the key is text the PROGRAM supplies:
/// `new RegExp(input)` in a loop over distinct inputs would otherwise keep every
/// compiled program alive for the run. When it fills, the whole cache is
/// dropped rather than one entry evicted — a program cycling through more than
/// this many patterns then pays compilation again, which is exactly what it paid
/// before this existed, and an LRU's bookkeeping buys nothing against a cost
/// this lopsided.
const COMPILED_KEPT: usize = 64;

/// The compiled program for a pattern, built once per spelling.
///
/// # Why a cache is sound here at all
///
/// Because the compiled program is a pure function of the source and the flags,
/// and nothing observable depends on two objects having distinct ones. What a
/// program CAN observe about a regular expression object is its identity and its
/// `lastIndex`, and both stay per-cell: `regexes` still holds one [`Regexp`] per
/// object and this shares only the `Engine` inside it. `/a/g` evaluated twice is
/// still two objects with two `lastIndex`, which is the property `mod.rs`'s first
/// page says the literal cannot be interned for.
///
/// # What it costs, and why it is worth a `HashMap`
///
/// Measured 2026-08-26 at `e5058342`: `"abc123def456".split(/[0-9]/)` cost
/// **60 612 ns** with the literal inside the loop and **2 994 ns** with the same
/// pattern hoisted out of it — so **~57.6 us of every regular-expression literal
/// evaluation was `Engine::compile`**, and the matching was the small half.
/// `Context::well_known` argues against a `HashMap` for its five names because
/// hashing to avoid hashing is silly; here the alternative to a hash of a short
/// string is fifty-seven microseconds, which settles it the other way.
///
/// # Why on `Context` and not in an `Aside`
///
/// `regexes` is an `Aside`, which is state keyed BY CELL — one entry per object,
/// which is the shape this is trying not to be. The nearest thing that already
/// has the right shape is `array_layout` and `callable_templates`: a small memo
/// of something a construction would otherwise recompute, keyed by what decides
/// it. Nothing in `rts-cranelift` answers this — it compiles no patterns, and
/// the two matchers are in this crate by its README's rule 5.
///
/// It holds no references into the heap — an `Engine` and a `String` are Rust
/// values — so it is not a root and the collector never has to know about it.
fn compiled(context: &mut Context, source: &str, flags: Flags) -> Option<Engine> {
    let key = (source.to_owned(), flags);
    if let Some(held) = context.compiled_patterns.get(&key) {
        return Some(held.clone());
    }
    let built = Engine::compile(source, flags)?;
    if context.compiled_patterns.len() >= COMPILED_KEPT {
        context.compiled_patterns.clear();
    }
    context.compiled_patterns.insert(key, built.clone());
    Some(built)
}

/// The text a value has, when it is genuinely a string.
fn text_of(context: &Context, value: u64) -> Option<String> {
    context.text_at(Value(value).as_slot()?)?.to_rust()
}

/// Writes the ONE own property a regular expression has.
///
/// `lastIndex`, and nothing else. `source`, `flags` and the eight booleans were
/// written here too and are now accessors on the prototype — [`accessors`] says
/// what that fixed, and why an accessor does not have the fast-path problem
/// this comment used to give as the reason they were data.
///
/// The value is zero even for a pattern that never reads it, because a program
/// may, and `undefined` is not what the language says is there. The attributes
/// are recorded rather than left to default: `lastIndex` is the one built-in
/// property that is WRITABLE and NOT configurable, so `delete re.lastIndex`
/// answers false and `Object.defineProperty` refuses to turn it into an
/// accessor — both program-visible, and the defaults had both backwards.
fn describe(context: &mut Context, cell: u32, _source: &str, _letters: &str, _flags: Flags) {
    let key = context.well_known("lastIndex");
    super::objects::put(context, cell, key, Value::from_f64(0.0).bits());
    if let crate::object::Key::Name(named) = key {
        super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
            writable: true,
            enumerable: false,
            configurable: false,
        });
    }
}

/// `re.source` — the pattern written so that `/` + it + `/` is the same pattern.
///
/// The specification calls this `EscapeRegExpPattern` and it exists for one
/// reason: `source` is what a program puts back between slashes, and two kinds
/// of character break that round trip. A bare `/` would CLOSE the literal, and a
/// line terminator would end the line the literal is on — so both are spelled as
/// escapes, and the result is text a reader can paste back.
///
/// The empty pattern is `(?:)` for the same reason and not as a special case:
/// `//` is a comment, so the one pattern that matches everywhere would round
/// trip into something that is not a pattern at all.
///
/// A `/` INSIDE a character class is left alone, which is what V8 and
/// JavaScriptCore both answer: it cannot close the literal from there, so
/// escaping it would change the text a program reads back without changing what
/// it means.
pub(super) fn escaped_source(source: &str) -> String {
    if source.is_empty() {
        return "(?:)".to_owned();
    }
    let mut out = String::with_capacity(source.len());
    let mut inside = false;
    let mut characters = source.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            out.push(character);
            // Already an escape: whatever follows is spelled, not written raw,
            // so re-escaping it would double every backslash in the pattern.
            if let Some(next) = characters.next() {
                out.push(next);
            }
            continue;
        }
        match character {
            '[' => inside = true,
            ']' => inside = false,
            _ => {}
        }
        match character {
            '/' if !inside => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(character),
        }
    }
    out
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
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    let object = Value::from_slot(cell).bits();
    super::native::install(context, cell, methods::NATIVES);
    // `source`, `flags` and the eight booleans — accessors here rather than
    // properties on every instance. [`accessors`] says what that fixed.
    accessors::install(context, cell);
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
    let callable = super::native::callable(context, methods::construct);
    // `RegExp.name`, which nothing else derives for a hand-built constructor.
    super::native::name_of(context, callable, "RegExp");
    // `RegExp.length` is 2 — the pattern and the flags.
    super::native::length_of(context, callable, 2);
    let prototype = prototype_of(context);
    if let Some(cell) = Value(callable).as_slot() {
        let key = context.well_known("prototype");
        super::objects::put(context, cell, key, prototype);
    }
    // `RegExp.prototype.constructor`, and then the species hook that reads it.
    // Both were missing, and the pair is what makes a subclass of `RegExp`
    // answer itself from `Symbol.split` and `Symbol.replace` rather than a
    // plain one.
    if let Some(cell) = Value(prototype).as_slot() {
        let key = context.well_known("constructor");
        super::objects::put(context, cell, key, callable);
        super::native::hidden(context, cell, key);
    }
    super::native::species(context, callable);
    callable
}

impl Context {
    /// The compiled pattern a cell holds, if it is a regular expression.
    pub(super) fn regexp_at(&self, cell: u32) -> Option<&Regexp> {
        self.regexes.get(cell)
    }
}

impl Regexp {
    /// Where the first match at or after `start` is, in bytes.
    /// Whether it matches, without saying where — see `Engine::matches_at`
    /// for the two allocations that answer avoids.
    ///
    /// A STICKY pattern cannot take the short answer: `y` means "match here"
    /// and the engine has no such mode, so where the match began is part of
    /// deciding whether there was one. It goes the long way and the spans are
    /// what reject it.
    pub(super) fn matches_at(&self, subject: &str, start: usize) -> bool {
        if self.flags.sticky {
            return self.find_at(subject, start).is_some();
        }
        self.engine.matches_at(subject, start)
    }

    /// The name of each capture group, by position. See [`compile::Engine::names`].
    pub(super) fn names(&self) -> Vec<Option<String>> {
        self.engine.names()
    }

    /// The named groups of one match, paired with what they captured.
    ///
    /// Here rather than at each of the three call sites — `exec`,
    /// `String.prototype.match` and `matchAll` — because the pairing is the
    /// only place the engine's positional list meets the language's names, and
    /// three copies of it is three chances to disagree about which group is
    /// which.
    pub(in crate::entry) fn named_groups(
        &self,
        parts: &[Option<String>],
    ) -> Vec<(String, Option<String>)> {
        self.names()
            .into_iter()
            .enumerate()
            .filter_map(|(position, name)| Some((name?, parts.get(position).cloned().flatten())))
            .collect()
    }

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

    /// Whether `g` was written.
    ///
    /// Not the same question as [`Self::tracks_last_index`], which `y` also
    /// answers yes to. This one decides how many matches a string method
    /// produces — `"aa".match(/a/)` is one match and `"aa".match(/a/g)` is two —
    /// and reading the wrong one would make a sticky pattern collect all of
    /// them.
    pub(super) fn is_global(&self) -> bool {
        self.flags.global
    }

    /// Whether `d` was written, and a match therefore says where its groups
    /// were.
    ///
    /// Asked at the three places that build a match object rather than folded
    /// into the match itself: the spans are there either way, and a pattern
    /// without `d` must answer `undefined` for `indices` rather than an array
    /// nobody asked for.
    pub(in crate::entry) fn has_indices(&self) -> bool {
        self.flags.has_indices
    }

    /// The text this pattern was compiled from — what `new RegExp(existing)`
    /// copies.
    pub(super) fn source(&self) -> &str {
        &self.source
    }

    /// The flag letters this pattern was compiled with.
    pub(super) fn flags(&self) -> &str {
        &self.letters
    }
}

/// `m.groups` — an object without a prototype, or `undefined` when the pattern
/// declares no names.
///
/// The two are observable and different: `m.groups?.x` distinguishes them, and
/// a plain object would have inherited `Object.prototype`, so
/// `m.groups.toString` would answer a function for a pattern that named no such
/// group.
pub(in crate::entry) fn groups_object(
    context: &mut Context,
    named: &[(String, Option<String>)],
) -> u64 {
    let absent = super::objects::undefined_of(context);
    if named.is_empty() {
        return absent;
    }
    let Some(holder) = super::native::plain(context) else {
        return absent;
    };
    let bare = methods::null_of(context);
    context.set_prototype(holder, bare);
    for (name, group) in named {
        let key = context.well_known(name);
        let value = match group {
            Some(text) => context.intern_value(Str::from_str(text)).bits(),
            None => absent,
        };
        super::objects::put(context, holder, key, value);
    }
    Value::from_slot(holder).bits()
}
