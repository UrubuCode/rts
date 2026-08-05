//! `Symbol` — a value that is a property key nothing else can spell.
//!
//! # Why a symbol key is a NAME with a reserved prefix
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
//! `JSON.stringify` there. Porting a proven encoding beats inventing a second
//! one to be different.
//!
//! **What the prefix costs, said out loud:** a program that writes
//! `o["@@iterator"] = 1` has written to the symbol slot. That is unreachable
//! from any spelling a real program uses and it is a genuine divergence rather
//! than an impossibility, which is why it is written here rather than assumed
//! away.
//!
//! # Why a symbol is a cell and not a tag
//!
//! `Symbol("a") === Symbol("a")` is **false**: two symbols with the same
//! description are different values. That is identity, and identity is what a
//! cell already is. A tag over an interned description would have made the two
//! equal, which is the one thing a symbol exists not to be.
//!
//! The cell carries no shape of its own beyond the root, and what makes it a
//! symbol is recorded beside it — the pattern arrays, callables and regular
//! expressions all use, and for the reason the `array_elements` comment states:
//! a reserved layout makes `s.tag = 9` a silent no-op.
//!
//! # What is deliberately absent
//!
//! `Object.getOwnPropertySymbols`. Recovering the symbol VALUE from a key text
//! means a table from `"@@sym:7"` back to the cell, which is a second index over
//! something already recorded. It lands with a caller.
//!
//! `Symbol.prototype.description` is a real property on the instance rather than
//! an accessor on the prototype, because an accessor pair is invisible to the
//! collector and a data property is not — and nothing observable distinguishes
//! them here.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::heap::Aside;
use crate::text::Str;
use crate::value::Value;

/// The prefix that makes a key unwritable by ordinary means.
///
/// One constant, because it is read by the minting side, the enumeration filter
/// and the well-known spelling — and three copies is where one of them would
/// come to use a single `@`.
pub(super) const PREFIX: &str = "@@";

/// What a symbol cell is, beside the cell.
pub(super) struct SymbolInfo {
    /// The key text this symbol names a property with.
    key: String,
    /// What `String(sym)` and `sym.description` answer.
    description: Option<String>,
}

/// Everything a symbol needs of the context, kept in one place.
///
/// A struct rather than three fields on `Context`, because they are one fact
/// with three parts: which cells are symbols, which symbol a shared name
/// already has, and which number the next minted one gets. Splitting them
/// across the context is how the registry and the counter come to disagree.
pub struct Symbols {
    /// Which cells are symbols.
    beside: Aside<SymbolInfo>,
    /// The symbols that are shared by name: the well-known ones, and whatever
    /// `Symbol.for` has been asked for.
    ///
    /// One list for both, keyed by the KEY TEXT rather than the description —
    /// which is what keeps `Symbol.for("iterator")` and `Symbol.iterator`
    /// distinct, because the first mints `"@@for:iterator"` and the second is
    /// `"@@iterator"`. The old engine keeps two tables for this and its own
    /// documentation says the identities must not collide; one table over
    /// distinct key spaces says the same thing once.
    shared: Vec<(String, u64)>,
    /// How many symbols have been minted, which is what makes the next key
    /// unique.
    minted: u32,
    /// What every symbol inherits from, once one exists.
    prototype: Option<u32>,
}

impl Symbols {
    /// A symbol table holding nothing.
    pub fn new() -> Self {
        Symbols {
            beside: Aside::new(),
            shared: Vec::new(),
            minted: 0,
            prototype: None,
        }
    }
}

/// The twelve names the language reserves.
///
/// Written out rather than minted on demand, because `Symbol.iterator` must be
/// the same value in every program that reads it and a typo in one of these is
/// a property that silently never matches. The spelling is the wire format:
/// `"@@iterator"` is the key a `[Symbol.iterator]() {}` member writes to.
const WELL_KNOWN: &[&str] = &[
    "iterator",
    "asyncIterator",
    "hasInstance",
    "isConcatSpreadable",
    "match",
    "replace",
    "search",
    "species",
    "split",
    "toPrimitive",
    "toStringTag",
    "unscopables",
    // Not in ECMA-262 yet at the time of writing and already what `using`
    // compiles against, which is why it is here rather than waiting: the
    // construct is refused by the emitter today and the symbol it will need
    // costs one row.
    "dispose",
    "asyncDispose",
];

impl Context {
    /// The symbol a cell is, if it is one.
    pub(super) fn symbol_at(&self, cell: u32) -> Option<&SymbolInfo> {
        self.symbols.beside.get(cell)
    }
}

/// Whether a key text is a symbol's.
///
/// What enumeration filters on, and the only thing outside this module that
/// needs to know the encoding exists.
pub(super) fn is_symbol_key(text: &str) -> bool {
    text.starts_with(PREFIX)
}

/// The key text a value names, when the value is a symbol.
///
/// This is `ToPropertyKey` for the one case that is not a string: a symbol is
/// its own key rather than one derived from text, which is why the computed
/// path asks this before converting.
pub(super) fn key_text_of(context: &Context, value: u64) -> Option<String> {
    let cell = Value(value).as_slot()?;
    Some(context.symbol_at(cell)?.key.clone())
}

/// A new symbol, with a key nothing has used.
fn mint(context: &mut Context, key: String, description: Option<String>) -> u64 {
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = prototype_of(context) {
        context.set_prototype(cell, Value::from_slot(prototype).bits());
    }
    // A real property rather than an accessor, for the reason the module doc
    // gives — and written before the cell is recorded as a symbol so that
    // interning the name cannot see a half-built one.
    if let Some(text) = &description {
        let value = context.intern_value(Str::from_str(text)).bits();
        let named = context.well_known("description");
        super::objects::put(context, cell, named, value);
    }
    context.symbols.beside.set(cell, SymbolInfo { key, description });
    Value::from_slot(cell).bits()
}

/// `Symbol(description)` — a value equal to nothing but itself.
fn fresh(context: &mut Context, description: Option<String>) -> u64 {
    context.symbols.minted += 1;
    let key = format!("{PREFIX}sym:{}", context.symbols.minted);
    mint(context, key, description)
}

/// A symbol shared under a key text, made once.
fn shared(context: &mut Context, key: String, description: Option<String>) -> u64 {
    if let Some((_, made)) = context.symbols.shared.iter().find(|(held, _)| *held == key) {
        return *made;
    }
    let made = mint(context, key.clone(), description);
    context.symbols.shared.push((key, made));
    made
}

/// One of the language's own symbols, by its bare name.
///
/// `well_known(context, "iterator")` is `Symbol.iterator`, and it is the same
/// value every time — which is the whole point of the shared table, because a
/// per-read symbol would make `o[Symbol.iterator]` write a property no later
/// read could find.
pub(super) fn well_known(context: &mut Context, name: &str) -> u64 {
    shared(
        context,
        format!("{PREFIX}{name}"),
        Some(format!("Symbol.{name}")),
    )
}

/// What every symbol inherits from, made once.
///
/// Recorded before the members are installed, for the reason
/// `string::prototype_of` records: installing interns names, interning
/// allocates, and an allocation can reach back here.
fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.symbols.prototype {
        return Some(made);
    }
    let cell = super::native::plain(context)?;
    context.symbols.prototype = Some(cell);
    super::native::install(context, cell, NATIVES);
    Some(cell)
}

/// What `Symbol.prototype` holds.
const NATIVES: &[(&str, super::native::Native)] = &[("toString", to_string)];

/// `sym.toString()` — `"Symbol(a)"`.
///
/// A method rather than a conversion the `+` operator reaches: the language
/// makes implicit conversion of a symbol a `TypeError` precisely so that a
/// symbol never accidentally becomes text, and `super::text::to_text` therefore
/// refuses one. This is the explicit spelling, and it is the only one.
extern "C" fn to_string(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let described = Value(this)
            .as_slot()
            .and_then(|cell| context.symbol_at(cell))
            .map(|symbol| match &symbol.description {
                Some(text) => format!("Symbol({text})"),
                None => "Symbol()".to_owned(),
            });
        match described {
            Some(text) => context.intern_value(Str::from_str(&text)).bits(),
            None => undefined_of(context),
        }
    })
}

/// `Symbol(description)`.
extern "C" fn make(_e: u64, _this: u64, description: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let text = match description == undefined_of(context) {
            true => None,
            false => super::text::to_text(context, Value(description))
                .and_then(|text| text.to_rust()),
        };
        fresh(context, text)
    })
}

/// `Symbol.for(key)` — the symbol shared under a key across the program.
///
/// Its key space is deliberately separate from the well-known one:
/// `Symbol.for("iterator")` is **not** `Symbol.iterator`, and giving them the
/// same key text is the collision the old engine's own documentation warns
/// about.
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
        let found = Value(value)
            .as_slot()
            .and_then(|cell| context.symbol_at(cell))
            .and_then(|symbol| symbol.key.strip_prefix(&format!("{PREFIX}for:")))
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
/// naming: the well-known symbols are static properties whose values are
/// **objects made at registration time**, and the attribute's constants are
/// numbers and text. Teaching it about a computed constant would be teaching it
/// to run code at registration, which is what this function already is.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, make);
    let Some(cell) = Value(callable).as_slot() else {
        return callable;
    };
    super::native::install(context, cell, &[("for", for_key), ("keyFor", key_for)]);
    if let Some(prototype) = prototype_of(context) {
        let key = context.well_known("prototype");
        let value = Value::from_slot(prototype).bits();
        super::objects::put(context, cell, key, value);
    }
    for name in WELL_KNOWN {
        let symbol = well_known(context, name);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, symbol);
    }
    callable
}
