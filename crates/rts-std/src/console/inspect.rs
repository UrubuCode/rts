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
//! deeper, it repeats. `ancestors` carries the cell of every container currently
//! being printed, and one already on it prints as `[Circular *1]`. Node numbers
//! distinct cycles in one structure differently; this always prints `*1` because
//! telling two cycles apart needs a first, non-printing pass over the whole
//! value, which is more machinery than one honest (and always terminating) digit
//! buys.
//!
//! # What is not implemented, by name
//!
//! A function's name. This engine does not store one as a readable property of
//! the callable — a gap in `rts-core`, not this file — so every function
//! prints as `[Function (anonymous)]`, visibly incomplete rather than a name
//! invented from nowhere. A class is indistinguishable from a plain function for
//! the same reason (`is_class_constructor` is private to that crate), so
//! `console.log(Map)` also prints `[Function (anonymous)]` where Node prints
//! `[class Map]`. Symbols fall through to `[object]`, the pre-existing answer
//! for anything unrecognised.

use rts_core::entry::{
    self, call, get_indexed, get_property, global_get, instance_of, is_array, key_number,
    make_callable, make_number, make_string, own_keys, undefined_value, with_runtime,
};

/// A throw was seen in flight while reading the program to describe it.
///
/// Carries nothing: the throw itself already lives in the context —
/// [`entry::thrown`] keeps answering true for it — and this is only the signal
/// that formatting must stop rather than print a partial answer.
pub struct Poisoned;

const MAX_DEPTH: u32 = 2;

/// Node's own spacing, for a value at the top of an argument list.
///
/// A string prints bare here and quoted everywhere it is nested —
/// `console.log("hi")` prints `hi`, `console.log(["hi"])` prints `[ 'hi' ]`.
pub fn top_level(value: u64) -> Result<String, Poisoned> {
    if is_bare_string(value) {
        // `described` cannot fail on a string: no user code runs to answer it.
        return Ok(entry::described(value).unwrap_or_default());
    }
    inspect(value, 0, &mut Vec::new())
}

/// Whether a value is a STRING — the one kind `described` also answers that
/// prints bare at the top level and quoted everywhere else.
fn is_bare_string(value: u64) -> bool {
    entry::number_of(value).is_none() && entry::described(value).is_some() && as_slot(value).is_some()
}

/// The formatted text of one value, at `depth` levels of nesting from the
/// argument that was printed.
fn inspect(value: u64, depth: u32, ancestors: &mut Vec<u32>) -> Result<String, Poisoned> {
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
            None if as_slot(value).is_none() => text, // boolean / null / undefined
            None => quoted(&text),                    // a string, nested
        });
    }
    let Some(slot) = as_slot(value) else {
        // A symbol, or a `Kind::Client` this module does not recognise.
        return Ok("[object]".to_owned());
    };
    if ancestors.contains(&slot) {
        return Ok("[Circular *1]".to_owned());
    }
    if with_runtime(|context| entry::is_callable_in(context, value)) {
        return Ok("[Function (anonymous)]".to_owned());
    }
    if let Some(kind) = collection_kind(value)? {
        return collection(value, kind, depth, ancestors, slot);
    }
    if is_array(value) {
        return array(value, depth, ancestors, slot);
    }
    object(value, depth, ancestors, slot)
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

fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

/// Whether a property name prints bare (`a: 1`) or quoted (`'a-b': 1`) — Node's
/// own rule, an identifier's shape: a letter, `_` or `$` first, then those plus
/// digits.
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
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

fn array(value: u64, depth: u32, ancestors: &mut Vec<u32>, slot: u32) -> Result<String, Poisoned> {
    if depth > MAX_DEPTH {
        return Ok("[Array]".to_owned());
    }
    let length = length_of(value)?;
    if length == 0 {
        return Ok("[]".to_owned());
    }
    ancestors.push(slot);
    let mut items = Vec::with_capacity(length);
    for index in 0..length {
        let element = get_indexed(value, make_number(index as f64));
        if poisoned() {
            ancestors.pop();
            return Err(Poisoned);
        }
        items.push(inspect(element, depth + 1, ancestors)?);
    }
    ancestors.pop();
    Ok(format!("[ {} ]", items.join(", ")))
}

fn object(value: u64, depth: u32, ancestors: &mut Vec<u32>, slot: u32) -> Result<String, Poisoned> {
    if depth > MAX_DEPTH {
        return Ok("[Object]".to_owned());
    }
    let keys = keys_of(value)?;
    if keys.is_empty() {
        return Ok("{}".to_owned());
    }
    ancestors.push(slot);
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let key_num = well_known(&key);
        let read = get_property(value, key_num);
        if poisoned() {
            ancestors.pop();
            return Err(Poisoned);
        }
        let printed = inspect(read, depth + 1, ancestors)?;
        let name = if is_identifier(&key) { key } else { quoted(&key) };
        entries.push(format!("{name}: {printed}"));
    }
    ancestors.pop();
    Ok(format!("{{ {} }}", entries.join(", ")))
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
    ancestors: &mut Vec<u32>,
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
    ancestors.push(slot);
    let mut printed = Vec::with_capacity(pairs.len());
    for (member, key) in &pairs {
        let result = match kind {
            // A `Set` calls back with the member as both `value` and `key`
            // (`s.forEach((value, value, set) => …)`, `set.rs`'s own words) —
            // one element per entry.
            Kind::Set => inspect(*member, depth + 1, ancestors).map(|text| text),
            Kind::Map => {
                let k = inspect(*key, depth + 1, ancestors)?;
                let v = inspect(*member, depth + 1, ancestors)?;
                Ok(format!("{k} => {v}"))
            }
        };
        match result {
            Ok(text) => printed.push(text),
            Err(poisoned) => {
                ancestors.pop();
                return Err(poisoned);
            }
        }
    }
    ancestors.pop();
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
