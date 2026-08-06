//! What an `import` binds to, when the module is one the host provides.
//!
//! # What this is NOT
//!
//! A module system. Nothing here reads a file, resolves a path, orders an
//! evaluation, or links a cycle. Those are real and they belong above this
//! crate: reading a file is the host's, and deciding what a specifier means is
//! the language's.
//!
//! What this is: a table from a specifier to an **object**, and a read of one of
//! its properties. That is exactly enough for `import { test } from "rts:test"`,
//! which is what every file in this repository's own suite begins with, and it
//! is deliberately not enough for `import { x } from "./other.ts"` — which
//! answers `undefined` rather than pretending, and is the gap a real module
//! system fills.
//!
//! # Why an object rather than a table of names
//!
//! The reason [`super::global`] records for the global object: a namespace IS an
//! object in the language — `import * as ns` binds one, and `ns.test` is an
//! ordinary property read. A table keyed by name would answer the read and have
//! nothing to say about the namespace itself.
//!
//! And it means the number that crosses for the imported NAME is a key from the
//! registry the compiler mints from, rather than a second numbering over the
//! same names.
//!
//! # Why the specifier crosses as a literal number
//!
//! Because it is a string the compiler already has: `"rts:test"` is written in
//! the program, so it is in the literal table like every other string, and
//! handing over the text again at every import would hand over something already
//! resolved. The same decision `string_const` records.

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

impl Context {
    /// The namespace object a specifier names, if the host provided one.
    fn module_at(&self, specifier: &str) -> Option<u64> {
        self.modules
            .iter()
            .find(|(name, _)| name == specifier)
            .map(|(_, object)| *object)
    }
}

/// Registers a module the host provides, by specifier.
///
/// A linear list rather than a map: a host provides a handful of these, and the
/// same reasoning the accessor table records applies — hashing a specifier costs
/// more than walking five of them.
pub fn declare_module(context: &mut Context, specifier: &str, namespace: u64) {
    let held = context
        .modules
        .iter_mut()
        .find(|(name, _)| name == specifier);
    match held {
        Some((_, object)) => *object = namespace,
        None => context.modules.push((specifier.to_owned(), namespace)),
    }
}

/// One name imported from one module.
///
/// # Why this is a read rather than a binding
///
/// A live binding — where the exporting module reassigning `x` is seen by the
/// importer — needs the two sides to share a cell, and nothing here has two
/// sides: a host module is finished before the program starts. So an import
/// reads the namespace once, at the point the program reaches it, and that is
/// the divergence to state rather than the mechanism to fake.
///
/// `undefined` for a specifier the host did not provide, which is what makes
/// `import { x } from "./other.ts"` a program that runs and finds nothing
/// instead of one that silently reads someone else's `x`.
#[rtse::entry]
pub fn module_binding(specifier: i64, key: i64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(text) = context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|value| Value(value).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
        else {
            return absent;
        };
        let Some(namespace) = context.module_at(&text) else {
            return absent;
        };
        let Some(cell) = Value(namespace).as_slot() else {
            return absent;
        };
        let Ok(number) = u32::try_from(key) else {
            return absent;
        };
        let Some(key) = context.keys.key(number) else {
            return absent;
        };
        read_property(context, cell, crate::object::Key::Name(key)).map_or(absent, |found| found.bits())
    })
}

/// The whole namespace, for `import * as ns from "m"`.
///
/// The same lookup stopping one step earlier, which is why it is here rather
/// than a second function that could come to disagree about what a specifier
/// resolves to.
#[rtse::entry]
pub fn module_namespace(specifier: i64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|value| Value(value).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
            .and_then(|text| context.module_at(&text))
            .unwrap_or(absent)
    })
}

/// The shape a host-provided function must have.
///
/// The same one every native here has and the same one compiled code has —
/// stated once, in [`super::native`], and re-exported rather than re-spelled:
/// two spellings of a calling convention is how an argument comes to be read as
/// the wrong thing, and a wrong one is a jump with a corrupt stack rather than a
/// wrong answer.
pub type Provided = super::native::Native;

/// Builds a namespace object out of Rust functions.
///
/// # Why a host needs this and could not write it
///
/// Making a callable means allocating a cell in the region, recording a code
/// address beside it where no program can reach it, and interning each name into
/// the key registry the compiler mints from. All three are this crate's, and a
/// host reproducing any of them would be reproducing exactly the agreements
/// `rts-host-rwk` exists to hold rather than restate.
///
/// So the host says WHAT is in a module — which is its business, since
/// availability is what decides membership — and this says how one is built.
pub fn make_namespace(context: &mut Context, members: &[(&str, Provided)]) -> u64 {
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    super::native::install(context, cell, members);
    Value::from_slot(cell).bits()
}

/// The same, for a namespace that also holds already-built values.
///
/// `rts`'s `io` is an object of functions rather than a function, so a namespace
/// has to be able to hold one — and building it is the caller's, because what is
/// inside it is the caller's.
pub fn put_member(context: &mut Context, namespace: u64, name: &str, value: u64) {
    if let Some(cell) = Value(namespace).as_slot() {
        let key = context.well_known(name);
        super::objects::put(context, cell, key, value);
    }
}

/// One member of a namespace, by name.
///
/// The read half of [`put_member`], and it exists for the same reason: interning
/// the name reaches the key registry, which is this crate's.
pub fn get_member(context: &mut Context, object: u64, name: &str) -> u64 {
    let absent = undefined_of(context);
    let Some(cell) = Value(object).as_slot() else {
        return absent;
    };
    let key = context.well_known(name);
    read_property(context, cell, key).map_or(absent, |found| found.bits())
}

/// One Rust function as a callable value.
///
/// The piece [`make_namespace`] is built out of, exported because a caller
/// building an object of methods one at a time — which is what an `expect(x)`
/// is — would otherwise have to build a namespace and read it back.
pub fn make_callable(context: &mut Context, code: Provided) -> u64 {
    super::native::callable(context, code)
}

/// Runs a body with the installed context.
///
/// The public half of the borrow discipline this crate is written around: a
/// caller outside it cannot reach the thread-local, and the rule that a native
/// must not call user code while holding this borrow applies to it exactly as it
/// applies here. See [`super::native`] for what happens when it is broken — an
/// `extern "C"` frame cannot unwind, so it aborts the process.
pub fn with_runtime<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    with_current(body)
}

/// `undefined`, from outside a borrow.
pub fn undefined_value() -> u64 {
    with_current(|context| undefined_of(context))
}

/// `null`, from outside a borrow.
///
/// Not the same as `undefined` and not interchangeable with it: the two are
/// distinct singletons and a matcher comparing against the wrong one would pass
/// for the wrong value.
pub fn null_value() -> u64 {
    with_current(|context| {
        rts_cranelift::tags::encode(
            rts_cranelift::tags::TAG_SINGLETON,
            u64::from(context.singletons.null),
        )
    })
}

/// A string value over Rust text.
///
/// Interned, which is not an optimisation: two occurrences of one string in a
/// program ARE the same string, so a host handing back a fresh cell each time
/// would break the identity `===` reports for interned text.
pub fn make_string(context: &mut Context, text: &str) -> u64 {
    context.intern_value(Str::from_str(text)).bits()
}

/// The text a value holds, for a host reading an argument.
///
/// `None` for an object, whose `ToString` runs user code an entry point cannot
/// call — the boundary every conversion in this crate stops at.
pub fn text_of(value: u64) -> Option<String> {
    with_current(|context| super::text::to_text(context, Value(value))?.to_rust())
}

/// An array value holding these.
pub fn make_array(values: Vec<u64>) -> u64 {
    super::array_proto::built(values)
}

/// A boolean value.
///
/// Not a Rust `bool` handed across the boundary: that is one BYTE, and a caller
/// reading it as a word takes the callee's leftover bits — the failure that made
/// `===` answer true for two different strings in release and false in debug.
pub fn boolean_value(held: bool) -> u64 {
    Value::from_bool(held).bits()
}

/// Puts a value on the global object, under a name.
///
/// # Why a host needs this and `declare_module` is not enough
///
/// `console` is not imported. A program writes `console.log(x)` with no import
/// line at all, so the value has to be reachable by NAME — which means the
/// global object, which is this crate's and is made on demand.
///
/// The compiler decides which bare names are readable at all and refuses the
/// rest; this decides which of those actually have a value. The two sets are
/// allowed to differ, and the difference shows up as `undefined` rather than as
/// a link error — the same arrangement `global_get` already documents.
pub fn declare_global(context: &mut Context, name: &str, value: u64) {
    let Some(object) = super::global::holder(context) else {
        return;
    };
    let key = context.well_known(name);
    super::objects::put(context, object, key, value);
}

/// A number value.
///
/// The gap the first host module found: without it a byte count had to cross as
/// TEXT, which is a different type for a program to compare against. Numbers
/// need no interning — a double is the encoding itself, not a cell — which is
/// why this takes no context where [`make_string`] does.
pub fn make_number(value: f64) -> u64 {
    Value::from_f64(value).bits()
}

/// An empty object, from a context already in hand.
///
/// [`super::objects::object_new`] is an entry point and takes the ambient
/// borrow, so it can be called neither while a host holds a `&mut Context` nor
/// from inside [`with_runtime`] — the first aborts with no context installed and
/// the second with a borrow already held. Both were real: a namespace built one
/// at construction, and a native built one inside its own borrow.
pub fn make_object(context: &mut Context) -> u64 {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    match context.region.alloc(crate::heap::STRIDE, ty) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => super::alloc::heap_exhausted(context),
    }
}

/// An array holding these, from a context already in hand.
///
/// [`make_array`] reaches the ambient context, which a host building a namespace
/// does not have — `process.argv` is an array built before the program starts,
/// and the ambient form aborts there.
pub fn make_array_in(context: &mut Context, values: Vec<u64>) -> u64 {
    super::array::built_in(context, values)
}

/// The number a value holds, if it holds one.
///
/// The hole three modules reported at once: without it a native read a number by
/// asking for its TEXT and parsing that back — which is slow, and lossy at the
/// edges where a double's shortest decimal is not the double.
pub fn number_of(value: u64) -> Option<f64> {
    Value(value).numeric()
}

/// Whether a value is an array.
///
/// Also reported by three modules, each having inferred it differently: one by
/// whether `described` answered nothing, one by reading a numeric `length`. Two
/// inferences of one fact is the drift this crate keeps refusing, and neither
/// was right — a plain object with a `length` satisfied the second.
pub fn is_array(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.elements_at(cell).is_some())
    })
}

/// The text a value holds, from a context already in hand.
///
/// The context-taking half of [`text_of`], so a module reading an argument
/// object's fields can do it in ONE borrow instead of two passes — collect the
/// raw values under the borrow, then drop it to read their text was the shape
/// `path.format` had to be written in without this.
pub fn text_in(context: &Context, value: u64) -> Option<String> {
    super::text::to_text(context, Value(value))?.to_rust()
}

/// `undefined`, from a context already in hand.
pub fn undefined_in(context: &Context) -> u64 {
    undefined_of(context)
}

/// The bytes a typed array or `DataView` is a window onto.
///
/// # Nothing new is invented here
///
/// `rts-core-rwk`'s buffer module already holds every piece: a `View` records
/// which buffer cell, the byte offset and the length, and `window` answers the
/// slice. This is that, exported — the machine layer answers none of it, because
/// a buffer's bytes are a runtime table rather than anything the compiler emits.
///
/// A COPY, not a borrow: the slice is alive only while the context is, and a
/// host holding one across a call into user code would hold a reference into a
/// table that call may reallocate. The copy is what makes the boundary safe, and
/// it is why a host reading a large buffer pays for it — named rather than
/// hidden.
pub fn bytes_of(context: &Context, value: u64) -> Option<Vec<u8>> {
    let view = super::buffers::view_of(context, value)?;
    Some(super::buffers::window(context, &view)?.to_vec())
}

/// Writes bytes into a typed array's window, and answers how many landed.
///
/// Short of the window is a partial write rather than a refusal, which is what
/// `fs.readSync` needs: it answers how many bytes it read, and a buffer larger
/// than the file is the ordinary case rather than an error.
pub fn write_bytes(context: &mut Context, value: u64, at: usize, source: &[u8]) -> usize {
    let Some(view) = super::buffers::view_of(context, value) else {
        return 0;
    };
    let Some(window) = super::buffers::window_mut(context, &view) else {
        return 0;
    };
    if at >= window.len() {
        return 0;
    }
    let count = source.len().min(window.len() - at);
    window[at..at + count].copy_from_slice(&source[..count]);
    count
}

/// A `Uint8Array` over a copy of these bytes.
///
/// The one shape a host needs to ANSWER bytes with, and it is a `Uint8Array`
/// rather than a `Buffer` because this engine has no `Buffer` — which is the
/// divergence `node:fs` already states rather than a decision taken here.
pub fn make_bytes(context: &mut Context, source: &[u8]) -> u64 {
    let Some(buffer) = super::buffers::new_buffer(context, source.len()) else {
        return undefined_of(context);
    };
    let view = super::buffers::View {
        buffer,
        offset: 0,
        length: source.len(),
        kind: super::buffers::element::Kind::Uint8,
    };
    if let Some(window) = super::buffers::window_mut(context, &view) {
        window.copy_from_slice(source);
    }
    super::buffers::typed::made(context, view)
}

/// A prototype a host's instances inherit from, made once and named.
///
/// # What this is, and what `#[rtse::class]` is
///
/// The attribute builds a class from an `impl` block at COMPILE time, which is
/// how everything in this crate is written. A host outside it has an `impl`
/// block the attribute cannot see, so it needs the same thing at run time: an
/// object holding the methods, remembered under a name so a second call answers
/// the same one.
///
/// That "same one" is the whole reason this is not just [`make_namespace`].
/// `node:fs`'s `Dirent` and `EventEmitter` both hand back many instances, and
/// `a instanceof b`, `Object.getPrototypeOf(x) === Object.getPrototypeOf(y)` and
/// a method added by a program to the prototype all depend on there being ONE.
///
/// Registered through the same table `#[rtse::class]` uses, so a name taken by a
/// built-in is answered rather than shadowed — and recorded BEFORE the members
/// are installed, because installing interns names and interning allocates,
/// which is the recursion `string::prototype_of` paid for once.
pub fn make_prototype(context: &mut Context, name: &'static str, members: &[(&str, Provided)]) -> u64 {
    if let Some(made) = super::class_support::prototype(context, name) {
        return made;
    }
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    let object = Value::from_slot(cell).bits();
    super::class_support::record(context, name, object, object);
    super::native::install(context, cell, members);
    object
}

/// An object inheriting from a prototype.
///
/// The instance half of [`make_prototype`]: an ordinary object with the link
/// set, which is what makes a method found by the ordinary chain walk rather
/// than by anything knowing what a `Dirent` is.
pub fn make_instance(context: &mut Context, prototype: u64) -> u64 {
    let instance = make_object(context);
    if let Some(cell) = Value(instance).as_slot() {
        context.set_prototype(cell, prototype);
    }
    instance
}
