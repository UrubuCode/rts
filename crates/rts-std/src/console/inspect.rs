//! Node-shaped `util.inspect`, over the public entry points only.
//!
//! # Why this calls into the running program at all
//!
//! `console.log({a:1})` has to read `a`'s value, and `console.log(new Map([...]))`
//! has to read what is in the table — neither is data this crate can see without
//! asking the runtime, and the runtime's own collection tables are private to
//! `rts-core` (rule 2 of that crate's README: ask the machine — here, the
//! runtime — before inventing a second answer). So every read below goes through
//! `get_property`, `get_indexed`, `own_keys`, or a call to the object's own
//! `forEach` — the same entry points compiled code uses, which is what keeps this
//! agreeing with the language about what a property or an element IS.
//!
//! # Rule 8, applied to a formatter
//!
//! `get_property` can run a getter, and `Map`/`Set`'s own `forEach` is a call
//! into the program. Both are user code an entry point cannot assume succeeded,
//! so every site that might have run one checks [`entry::thrown`] before
//! trusting what came back. [`Poisoned`] is that check turned into a type: once
//! any read sees a throw in flight, the whole inspection stops and answers
//! `Err`, and `console.log` prints nothing rather than a value built from a
//! program that is unwinding underneath it. The throw itself is **not** taken —
//! it is left exactly where it was, so the compiled call site above
//! `console.log` still sees it and re-raises, the same propagation
//! `array_proto::iterate::visit` uses for the same reason.
//!
//! # Depth and cycles
//!
//! Node's default depth is 2: a value is expanded at recursion depth 0, 1 and 2,
//! and anything past that prints as `[Object]`/`[Array]`/`[Map]`/`[Set]` instead
//! of being walked. `{a:{b:{c:{d:1}}}}` prints `{ a: { b: { c: [Object] } } }` —
//! checked against `node --experimental-strip-types`, not guessed.
//!
//! A cycle is not a special case of depth: `const a = {}; a.self = a` never gets
//! deeper, it repeats. [`Walk`] carries the cell of every container currently
//! being printed, and one already on it prints as `[Circular *N]`, with
//! `<ref *N>` written in front of the container it points at once that
//! container is finished. `N` counts the distinct targets in the order they are
//! found, as Node numbers them.
//!
//! # What is not implemented, by name
//!
//! Async and generator functions print as plain `[Function: name]`: the engine
//! keeps no kind flag on a callable (an `async function` and an arrow are the
//! same record), so `[AsyncFunction: n]` / `[GeneratorFunction: n]` cannot be
//! told apart from the ordinary form without a codegen change.
//!
//! No line-width layout: Node folds an object past 72 columns onto several
//! lines and groups long numeric arrays into columns; this prints one line.

use rts_core::entry::{
    self, call, get_indexed, get_property, get_prototype, global_get, instance_of, is_array,
    key_number, make_callable, make_number, make_string, own_keys, undefined_value, with_runtime,
};

/// A throw was seen in flight while reading the program to describe it.
///
/// Carries nothing: the throw itself already lives in the context —
/// [`entry::thrown`] keeps answering true for it — and this is only the signal
/// that formatting must stop rather than print a partial answer.
pub struct Poisoned;

const MAX_DEPTH: u32 = 2;

/// Node prints at most this many array elements before `... N more items`.
const MAX_ITEMS: usize = 100;

/// The cells currently being printed, and the ones a cycle has pointed back at.
///
/// Node numbers a cycle in the order it is FOUND, and writes `<ref *N>` in front
/// of the object that was pointed at — which is only known once that object's
/// own children are done, so the prefix is added on the way out.
#[derive(Default)]
struct Walk {
    ancestors: Vec<u32>,
    circular: Vec<u32>,
}

impl Walk {
    fn push(&mut self, slot: u32) {
        self.ancestors.push(slot);
    }

    fn pop(&mut self) {
        self.ancestors.pop();
    }

    fn contains(&self, slot: &u32) -> bool {
        self.ancestors.contains(slot)
    }

    /// The 1-based number of the cycle that points at `slot`, minted on first
    /// sight.
    fn reference(&mut self, slot: u32) -> usize {
        match self.circular.iter().position(|seen| *seen == slot) {
            Some(at) => at + 1,
            None => {
                self.circular.push(slot);
                self.circular.len()
            }
        }
    }

    /// The number `slot` was given, if a cycle has pointed at it.
    fn referenced(&self, slot: u32) -> Option<usize> {
        self.circular.iter().position(|seen| *seen == slot).map(|at| at + 1)
    }
}

/// Node's own spacing, for a value at the top of an argument list.
///
/// A string prints bare here and quoted everywhere it is nested —
/// `console.log("hi")` prints `hi`, `console.log(["hi"])` prints `[ 'hi' ]`.
pub fn top_level(value: u64) -> Result<String, Poisoned> {
    if is_bare_string(value) {
        // `described` cannot fail on a string: no user code runs to answer it.
        return Ok(entry::described(value).unwrap_or_default());
    }
    inspect(value, 0, &mut Walk::default())
}

/// Whether a value is a STRING — the one kind `described` also answers that
/// prints bare at the top level and quoted everywhere else.
fn is_bare_string(value: u64) -> bool {
    entry::number_of(value).is_none() && entry::described(value).is_some() && as_slot(value).is_some()
}

/// One value, formatted the way it prints NESTED — a string quoted, unlike
/// [`top_level`]. What `console.table`'s cells need: a table cell is never
/// the sole top-level argument `console.log` gives its one bare-string
/// exception to.
pub fn cell(value: u64) -> Result<String, Poisoned> {
    inspect(value, 1, &mut Walk::default())
}

/// The region index of an object value, or `None` for anything with no cell —
/// `pub(super)` so [`super::table`] can tell "is this a row/column source" from
/// "is this a primitive that only has a `Values` cell" without re-deriving the
/// decoder [`as_slot`] already is.
pub(super) fn slot_of(value: u64) -> Option<u32> {
    as_slot(value)
}

/// Whether `value` is callable — [`super::table`] excludes a function from
/// "expand into columns" the same way [`inspect`] excludes one from "walk its
/// own keys": `console.table([Math.max])` has one row and no columns, not a
/// row of whatever properties a function object happens to carry.
pub(super) fn is_callable(value: u64) -> bool {
    with_runtime(|context| entry::is_callable_in(context, value))
}

/// [`super::table`]'s own read of `Object.keys(value)`, through the same
/// throw-checked path [`keys_of`] already is — `pub(super)` rather than a
/// second copy, per `reuse-check`.
pub(super) fn own_key_texts(value: u64) -> Result<Vec<String>, Poisoned> {
    keys_of(value)
}

/// [`super::table`]'s own property read, through the same throw-checked path
/// every other member of this file uses.
pub(super) fn property(value: u64, name: &str) -> Result<u64, Poisoned> {
    let read = get_property(value, well_known(name));
    if poisoned() { Err(Poisoned) } else { Ok(read) }
}

/// The formatted text of one value, at `depth` levels of nesting from the
/// argument that was printed.
fn inspect(value: u64, depth: u32, walk: &mut Walk) -> Result<String, Poisoned> {
    if let Some(text) = entry::described(value) {
        // A primitive: number, boolean, `null`, `undefined`, a string (nested,
        // so quoted), or a bigint's digits — `described` covers all of them and
        // none can run user code to answer, so there is nothing here to check
        // for a throw.
        return Ok(match entry::number_of(value) {
            // Node's inspector shows the sign of zero; `ToString` — what
            // `described` answers — does not, so `-0` is the one value this
            // reads the tag for rather than trusting the text.
            Some(number) if number == 0.0 && number.is_sign_negative() => "-0".to_owned(),
            Some(_) => text,
            // A bigint is the one primitive Node suffixes: `2n`.
            None if type_name(value) == "bigint" => format!("{text}n"),
            None if as_slot(value).is_none() => text, // boolean / null / undefined
            None => quoted(&text),                    // a string, nested
        });
    }
    let Some(slot) = as_slot(value) else {
        if type_name(value) == "symbol" {
            return symbol_text(value);
        }
        // A `Kind::Client` this module does not recognise.
        return Ok("[object]".to_owned());
    };
    if walk.contains(&slot) {
        return Ok(format!("[Circular *{}]", walk.reference(slot)));
    }
    let text = cell_text(value, depth, walk, slot)?;
    // A cycle found somewhere below pointed back at this very cell: Node marks
    // the target, which is only known now that its children are printed.
    Ok(match walk.referenced(slot) {
        Some(number) => format!("<ref *{number}> {text}"),
        None => text,
    })
}

/// A container, by what it is.
fn cell_text(value: u64, depth: u32, walk: &mut Walk, slot: u32) -> Result<String, Poisoned> {
    if with_runtime(|context| entry::is_callable_in(context, value)) {
        return function(value, depth, walk, slot);
    }
    if let Some(kind) = collection_kind(value)? {
        return collection(value, kind, depth, walk, slot);
    }
    if is_array(value) {
        return array(value, depth, walk, slot);
    }
    if let Some(name) = typed_array_name(value)? {
        return typed_array(value, &name, depth, walk, slot);
    }
    if entry::is_regexp_object(value) {
        let source = text_property(value, "source")?;
        let flags = text_property(value, "flags")?;
        return decorated(format!("/{source}/{flags}"), "RegExp", value, 0, depth, walk, slot);
    }
    if let Some(primitive) = entry::boxed_primitive(value) {
        return boxed(value, primitive, depth, walk, slot);
    }
    if let Some(outcome) = entry::promise_inspect_outcome(value) {
        return promise(value, outcome, depth, walk, slot);
    }
    object(value, depth, walk, slot)
}

/// The type name `typeof` answers for a value.
fn type_name(value: u64) -> String {
    entry::text_of(entry::type_of(value)).unwrap_or_default()
}

/// `Symbol(desc)`, through the global `String` — the one conversion that
/// accepts a symbol and spells it out.
fn symbol_text(value: u64) -> Result<String, Poisoned> {
    let string = global_get(well_known("String"));
    if poisoned() {
        return Err(Poisoned);
    }
    let undefined = undefined_value();
    let spelled = call(string, undefined, value, undefined, undefined, undefined);
    if poisoned() {
        return Err(Poisoned);
    }
    Ok(entry::text_of(spelled).unwrap_or_else(|| "Symbol()".to_owned()))
}

/// A property read as TEXT, empty when it is not a string.
fn text_property(value: u64, name: &str) -> Result<String, Poisoned> {
    let read = get_property(value, well_known(name));
    if poisoned() {
        return Err(Poisoned);
    }
    if type_name(read) == "string" {
        Ok(entry::text_of(read).unwrap_or_default())
    } else {
        Ok(String::new())
    }
}

/// Whether a throw is in flight, from a read this module just performed.
///
/// Checked after every call that can reach user code — a getter through
/// `get_property`, `forEach` through `call` — per rule 8 of `rts-core`'s
/// README. Not taken: a native that clears a throw it did not raise is a native
/// claiming to have handled something it only noticed, and the compiled call
/// site above `console.log` is the one this engine designates to re-raise it.
fn poisoned() -> bool {
    entry::thrown() != 0
}

/// The region index a reference value carries, or `None` for anything that has
/// no cell — a number, a boolean, a singleton, a symbol, a bigint.
///
/// `rts_core::Value` is exported from the crate root for exactly this: a
/// client crate needing "which cell is this" without re-deriving the encoding
/// reaches the same decoder every entry point in that crate uses, rather than a
/// second one over the tag bits.
fn as_slot(value: u64) -> Option<u32> {
    rts_core::Value(value).as_slot()
}

enum Kind {
    Map,
    Set,
}

/// `Map` or `Set`, if the global constructor for either is on the value's
/// prototype chain.
///
/// Asked before "is this an array": nothing stops a program from putting a
/// `length` property on a `Map`, and asking `is_array` first would have been a
/// guess where this is a fact about the chain.
fn collection_kind(value: u64) -> Result<Option<Kind>, Poisoned> {
    for (name, kind) in [("Map", Kind::Map), ("Set", Kind::Set)] {
        let ctor = global_get(well_known(name));
        if poisoned() {
            return Err(Poisoned);
        }
        if instance_of(value, ctor) {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

/// The property-key number for a name this module reads or calls by —
/// `"forEach"`, `"length"`, `"Map"`, `"Set"` — minted the same way any computed
/// property name is: intern the text, then `key_number`, the public half of
/// `ToPropertyKey`.
fn well_known(name: &str) -> i64 {
    let text = with_runtime(|context| make_string(context, name));
    key_number(text)
}

/// Node's `strEscape`: single quotes unless the text holds one — then double
/// quotes unless it holds one of those too — then backticks, and only when all
/// three are present is the single quote escaped. Control characters print as
/// escapes, never raw.
fn quoted(text: &str) -> String {
    let quote = if !text.contains('\'') {
        '\''
    } else if !text.contains('"') {
        '"'
    } else if !text.contains('`') && !text.contains("${") {
        '`'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\'' if quote == '\'' => out.push_str("\\'"),
            c if (c as u32) < 0x20 || (0x7f..=0x9f).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Whether a property name prints bare (`a: 1`) or quoted (`'a-b': 1`) — Node's
/// own rule, `/^[a-zA-Z_][a-zA-Z_0-9]*$/`: ASCII only, and no `$`.
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `object.length`, as a count — `0` for anything absent or negative, which a
/// well-formed array never has and a tampered `length` should not crash over.
fn length_of(value: u64) -> Result<usize, Poisoned> {
    let read = get_property(value, well_known("length"));
    if poisoned() {
        return Err(Poisoned);
    }
    Ok(entry::number_of(read).filter(|n| *n >= 0.0).map_or(0, |n| n as usize))
}

/// `<3 empty items>`, the way Node writes a run of holes.
fn empty_items(count: usize) -> String {
    format!("<{count} empty item{}>", if count == 1 { "" } else { "s" })
}

fn array(value: u64, depth: u32, walk: &mut Walk, slot: u32) -> Result<String, Poisoned> {
    if depth > MAX_DEPTH {
        return Ok("[Array]".to_owned());
    }
    let length = length_of(value)?;
    if length == 0 {
        return Ok("[]".to_owned());
    }
    let holes = entry::array_hole_runs(value, length);
    walk.push(slot);
    let mut items = Vec::new();
    let mut index = 0;
    let mut run = 0;
    while index < length && items.len() < MAX_ITEMS {
        // A run of holes is ONE entry, however long.
        if let Some(&(start, count)) = holes.get(run) {
            if start == index {
                items.push(empty_items(count));
                index += count;
                run += 1;
                continue;
            }
        }
        let element = get_indexed(value, make_number(index as f64));
        if poisoned() {
            walk.pop();
            return Err(Poisoned);
        }
        match inspect(element, depth + 1, walk) {
            Ok(text) => items.push(text),
            Err(poisoned) => {
                walk.pop();
                return Err(poisoned);
            }
        }
        index += 1;
    }
    walk.pop();
    if index < length {
        let more = length - index;
        items.push(format!("... {more} more item{}", if more == 1 { "" } else { "s" }));
    }
    Ok(format!("[ {} ]", items.join(", ")))
}

/// The constructor name when `value` is a typed array (by its constructor, the
/// same read `label_of` makes), so the inspector can print `Uint8Array(2) [ 7, 8 ]`.
fn typed_array_name(value: u64) -> Result<Option<String>, Poisoned> {
    const NAMES: [&str; 11] = [
        "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array",
        "Uint32Array", "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array",
    ];
    let label = label_of(value)?;
    let name = label.trim_end();
    Ok(NAMES.contains(&name).then(|| name.to_owned()))
}

/// `Name(length) [ a, b ]`, past 100 elements `... N more items` as Node does.
/// Elements are read through the indexed path, so a BigInt element prints with
/// its `n` suffix via [`inspect`].
fn typed_array(
    value: u64,
    name: &str,
    depth: u32,
    walk: &mut Walk,
    slot: u32,
) -> Result<String, Poisoned> {
    if depth > MAX_DEPTH {
        return Ok(format!("[{name}]"));
    }
    let length = length_of(value)?;
    if length == 0 {
        return Ok(format!("{name}(0) []"));
    }
    walk.push(slot);
    let shown = length.min(MAX_ITEMS);
    let mut items = Vec::with_capacity(shown + 1);
    for index in 0..shown {
        let element = get_indexed(value, make_number(index as f64));
        if poisoned() {
            walk.pop();
            return Err(Poisoned);
        }
        items.push(inspect(element, depth + 1, walk)?);
    }
    walk.pop();
    if length > shown {
        let more = length - shown;
        items.push(format!("... {more} more item{}", if more == 1 { "" } else { "s" }));
    }
    Ok(format!("{name}({length}) [ {} ]", items.join(", ")))
}

/// A function or class: `[Function: name]`, `[Function (anonymous)]`,
/// `[class A]`, `[class B extends A]` — with `{ k: v }` after it when it carries
/// own enumerable properties.
fn function(value: u64, depth: u32, walk: &mut Walk, slot: u32) -> Result<String, Poisoned> {
    let name = text_property(value, "name")?;
    let base = if entry::is_class_constructor(value) {
        let mut base = format!("[class {}", if name.is_empty() { "(anonymous)" } else { &name });
        let parent = get_prototype(value);
        if as_slot(parent).is_some() {
            let parent_name = text_property(parent, "name")?;
            if !parent_name.is_empty() {
                base.push_str(&format!(" extends {parent_name}"));
            }
        }
        base.push(']');
        base
    } else if name.is_empty() {
        "[Function (anonymous)]".to_owned()
    } else {
        format!("[Function: {name}]")
    };
    decorated(base, "Function", value, 0, depth, walk, slot)
}

/// `[Number: 1]`, `[String: 's']`, `[Boolean: true]`, `[BigInt: 1n]` — a wrapper
/// object by the primitive it wraps, plus any own properties it was given. A
/// string wrapper's index keys are the characters themselves, not extras.
fn boxed(value: u64, primitive: u64, depth: u32, walk: &mut Walk, slot: u32) -> Result<String, Poisoned> {
    let kind = match type_name(primitive).as_str() {
        "number" => "Number",
        "string" => "String",
        "boolean" => "Boolean",
        "bigint" => "BigInt",
        "symbol" => "Symbol",
        _ => return object(value, depth, walk, slot),
    };
    let inner = inspect(primitive, depth + 1, walk)?;
    let skip = if kind == "String" { length_of(value)? } else { 0 };
    decorated(format!("[{kind}: {inner}]"), kind, value, skip, depth, walk, slot)
}

/// `base`, followed by `{ k: v, … }` for the own enumerable properties of
/// `value` when it has any — the shape a function, a regular expression and a
/// wrapper share. `skip_indices` hides that many leading index keys (a string
/// wrapper's characters). Past the depth limit a decorated value collapses to
/// `[noun]`, as Node's does.
fn decorated(
    base: String,
    noun: &str,
    value: u64,
    skip_indices: usize,
    depth: u32,
    walk: &mut Walk,
    slot: u32,
) -> Result<String, Poisoned> {
    let keys = visible_keys(value, skip_indices)?;
    let symbols = symbol_keys(value)?;
    if keys.is_empty() && symbols.is_empty() {
        return Ok(base);
    }
    if depth > MAX_DEPTH {
        return Ok(format!("[{noun}]"));
    }
    walk.push(slot);
    let entries = properties(value, keys, symbols, depth, walk);
    walk.pop();
    Ok(format!("{base} {{ {} }}", entries?.join(", ")))
}

/// `Promise { 5 }`, `Promise { <pending> }`, `Promise { <rejected> 3 }`.
fn promise(
    value: u64,
    outcome: Option<(bool, u64)>,
    depth: u32,
    walk: &mut Walk,
    slot: u32,
) -> Result<String, Poisoned> {
    let label = label_of(value)?;
    let name = label.trim_end();
    let head = if name.is_empty() || name == "Promise" {
        "Promise".to_owned()
    } else {
        format!("{name} [Promise]")
    };
    if depth > MAX_DEPTH {
        return Ok("[Promise]".to_owned());
    }
    walk.push(slot);
    let state = match outcome {
        None => Ok("<pending>".to_owned()),
        Some((false, settled)) => inspect(settled, depth + 1, walk),
        Some((true, settled)) => inspect(settled, depth + 1, walk).map(|text| format!("<rejected> {text}")),
    };
    let entries = state.and_then(|text| {
        let keys = visible_keys(value, 0)?;
        let symbols = symbol_keys(value)?;
        let mut entries = vec![text];
        entries.extend(properties(value, keys, symbols, depth, walk)?);
        Ok(entries)
    });
    walk.pop();
    Ok(format!("{head} {{ {} }}", entries?.join(", ")))
}

fn object(value: u64, depth: u32, walk: &mut Walk, slot: u32) -> Result<String, Poisoned> {
    if depth > MAX_DEPTH {
        return Ok("[Object]".to_owned());
    }
    // O nome da CLASSE, que Node e Bun imprimem antes das chaves:
    // `class Test {}` mostra `Test { x: 1 }` e nao `{ x: 1 }`. Lido do
    // `constructor` do objeto, que e a unica coisa que o distingue de um
    // literal — e por isso nao ha lista de nomes em lado nenhum: uma classe
    // escrita pelo programa responde igual a uma classe do motor.
    let label = label_of(value)?;
    let keys = keys_of(value)?;
    // A `Date` keeps its time value in a slot that is an ordinary own key here;
    // Node prints the ISO text bare instead, at any depth.
    if label == "Date " && keys.iter().any(|key| key == "__dateValue") {
        if let Some(text) = entry::date_inspect_text(value) {
            return Ok(text);
        }
    }
    let symbols = symbol_keys(value)?;
    if keys.is_empty() && symbols.is_empty() {
        return Ok(format!("{label}{{}}"));
    }
    walk.push(slot);
    let entries = properties(value, keys, symbols, depth, walk);
    walk.pop();
    Ok(format!("{label}{{ {} }}", entries?.join(", ")))
}

/// One `key: value` per own enumerable key, string keys first and symbol keys
/// after, as Node orders them. An accessor prints as `[Getter]`, `[Setter]` or
/// `[Getter/Setter]` without being run.
fn properties(
    value: u64,
    keys: Vec<String>,
    symbols: Vec<u64>,
    depth: u32,
    walk: &mut Walk,
) -> Result<Vec<String>, Poisoned> {
    let accessors = entry::has_accessors(value);
    let mut entries = Vec::with_capacity(keys.len() + symbols.len());
    for key in keys {
        let printed = match accessor_text(value, accessors, key_text_value(&key))? {
            Some(text) => text,
            None => {
                let read = get_property(value, well_known(&key));
                if poisoned() {
                    return Err(Poisoned);
                }
                inspect(read, depth + 1, walk)?
            }
        };
        let name = if is_identifier(&key) { key } else { quoted(&key) };
        entries.push(format!("{name}: {printed}"));
    }
    for symbol in symbols {
        let printed = match accessor_text(value, accessors, symbol)? {
            Some(text) => text,
            None => {
                let read = get_indexed(value, symbol);
                if poisoned() {
                    return Err(Poisoned);
                }
                inspect(read, depth + 1, walk)?
            }
        };
        // The installed Node (v25) writes a symbol key bare, `Symbol(k): v`, with no
        // brackets; the fixture is compared against that binary, so this follows it.
        entries.push(format!("{}: {printed}", symbol_text(symbol)?));
    }
    Ok(entries)
}

fn key_text_value(key: &str) -> u64 {
    with_runtime(|context| make_string(context, key))
}

/// `[Getter]` / `[Setter]` / `[Getter/Setter]` when `key` is an accessor of
/// `value`, `None` for a data property. Asked of the descriptor, and only for
/// an object that has any accessor at all, so the common object pays nothing.
fn accessor_text(value: u64, accessors: bool, key: u64) -> Result<Option<String>, Poisoned> {
    if !accessors {
        return Ok(None);
    }
    let descriptor = descriptor_of(value, key)?;
    if as_slot(descriptor).is_none() {
        return Ok(None);
    }
    let getter = get_property(descriptor, well_known("get"));
    let setter = get_property(descriptor, well_known("set"));
    if poisoned() {
        return Err(Poisoned);
    }
    Ok(match (type_name(getter) == "function", type_name(setter) == "function") {
        (true, true) => Some("[Getter/Setter]".to_owned()),
        (true, false) => Some("[Getter]".to_owned()),
        (false, true) => Some("[Setter]".to_owned()),
        (false, false) => None,
    })
}

/// `Object.getOwnPropertyDescriptor(value, key)`, through the global.
fn descriptor_of(value: u64, key: u64) -> Result<u64, Poisoned> {
    let object = global_get(well_known("Object"));
    let find = get_property(object, well_known("getOwnPropertyDescriptor"));
    if poisoned() {
        return Err(Poisoned);
    }
    let undefined = undefined_value();
    let descriptor = call(find, undefined, value, key, undefined, undefined);
    if poisoned() { Err(Poisoned) } else { Ok(descriptor) }
}

/// The own ENUMERABLE symbol keys of `value`, in definition order.
fn symbol_keys(value: u64) -> Result<Vec<u64>, Poisoned> {
    let object = global_get(well_known("Object"));
    let list = get_property(object, well_known("getOwnPropertySymbols"));
    if poisoned() {
        return Err(Poisoned);
    }
    if type_name(list) != "function" {
        return Ok(Vec::new());
    }
    let undefined = undefined_value();
    let symbols = call(list, undefined, value, undefined, undefined, undefined);
    if poisoned() {
        return Err(Poisoned);
    }
    if !is_array(symbols) {
        return Ok(Vec::new());
    }
    let length = length_of(symbols)?;
    let mut found = Vec::new();
    for index in 0..length {
        let symbol = get_indexed(symbols, make_number(index as f64));
        if poisoned() {
            return Err(Poisoned);
        }
        let descriptor = descriptor_of(value, symbol)?;
        if as_slot(descriptor).is_none() {
            continue;
        }
        let enumerable = get_property(descriptor, well_known("enumerable"));
        if poisoned() {
            return Err(Poisoned);
        }
        if entry::described(enumerable).as_deref() == Some("true") {
            found.push(symbol);
        }
    }
    Ok(found)
}

/// The own enumerable string keys, less the first `skip_indices` index keys of a
/// string wrapper (its characters).
fn visible_keys(value: u64, skip_indices: usize) -> Result<Vec<String>, Poisoned> {
    let keys = keys_of(value)?;
    if skip_indices == 0 {
        return Ok(keys);
    }
    Ok(keys
        .into_iter()
        .filter(|key| !key.parse::<usize>().is_ok_and(|index| index < skip_indices))
        .collect())
}

/// `"Test "` for an instance of `class Test`, nothing at all for a plain
/// object, and `"[Object: null prototype] "` for one with no prototype.
///
/// The trailing space belongs to the label rather than to the caller, so the
/// empty case is genuinely empty and `{}` does not become ` {}`.
///
/// Read through `constructor`, which a program can change — and that is the
/// right behaviour rather than a hazard: an object whose `constructor` was
/// reassigned prints what it now claims to be, which is what Node does.
fn label_of(value: u64) -> Result<String, Poisoned> {
    let constructor = get_property(value, well_known("constructor"));
    if poisoned() {
        return Err(Poisoned);
    }
    if as_slot(constructor).is_none() {
        // No constructor at all. A prototype of `null` is what Node spells out;
        // anything else without one keeps the plain shape.
        let prototype = get_prototype(value);
        if entry::described(prototype).as_deref() == Some("null") {
            return Ok("[Object: null prototype] ".to_owned());
        }
        return Ok(String::new());
    }
    let name = get_property(constructor, well_known("name"));
    if poisoned() {
        return Err(Poisoned);
    }
    match entry::text_of(name) {
        // `Object` is the plain case and prints bare, which is what makes
        // `{ a: 1 }` look like what was written.
        Some(text) if text != "Object" && !text.is_empty() => Ok(format!("{text} ")),
        _ => Ok(String::new()),
    }
}

/// `Object.keys(value)`, read back out of the array `own_keys` answers.
///
/// `own_keys` is itself an entry point over a proxy trap, which is user code —
/// so this checks for a throw the same as every other read here.
fn keys_of(value: u64) -> Result<Vec<String>, Poisoned> {
    let array = own_keys(value);
    if poisoned() {
        return Err(Poisoned);
    }
    let length = length_of(array)?;
    let mut keys = Vec::with_capacity(length);
    for index in 0..length {
        let key = get_indexed(array, make_number(index as f64));
        if poisoned() {
            return Err(Poisoned);
        }
        keys.push(entry::text_of(key).unwrap_or_default());
    }
    Ok(keys)
}

fn collection(
    value: u64,
    kind: Kind,
    depth: u32,
    walk: &mut Walk,
    slot: u32,
) -> Result<String, Poisoned> {
    let label = match kind {
        Kind::Map => "Map",
        Kind::Set => "Set",
    };
    let pairs = entries_via_for_each(value)?;
    if depth > MAX_DEPTH {
        return Ok(format!("[{label}]"));
    }
    if pairs.is_empty() {
        return Ok(format!("{label}(0) {{}}"));
    }
    walk.push(slot);
    let mut printed = Vec::with_capacity(pairs.len());
    for (member, key) in &pairs {
        let result = match kind {
            // A `Set` calls back with the member as both `value` and `key`
            // (`s.forEach((value, value, set) => …)`, `set.rs`'s own words) —
            // one element per entry.
            Kind::Set => inspect(*member, depth + 1, walk).map(|text| text),
            Kind::Map => {
                let k = inspect(*key, depth + 1, walk)?;
                let v = inspect(*member, depth + 1, walk)?;
                Ok(format!("{k} => {v}"))
            }
        };
        match result {
            Ok(text) => printed.push(text),
            Err(poisoned) => {
                walk.pop();
                return Err(poisoned);
            }
        }
    }
    walk.pop();
    Ok(format!("{label}({}) {{ {} }}", pairs.len(), printed.join(", ")))
}

/// The `[value, key]` pairs a `Map` or `Set`'s own `forEach` calls back with.
///
/// Goes through `forEach` — a call into the program, exactly as `for (const x of
/// m)` in compiled code would reach `collections::iterated` — rather than
/// reading `rts-core`'s collection table directly, which is private to that
/// crate (`entry::collections::Table`) for the reason its own module doc gives:
/// the entries live beside the cell, addressed by region index, and nothing
/// outside that crate is wired to that table. A program that replaced
/// `Map.prototype.forEach` would be seen here through its replacement, which is
/// the one divergence from Node's own `util.inspect` — it reads an internal
/// slot no override can reach. Named rather than hidden: this is what "ask the
/// machine" costs when the machine's collection state has no public reader.
/// The frame lines `new Error().stack` carries, for `console.trace` — built
/// exactly the way a program's own `try { throw new Error() } catch (e) {}`
/// would read them, because that is the one place this engine writes a call
/// stack out as text (`rts-core`'s `throw::stack_text`, read back here
/// through the property it wrote rather than through a second copy of the
/// frame walk).
pub fn stack_frames() -> Result<String, Poisoned> {
    let ctor = global_get(well_known("Error"));
    if poisoned() {
        return Err(Poisoned);
    }
    let undefined = undefined_value();
    let error = entry::construct(ctor, undefined, undefined, undefined, undefined);
    if poisoned() {
        return Err(Poisoned);
    }
    let stack = get_property(error, well_known("stack"));
    if poisoned() {
        return Err(Poisoned);
    }
    let text = entry::text_of(stack).unwrap_or_default();
    // The first line is the header `Error` (an empty message), which
    // `console.trace` supplies its own message for instead.
    Ok(text.splitn(2, '\n').nth(1).map(|rest| format!("\n{rest}")).unwrap_or_default())
}

/// `JSON.stringify(value)`, for `%j` — through the global exactly as a program
/// spelling `JSON.stringify(x)` would reach it, rather than a second serialiser
/// this module would have to keep agreeing with the real one.
pub fn json_stringify(value: u64) -> Result<String, Poisoned> {
    let json = global_get(well_known("JSON"));
    if poisoned() {
        return Err(Poisoned);
    }
    let stringify = get_property(json, well_known("stringify"));
    if poisoned() {
        return Err(Poisoned);
    }
    let undefined = undefined_value();
    let result = call(stringify, undefined, value, undefined, undefined, undefined);
    if poisoned() {
        return Err(Poisoned);
    }
    Ok(entry::text_of(result).unwrap_or_else(|| "undefined".to_owned()))
}

fn entries_via_for_each(value: u64) -> Result<Vec<(u64, u64)>, Poisoned> {
    let for_each = get_property(value, well_known("forEach"));
    if poisoned() {
        return Err(Poisoned);
    }
    thread_local! {
        static COLLECTED: std::cell::RefCell<Vec<Vec<(u64, u64)>>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    extern "C" fn sink(_env: u64, _this: u64, value: u64, key: u64, _map: u64, _d: u64) -> u64 {
        COLLECTED.with(|frames| {
            if let Some(top) = frames.borrow_mut().last_mut() {
                top.push((value, key));
            }
        });
        undefined_value()
    }
    COLLECTED.with(|frames| frames.borrow_mut().push(Vec::new()));
    let callback = with_runtime(|context| make_callable(context, sink));
    let undefined = undefined_value();
    call(for_each, value, callback, undefined, undefined, undefined);
    let threw = poisoned();
    let pairs = COLLECTED.with(|frames| frames.borrow_mut().pop().unwrap_or_default());
    if threw { Err(Poisoned) } else { Ok(pairs) }
}
