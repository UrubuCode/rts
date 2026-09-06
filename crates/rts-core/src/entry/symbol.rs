//! `Symbol` — a primitive that is a property key nothing else can spell.
//!
//! # Why a symbol is a tag and not a cell
//!
//! It was a cell, and that was wrong in the way an implementation detail is
//! wrong when it is observable. A symbol is a **primitive**: `typeof` answers
//! `"symbol"`, `s.x = 1` writes nothing, `s instanceof Object` is false, and
//! `Object.keys(s)` is empty. A cell gives none of those — it gives an object
//! that a patched `typeof` lies about, and every other question answers "an
//! object" correctly for the encoding and wrongly for the language.
//!
//! So a symbol is one of the four tags the machine hands the language, exactly
//! as `undefined` is one of the singleton numbers: the payload is the symbol's
//! own number, and two symbols differ in it. `Symbol("a") !== Symbol("a")` then
//! falls out of comparing two words rather than out of comparing two heap
//! identities — which is the same answer arrived at honestly.
//!
//! What stays on the side is only the **description**, which is text and does
//! not fit in a tag's payload beside the number. That is a table this module
//! owns, keyed by the number, and nothing about it is reachable from a program
//! except through `description` and `toString`.
//!
//! # Why a symbol KEY is a name with a reserved prefix
//!
//! The obvious design adds a third `Key` variant beside `Index` and `Name`. It
//! costs more than it looks: `machine_key` would answer `None` for it, so a
//! symbol-keyed property could not live in a shape slot at all, and every place
//! that turns a shape into an enumeration would grow a second path.
//!
//! So a symbol's key is an ordinary interned name in a space a program cannot
//! write: `"@@iterator"` for a well-known one and `"@@sym:7"` for a minted one.
//! Storage, lookup, shapes and the inline cache all work unchanged, and the one
//! thing that has to know is enumeration, which filters the prefix out.
//!
//! This is not a shortcut invented here. It is what the engine being replaced
//! does — `crates/rts-runtime/src/adapters/value/objops.rs`, `key_text` — and it
//! is the design that survived contact with `Object.keys`, `for-in` and
//! `JSON.stringify` there.
//!
//! **What the prefix costs, said out loud:** a program that writes
//! `o["@@iterator"] = 1` has written the symbol slot. That is unreachable from
//! any spelling a real program uses and it is a genuine divergence rather than
//! an impossibility, which is why it is written here rather than assumed away.
//!
//! # What is deliberately absent
//!
//! `Object.getOwnPropertySymbols`. Recovering the symbol VALUE from a key text
//! means a table from `"@@sym:7"` back to the number, which is a second index
//! over something already recorded. It lands with a caller.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// The prefix that makes a key unwritable by ordinary means.
///
/// One constant, because it is read by the minting side, the enumeration filter
/// and the well-known spelling — and three copies is where one of them would
/// come to use a single `@`.
pub(super) const PREFIX: &str = prefix!();

/// The prefix as a literal, so a well-known spelling can be built at COMPILE
/// time.
///
/// `concat!` takes literals and not constants, and the spellings below have to
/// exist as `&'static str` — a `format!` per call is what this replaces. One
/// macro rather than a second `"@@"` written out: [`PREFIX`] is defined from it
/// too, so the two cannot drift.
macro_rules! prefix {
    () => {
        "@@"
    };
}
use prefix;

/// `Symbol.hasInstance`'s key text, spelled once.
///
/// `instance_of` reads this on EVERY `instanceof`, and it used to build the
/// text with `format!` and intern the result each time — two allocations and a
/// hash of the text per operation, for a string that never changes. Measured at
/// the head of a 477 ns operation.
pub(super) const HAS_INSTANCE: &str = concat!(prefix!(), "hasInstance");

/// `Symbol.species`'s internal property key, shared by installation and lookup.
pub(super) const SPECIES: &str = concat!(prefix!(), "species");

/// `Symbol.iterator`'s key text, spelled once for [`HAS_INSTANCE`]'s reason.
///
/// Installed by `array_proto::prototype` and asked for by every array pattern
/// that wants to know whether it may read by index. Both sites built it with
/// `format!("{PREFIX}iterator")` — a `String` and a hash for a name that cannot
/// change — which is precisely what the constant above exists to stop.
pub(super) const ITERATOR: &str = concat!(prefix!(), "iterator");

/// The five protocols a string method offers its argument before falling back
/// to the built-in scan, spelled at COMPILE time for [`HAS_INSTANCE`]'s reason.
///
/// `"abc".split(",")` asks the separator for a `Symbol.split` before scanning
/// anything, and `replace`, `match`, `matchAll` and `search` each ask their own
/// — see `super::string::pattern::hooked`, which is the one caller. So this is
/// asked once per call of five of the most-used methods on `String.prototype`,
/// and every one of those calls used to `format!` its name into a fresh `String`
/// on the way in.
///
/// Named constants rather than `concat!` at each call site so that the
/// **prefix** stays in one place: `prefix!` defines [`PREFIX`] as well, and a
/// spelling written out here with `"@@"` in it is the third copy this module's
/// own documentation says must not exist.
pub(super) const SPLIT: &str = concat!(prefix!(), "split");
/// `Symbol.replace`'s key text. See [`SPLIT`].
pub(super) const REPLACE: &str = concat!(prefix!(), "replace");
/// `Symbol.match`'s key text. See [`SPLIT`].
pub(super) const MATCH: &str = concat!(prefix!(), "match");
/// `Symbol.matchAll`'s key text. See [`SPLIT`].
pub(super) const MATCH_ALL: &str = concat!(prefix!(), "matchAll");
/// `Symbol.search`'s key text. See [`SPLIT`].
pub(super) const SEARCH: &str = concat!(prefix!(), "search");

/// What a symbol is, beside its number.
struct SymbolInfo {
    /// The key text this symbol names a property with.
    key: String,
    /// What `sym.description` and `sym.toString()` answer.
    description: Option<String>,
    /// The NUMBER that key text interned to, once anything has asked.
    ///
    /// # Why this is not the same fact twice
    ///
    /// It is derived from `key`, and rule 3 of this crate's README says a number
    /// space has one source — so this is filled by minting from
    /// `context.interner`/`context.keys`, exactly as [`super::computed`] would,
    /// and never by counting. What it removes is the DERIVING, not the
    /// authority: the text cannot change after the symbol is minted, so the
    /// answer cannot go stale.
    ///
    /// # What it costs to leave absent, measured
    ///
    /// Every symbol-keyed property access went through
    /// `computed::property_key`, which cloned this `String`, converted the copy
    /// to UTF-16 as a `Str`, and hashed that to reach a number the interner
    /// already had — two heap allocations and a hash **per access**.
    ///
    /// The identical defect on the STRING-key path was found and fixed earlier,
    /// and `computed::property_key`'s own comment records what it was worth
    /// there: *"a read through a computed key cost 123x a read through a named
    /// one, and two heap allocations per access were the difference"*. The
    /// symbol path was left on the slow side of that fix.
    ///
    /// It is not a rare path. `instanceof` reads `@@hasInstance` on every
    /// evaluation, every `for`-`of` reads `@@iterator`, and `split`/`replace`/
    /// `match`/`search` each read their own protocol symbol per call.
    key_id: Option<crate::object::Key>,
}

/// Every symbol the program has made, and what each is.
///
/// A `Vec` indexed by the symbol's own number rather than a map, because the
/// number IS the index: it is minted by pushing, so the two cannot drift.
pub struct Symbols {
    /// One entry per symbol, in the order they were minted.
    made: Vec<SymbolInfo>,
    /// The symbols shared by name: the well-known ones, and whatever
    /// `Symbol.for` has been asked for.
    ///
    /// One list for both, keyed by the KEY TEXT rather than the description —
    /// which is what keeps `Symbol.for("iterator")` and `Symbol.iterator`
    /// distinct, because the first mints `"@@for:iterator"` and the second is
    /// `"@@iterator"`. The engine being replaced keeps two tables for this and
    /// its own documentation says the identities must not collide; one table
    /// over two disjoint key spaces says the same thing once.
    shared: Vec<(String, u32)>,
    /// What every symbol inherits from, once one exists.
    prototype: Option<u32>,
}

impl Symbols {
    /// A symbol table holding nothing.
    pub fn new() -> Self {
        Symbols {
            made: Vec::new(),
            shared: Vec::new(),
            prototype: None,
        }
    }
}

/// The twelve names the language reserves, and the two `using` is waiting for.
///
/// Written out rather than minted on demand, because `Symbol.iterator` must be
/// the same value in every program that reads it and a typo in one of these is a
/// property that silently never matches. The spelling is the wire format:
/// `"@@iterator"` is the key a `[Symbol.iterator]() {}` member writes to.
const WELL_KNOWN: &[&str] = &[
    "iterator",
    "asyncIterator",
    "hasInstance",
    "isConcatSpreadable",
    "match",
    // Was ABSENT, and its key text has existed beside its siblings the whole
    // time — [`MATCH_ALL`] — so `"@@matchAll"` was a key `String.prototype.
    // matchAll` looked for while `Symbol.matchAll` was `undefined`. A program
    // defining the protocol could not name the symbol to define it under.
    "matchAll",
    "replace",
    "search",
    "species",
    "split",
    "toPrimitive",
    "toStringTag",
    "unscopables",
    "dispose",
    "asyncDispose",
];

impl Context {
    /// The symbol a value is, if it is one.
    fn symbol_of(&self, value: u64) -> Option<&SymbolInfo> {
        let number = Value(value).as_client(self.kinds.symbol)?;
        self.symbols.made.get(number as usize)
    }
}

/// Whether a key text is reserved — a symbol's, or a private class member's.
///
/// What enumeration filters on, and the only thing outside this module that
/// needs to know the encoding exists. A private member's key is `@@#x`, inside
/// this same space and for the same reason: `@@` is what no program can spell.
/// It was `#x` for one commit, and `#` alone is a prefix a program CAN write —
/// `o["#main"]` is an ordinary property, and it would have disappeared from
/// `Object.keys` and `JSON.stringify`.
/// Asked of the string as it is HELD, and that is the whole of why the
/// signature is a [`Str`]. It took `&str`, so every caller reached it through
/// `to_rust()` — a full copy of the subject — to look at two bytes. Enumeration
/// asks it once per key, so `Object.keys` on a four-property object allocated
/// four strings and dropped them.
pub(super) fn is_symbol_key(text: &Str) -> bool {
    text.starts_with_ascii(PREFIX)
}

/// The reserved spelling of a private class member's key — `@@#`.
const PRIVATE_PREFIX: &str = concat!("@@", "#");

/// Whether a key text is a PRIVATE class member's, as opposed to a symbol's.
///
/// Both live in the reserved space [`is_symbol_key`] answers for, and the two
/// questions are not the same one: a symbol key is an ordinary property that
/// enumeration hides, and a private name is not a property at all in the
/// language — it is a slot the class brands its instances with.
///
/// The difference is observable through a Proxy, which is what asks this.
/// `#x in p` for a proxy `p` is **false** in every runtime, because a proxy has
/// no private slots however faithfully it forwards; here a private name is a
/// key, so the `has` trap ran and the target answered for it.
pub(in crate::entry) fn is_private_key(text: &Str) -> bool {
    text.starts_with_ascii(PRIVATE_PREFIX)
}

/// Whether a value is a symbol at all.
pub fn is_symbol(context: &Context, value: u64) -> bool {
    Value(value).as_client(context.kinds.symbol).is_some()
}

/// Whether a value is a symbol the global registry holds — one `Symbol.for`
/// made, as against one `Symbol()` did.
///
/// The weak collections are what asks. ES2023 admits a symbol as a `WeakMap`
/// key, a `WeakSet` member, a `WeakRef` target and a `FinalizationRegistry`
/// target — but *only* an unregistered one, because a registered symbol is
/// reachable from the registry for ever and so can never die: holding one
/// weakly is a subscription to an event that cannot happen.
///
/// Asked of the key text's prefix, which is where [`for_key`] already writes
/// that fact and where [`key_for`] already reads it back. A flag on
/// `SymbolInfo` would be the same fact in a second place.
pub(in crate::entry) fn is_registered(context: &Context, value: u64) -> bool {
    context
        .symbol_of(value)
        .is_some_and(|symbol| symbol.key.starts_with(&format!("{PREFIX}for:")))
}

/// The key text a value names, when the value is a symbol.
///
/// This is `ToPropertyKey` for the one case that is not a string: a symbol is
/// its own key rather than one derived from text, which is why the computed path
/// asks this before converting.
pub(super) fn key_text_of(context: &Context, value: u64) -> Option<String> {
    Some(context.symbol_of(value)?.key.clone())
}

/// The same answer as [`key_text_of`], as the NUMBER a property is filed under.
///
/// # Why both exist
///
/// Because they answer different questions and only one of them is on a hot
/// path. `key_text_of` is asked where the TEXT is what the caller needs — the
/// `Object.getOwnPropertySymbols` direction, and the enumeration filter. This is
/// asked where a caller only ever wanted the number, which is every computed
/// property access with a symbol key, and it is the one that ran per operation.
///
/// The memo lives on [`SymbolInfo`] and its documentation carries the
/// measurement. Note that this cannot be folded into `key_text_of` by making
/// that one memoise too: interning needs `&mut Context` and that one is handed
/// `&Context`, which is not an accident — the callers that want text must not be
/// able to mint a number as a side effect of reading one.
pub(super) fn key_of(context: &mut Context, value: u64) -> Option<crate::object::Key> {
    let number = Value(value).as_client(context.kinds.symbol)? as usize;
    if let Some(found) = context.symbols.made.get(number)?.key_id {
        return Some(found);
    }
    // The cold path, run at most once per symbol in a program's life. The clone
    // is here rather than avoided because `intern` needs the context mutably
    // while the text is borrowed out of it — the same borrow shape
    // `Context::key_text_value` records, and paying it once is the whole point
    // of the memo.
    let text = crate::text::Str::from_str(&context.symbols.made[number].key);
    let minted = crate::object::Key::Name(context.interner.intern(&text, &mut context.keys));
    context.symbols.made[number].key_id = Some(minted);
    Some(minted)
}

/// The symbol a key text names, the direction [`key_text_of`] does not run.
///
/// Needed for `Object.getOwnPropertySymbols`: the shape only ever held the
/// key TEXT — `key_text_of` is what put it there — so answering the symbol
/// VALUES an object's symbol-keyed properties use means undoing that, not
/// doing it again with a second table. `"@@sym:<n>"` is `Symbol()`'s own
/// encoding and decodes directly; everything else under the `@@` prefix is
/// one of the shared ones — a well-known symbol or `Symbol.for` — and is
/// answered by matching the shared table [`shared`] already keeps.
pub(in crate::entry) fn value_of_key_text(context: &Context, text: &str) -> Option<u64> {
    if let Some(number) = text.strip_prefix(&format!("{PREFIX}sym:")) {
        let number: u64 = number.parse().ok()?;
        if (number as usize) < context.symbols.made.len() {
            return Some(Value::from_client(context.kinds.symbol, number).bits());
        }
        return None;
    }
    let (_, number) = context.symbols.shared.iter().find(|(held, _)| held == text)?;
    Some(Value::from_client(context.kinds.symbol, u64::from(*number)).bits())
}

/// `GetMethod(value, @@name)` — the callable a value carries under one of the
/// language's own symbols, if it carries one.
///
/// # Why this is here and not at the call site
///
/// Because the key text is this module's encoding and nothing else's.
/// `Symbol.match` is the property `"@@match"`, which is a fact stated at
/// [`PREFIX`] and at [`well_known`]; a string method spelling the prefix itself
/// would be the third place that knows it, and the day the prefix changes is the
/// day two of the three still agree.
///
/// `None` covers the three ways there is no protocol here: the value is not an
/// object, it has no such property, or the property is not callable. The
/// specification's own `GetMethod` collapses the first two and throws on the
/// third; throwing is the stated gap the rest of this crate has, and answering
/// `None` sends the caller to the built-in behaviour, which is what a program
/// with an ordinary pattern expects.
///
/// # The key arrives prefixed
///
/// `key` arrives ALREADY PREFIXED — [`SPLIT`] and its four siblings, or
/// [`HAS_INSTANCE`] — rather than as the bare protocol name.
///
/// It used to take `"split"` and build `"@@split"` with `format!` on every call,
/// which is one `String` allocation per `String.prototype.split`, `.replace`,
/// `.match`, `.matchAll` and `.search`, for text that is fixed at compile time.
/// The constants above are what that `format!` becomes, and the reasoning is
/// [`HAS_INSTANCE`]'s, which had the same defect and records what removing it
/// was measured against.
///
/// Taking the prefixed form rather than prefixing here is what makes the
/// improvement unrepresentable to undo: there is no longer a `format!` in this
/// function for a caller to reach.
pub(in crate::entry) fn method_of(context: &mut Context, value: u64, key: &str) -> Option<u64> {
    debug_assert!(
        key.starts_with(PREFIX),
        "method_of takes a prefixed spelling — {key} is a bare protocol name"
    );
    let cell = Value(value).as_slot()?;
    let key = context.well_known(key);
    let found = super::objects::read_property(context, cell, key)?;
    let held = found.as_slot()?;
    context.callable_at(held).is_some().then(|| found.bits())
}

/// `object[Symbol.unscopables]`, when it is an object.
///
/// Here rather than at the caller for [`method_of`]'s reason: `"@@unscopables"`
/// is this module's encoding of a well-known symbol, and a third place spelling
/// [`PREFIX`] is a third place to get it wrong the day the prefix changes.
///
/// Read through the prototype chain, which is what makes
/// `Array.prototype[Symbol.unscopables]` apply to every array — the list the
/// language ships it for. `None` covers "not an object", "no such property" and
/// "the property is not an object": in all three nothing is blocked, which is
/// the answer the specification's `HasBinding` reaches by the same three steps.
pub(in crate::entry) fn unscopables_of(context: &mut Context, cell: u32) -> Option<u32> {
    let key = context.well_known(&format!("{PREFIX}unscopables"));
    super::objects::read_property(context, cell, key)?.as_slot()
}

/// A new symbol under a key nothing has used.
fn mint(context: &mut Context, key: String, description: Option<String>) -> u64 {
    let number = context.symbols.made.len() as u64;
    context.symbols.made.push(SymbolInfo {
        key,
        description,
        key_id: None,
    });
    Value::from_client(context.kinds.symbol, number).bits()
}

/// A symbol shared under a key text, made once.
fn shared(context: &mut Context, key: String, description: Option<String>) -> u64 {
    if let Some((_, number)) = context.symbols.shared.iter().find(|(held, _)| *held == key) {
        return Value::from_client(context.kinds.symbol, u64::from(*number)).bits();
    }
    let made = mint(context, key.clone(), description);
    let number = Value(made)
        .as_client(context.kinds.symbol)
        .expect("just minted") as u32;
    context.symbols.shared.push((key, number));
    made
}

/// One of the language's own symbols, by its bare name.
///
/// `well_known(context, "iterator")` is `Symbol.iterator`, and it is the same
/// value every time — which is the whole point of the shared table, because a
/// per-read symbol would make `o[Symbol.iterator]` write a property no later
/// read could find.
pub fn well_known(context: &mut Context, name: &str) -> u64 {
    shared(
        context,
        format!("{PREFIX}{name}"),
        Some(format!("Symbol.{name}")),
    )
}

/// What every symbol inherits from, made once.
///
/// Reached by [`super::primitive_proto`], because a symbol has no cell to walk
/// from — the same route `(5).toFixed(2)` takes, and the reason a primitive
/// needs no wrapper object to have methods.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.symbols.prototype {
        return Some(made);
    }
    let cell = super::native::plain(context)?;
    // Recorded before the members are installed: installing interns names, and
    // interning allocates, which can reach back here.
    context.symbols.prototype = Some(cell);
    super::native::install(context, cell, NATIVES);
    // Forces `Symbol` itself, which is what writes the `constructor` link back
    // here — the registrations are lazy, so a program that never spells
    // `Symbol` read `Symbol("s").constructor === undefined`.
    super::global::ensure(context, "Symbol");
    // `description` as a prototype ACCESSOR, and `Symbol.prototype
    // [Symbol.toStringTag]` as a data property.
    //
    // `description` was already answerable — [`property`] intercepts the read on
    // a primitive receiver, the same shape `"a".length` has — but it was not a
    // PROPERTY: `Object.getOwnPropertyDescriptor(Symbol.prototype,
    // "description")` said `undefined` for something every symbol answers, and a
    // program walking the prototype to describe it found nothing there. The
    // accessor does not replace that interception: a primitive has no cell, so
    // the read still cannot reach a getter through an ordinary property walk.
    // What it adds is the property being VISIBLE, which is what the descriptor,
    // `Object.getOwnPropertyNames` and a wrapper object all ask for.
    super::native::getter(context, cell, "description", description as super::native::Native);
    let tag = context.well_known(&format!("{PREFIX}toStringTag"));
    let value = context.intern_value(Str::from_str("Symbol")).bits();
    super::objects::put(context, cell, tag, value);
    super::native::hidden(context, cell, tag);
    Some(cell)
}

/// `Symbol.prototype.description` — the getter half of the accessor above.
///
/// Reaches the same [`property`] the primitive path does, so the two cannot
/// answer differently for one symbol: a wrapper object and the primitive it
/// boxes describe themselves alike, which is the one thing a second reader here
/// would eventually get wrong.
extern "C" fn description(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::with_current(|context| {
        let wanted = context.well_known("description");
        match property(context, this, wanted) {
            Some(found) => found,
            None => undefined_of(context),
        }
    })
}

/// What `Symbol.prototype` holds.
const NATIVES: &[(&str, super::native::Native)] =
    &[("toString", to_string), ("valueOf", value_of)];

/// The text a symbol describes itself with.
pub(super) fn described(context: &Context, value: u64) -> Option<String> {
    let symbol = context.symbol_of(value)?;
    Some(match &symbol.description {
        Some(text) => format!("Symbol({text})"),
        None => "Symbol()".to_owned(),
    })
}

/// `sym.toString()` — `"Symbol(a)"`.
///
/// A method rather than a conversion the `+` operator reaches: the language
/// makes implicit conversion of a symbol a `TypeError` precisely so that a
/// symbol never accidentally becomes text, and [`super::text::to_text`]
/// therefore refuses one. This is the explicit spelling, and it is the only one.
extern "C" fn to_string(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| match described(context, this) {
        Some(text) => context.intern_value(Str::from_str(&text)).bits(),
        None => undefined_of(context),
    })
}

/// `sym.valueOf()` — the symbol itself.
extern "C" fn value_of(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

/// `sym.description`, which is a property read rather than a method call.
///
/// Answered here rather than stored, because there is nowhere to store it: a
/// symbol has no cell, which is the whole point. So the property read on a
/// primitive receiver asks this before it walks the prototype — the same shape
/// `"a".length` has, and for the same reason.
pub(super) fn property(context: &mut Context, value: u64, key: crate::object::Key) -> Option<u64> {
    let wanted = context.well_known("description");
    if key != wanted {
        return None;
    }
    let described = context.symbol_of(value)?.description.clone();
    Some(match described {
        Some(text) => context.intern_value(Str::from_str(&text)).bits(),
        None => undefined_of(context),
    })
}

/// `Symbol(description)`.
extern "C" fn make(_e: u64, _this: u64, description: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let text = match description == undefined_of(context) {
            true => None,
            false => {
                super::text::to_text(context, Value(description)).and_then(|text| text.to_rust())
            }
        };
        let number = context.symbols.made.len() as u64;
        context.symbols.made.push(SymbolInfo {
            key: format!("{PREFIX}sym:{number}"),
            description: text,
            key_id: None,
        });
        Value::from_client(context.kinds.symbol, number).bits()
    })
}

/// `Symbol.for(key)` — the symbol shared under a key across the program.
///
/// Its key space is deliberately separate from the well-known one:
/// `Symbol.for("iterator")` is **not** `Symbol.iterator`, and giving them the
/// same key text is the collision the engine being replaced warns about.
extern "C" fn for_key(_e: u64, _this: u64, key: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(text) = super::text::to_text(context, Value(key)).and_then(|text| text.to_rust())
        else {
            return undefined_of(context);
        };
        shared(context, format!("{PREFIX}for:{text}"), Some(text))
    })
}

/// `Symbol.keyFor(sym)` — the key a shared symbol was registered under.
///
/// `undefined` for a symbol that was not, which is what distinguishes one made
/// by `Symbol()` from one made by `Symbol.for`.
extern "C" fn key_for(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let registered = format!("{PREFIX}for:");
        let found = context
            .symbol_of(value)
            .and_then(|symbol| symbol.key.strip_prefix(&registered))
            .map(str::to_owned);
        match found {
            Some(text) => context.intern_value(Str::from_str(&text)).bits(),
            None => undefined_of(context),
        }
    })
}

/// `Symbol` itself, as the value the name reads.
///
/// Written by hand rather than through `#[rtse::class]` for one reason worth
/// naming: the well-known symbols are static properties whose values are made at
/// registration time, and the attribute's constants are numbers and text.
/// Teaching it about a computed constant would be teaching it to run code at
/// registration, which is what this function already is.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, make);
    let Some(cell) = Value(callable).as_slot() else {
        return callable;
    };
    // `Symbol.name` and `Symbol.length`, which every other constructor gets from
    // `#[rtse::class]` and this one — built by hand, for the reason above — got
    // from nothing. Both read `undefined`, and the visible cost is not the
    // metadata itself: `e.constructor.name` is how a program names the thing it
    // caught, so `try { throw Symbol() } catch (e) { e.constructor.name }`
    // answered `undefined` where every runtime answers `"Symbol"`.
    //
    // `0` is the arity the specification pins: `Symbol(description)` counts no
    // required argument.
    super::native::name_of(context, callable, "Symbol");
    super::native::length_of(context, callable, 0);
    super::native::install(context, cell, &[("for", for_key), ("keyFor", key_for)]);
    if let Some(prototype) = prototype_of(context) {
        let key = context.well_known("prototype");
        let value = Value::from_slot(prototype).bits();
        super::objects::put(context, cell, key, value);
        // A constructor's `prototype` is the one property in the language that
        // is non-writable AND non-configurable — `Symbol.prototype = x` is
        // refused and `defineProperty` cannot get round it. Recorded rather than
        // left at the defaults, which say enumerable and writable: it showed up
        // in `Object.keys(Symbol)`, and `Symbol.prototype = 1` STORED.
        super::native::pinned(context, cell, key);
        // And back: `Symbol("s").constructor` answered `undefined` without it,
        // where the language says `Symbol`. Non-enumerable like every other
        // member of a built-in prototype.
        let key = context.well_known("constructor");
        super::objects::put(context, prototype, key, callable);
        super::native::hidden(context, prototype, key);
    }
    for name in WELL_KNOWN {
        let symbol = well_known(context, name);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, symbol);
        // Non-writable, non-enumerable and NON-CONFIGURABLE — the one property
        // shape in the language that cannot be redefined at all, and the
        // specification gives it to every well-known symbol for a reason a
        // program can reach: `Symbol.iterator` is an identity that `for`-`of`,
        // spread and destructuring all compare against, so a program able to
        // replace it could make a class's `[Symbol.iterator]` member write a key
        // nothing looks for.
        //
        // They were installed at the defaults, so all three read the other way:
        // `Object.keys(Symbol)` listed fourteen names, `Symbol.iterator = 1`
        // stored, and `delete Symbol.iterator` succeeded.
        super::native::pinned(context, cell, key);
    }
    callable
}
