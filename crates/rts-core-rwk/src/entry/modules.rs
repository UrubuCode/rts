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
    let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
    Value::from_slot(cell).bits()
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
    with_current(|context| is_array_in(context, value))
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

/// Names registered from more than one file ON PURPOSE, exempted from
/// [`make_prototype`]'s collision panic, and each is self-healing rather than
/// a race: `"Performance"` (`rts-node-rwk`'s `perf_hooks::namespace` and
/// `rts-std-rwk`'s `timing::install`) and `"ChildProcess"`
/// (`child_process::mod`'s namespace builder and `spawn_async`'s own
/// construction) both call `make_prototype` and then reinstall their own
/// full member list onto the returned cell regardless of which caller minted
/// it — see `perf_hooks::mod::namespace`'s comment, which states the
/// idempotence explicitly. That is a different shape from the bug class the
/// panic exists for: those two callers never TRUST the returned prototype to
/// already have the members they need, so a wrong table never survives to be
/// read. Listed here rather than inferred, because "does the caller reinstall
/// afterwards" is not something this function can observe.
const SHARED_BY_DESIGN: &[&str] = &["Performance", "ChildProcess"];

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
///
/// # Idempotent by name, but a COLLISION between two owners panics
///
/// The idiom this crate uses on purpose is a "chain-read": ask for a name with
/// an EMPTY `members` slice to get the parent object back (`"EventEmitter"`,
/// `"Readable"`), never intending to define it. That case must stay silent —
/// it is how a dozen modules link an instance onto an `EventEmitter` another
/// module owns.
///
/// What must NOT stay silent is two DIFFERENT owners each calling this with a
/// real, non-empty `members` table under the same name — that is not a chain,
/// it is two classes that picked the same name, and by-name idempotence used
/// to hand the second one the first one's method table with no signal at all:
/// `fs` and `stream` both defining `"Writable"`, `http` and `net` both defining
/// `"Server"`, `dgram` and `net` both defining `"Socket"` each shipped this way
/// and were only found by reading output, one module wide at a time.
///
/// So: the first call that installs real members records its caller's file as
/// the name's owner. A later call that ALSO installs real members and comes
/// from a different file panics naming both files and the name — this can only
/// fire from a programming error inside this engine (a native module reusing a
/// name), never from anything a JS program's input can select, since which
/// native modules exist and what they register is fixed at compile time, not
/// read from a script. The trade taken is a crash rather than a silent wrong
/// method table, because the wrong table is the harder bug to ever notice.
/// [`SHARED_BY_DESIGN`] lists the two names that share by design instead.
#[track_caller]
pub fn make_prototype(context: &mut Context, name: &'static str, members: &[(&str, Provided)]) -> u64 {
    // `Location::caller()` is read HERE, directly in the `#[track_caller]`
    // body, and threaded through as a value — read lazily inside the
    // `.then(...)` closure below, it reports the closure's OWN definition
    // site (this file) instead of the propagated call site, which is exactly
    // the false collision this mechanism must not report.
    let caller = std::panic::Location::caller().file();
    let owning = !members.is_empty() && !SHARED_BY_DESIGN.contains(&name);
    if let Some(made) = super::class_support::prototype(context, name) {
        if owning {
            if let Some(existing) = super::class_support::owner(context, name) {
                if existing != caller {
                    panic!(
                        "make_prototype(\"{name}\") collision: already owned by {existing}, \
                         also claimed by {caller} — two modules registered different method \
                         tables under one prototype name; rename one (e.g. \"module.{name}\") \
                         or, if the sharing is deliberate and self-healing, add it to \
                         SHARED_BY_DESIGN with the same reasoning as its existing entries"
                    );
                }
            }
        }
        return made;
    }
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    let object = Value::from_slot(cell).bits();
    let owner = owning.then_some(caller);
    super::class_support::record(context, name, object, object, owner);
    super::native::install(context, cell, members);
    object
}

/// A string's bytes under a named encoding — the codec [`buffer_class`]'s
/// `Buffer` uses, exported so a host's own top-level functions (`node:buffer`'s
/// `atob`/`btoa`/`transcode`, which are not `Buffer` methods) reach the ONE
/// implementation rather than a second one on the other side of the crate
/// boundary. `None` for a name the codec does not recognize.
pub fn encode_text(text: &str, encoding: &str) -> Option<Vec<u8>> {
    super::buffer::codec::encode(text, encoding)
}

/// Bytes decoded to text under a named encoding — the read half of
/// [`encode_text`].
pub fn decode_bytes(bytes: &[u8], encoding: &str) -> String {
    super::buffer::codec::decode(bytes, encoding)
}

/// Whether a name is one of the encodings [`encode_text`]/[`decode_bytes`]
/// recognize, and its canonical spelling.
pub fn canonical_encoding(name: &str) -> Option<&'static str> {
    super::buffer::codec::canonical_encoding(name)
}

/// Base64 encode, standard (`true`) or URL-safe (`false`) alphabet.
pub fn encode_base64(bytes: &[u8], standard: bool) -> String {
    super::buffer::codec::encode_base64(bytes, standard)
}

/// Base64/base64url decode, permissive about which alphabet and about padding.
pub fn decode_base64(text: &str) -> Vec<u8> {
    super::buffer::codec::decode_base64(text)
}

/// The `Buffer` class, registering it if this is the first read.
///
/// A host module (`node:buffer`) cannot reach `entry::buffer` directly — it is
/// a private submodule, the same way every other declared class is — so this is
/// the small accessor `rts-node-rwk` needs to hand its own `Buffer` name back to
/// the one this crate builds, rather than fabricating a second one.
pub fn buffer_class(context: &mut Context) -> u64 {
    super::buffer::register_buffer(context)
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

/// Whether a value is an object a host can hang a property on.
///
/// What a constructor asks to tell `new C()` from `C()`: the first hands over an
/// object already linked to the class's prototype, the second hands over
/// `undefined`.
///
/// Takes the context rather than reaching for it, because a constructor asks
/// this from inside `with_runtime` — where the ambient form is a nested borrow
/// and therefore an abort.
pub fn is_object(context: &Context, value: u64) -> bool {
    Value(value)
        .as_slot()
        .is_some_and(|cell| context.region.type_of(cell).is_some())
}

/// A `Buffer` over a copy of these bytes.
///
/// [`make_bytes`] answers a `Uint8Array`, which was the only shape available
/// while `Buffer` was functions beside a typed array. It is a class now, so a
/// host answering bytes where Node answers a `Buffer` can answer one — and the
/// difference is observable: `Buffer.isBuffer` on a plain `Uint8Array` is false,
/// and so is every instance method.
///
/// Built through the class's own construction rather than by linking a
/// prototype here, so there is one answer to what a `Buffer` IS.
pub fn make_buffer(context: &mut Context, source: &[u8]) -> u64 {
    super::buffer::ops::made(context, source)
}

/// Links one object to another, from a context already in hand.
///
/// `chain::set_prototype` is an entry point and takes the ambient borrow, so a
/// host chaining one class onto another — which is what a stream does to
/// `EventEmitter` — aborts with nothing installed. The same pairing
/// [`make_object`] documents, and found the same way.
pub fn set_prototype_in(context: &mut Context, object: u64, prototype: u64) {
    if let Some(cell) = Value(object).as_slot() {
        context.set_prototype(cell, prototype);
    }
}

/// `null`, from a context already in hand.
///
/// The pair of [`null_value`], for the reason [`undefined_in`] exists: a module
/// holding the context cannot call the ambient form without aborting.
pub fn null_in(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}

/// What a host can offer that this crate cannot do: turn source text into a
/// value.
///
/// # Why this is an injection and not a call
///
/// Compiling is the host's — it owns the compiler, the placement and the region.
/// A module that wanted it could not reach up: the host DEPENDS on the module
/// crates, so a dependency the other way is a cycle. So the host hands the
/// capability down, and a module asks for it by name without knowing who
/// answered.
///
/// # Why the name says nothing about a client
///
/// `evaluate`, not `runInNewContext` or `compileScript` — the same rule that
/// keeps a language's name out of the machine layer keeps a module's name out of
/// this one. What crosses is the operation; what a `node:vm` or a `repl` calls
/// it is theirs.
///
/// `None` until a host installs one, which is what makes a module able to say
/// "this engine cannot evaluate source" rather than crash trying.
pub type Evaluator = fn(&str) -> Option<u64>;

/// Installs the host's evaluator.
pub fn declare_evaluator(context: &mut Context, evaluator: Evaluator) {
    context.evaluator = Some(evaluator);
}

/// Compiles and runs source text, answering what it produced.
///
/// `None` when no host installed an evaluator, or when the source did not
/// compile — the two are deliberately one answer here, because a module asking
/// this cannot act differently on them and a second channel to tell them apart
/// would be a mechanism nothing uses.
pub fn evaluate(source: &str) -> Option<u64> {
    let evaluator = with_current(|context| context.evaluator)?;
    evaluator(source)
}

/// A bigint over a whole number, from a context already in hand.
///
/// # Why a host needed this
///
/// `node:sqlite` reads a real `i64` out of a database and had nowhere to put it:
/// the only bigint constructor here parses TEXT and takes the ambient borrow, so
/// a native holding the context could not call it. Every INTEGER became a
/// double, and one past `Number.MAX_SAFE_INTEGER` **silently rounded** — a wrong
/// answer that runs, which is the outcome this project refuses everywhere else.
///
/// `from_i64` already existed on the value; what was missing was a way to reach
/// it from outside this crate.
pub fn make_bigint(context: &mut Context, value: i64) -> u64 {
    context.bigint_value(crate::bigint::BigInt::from_i64(value))
}

/// The host's evaluator itself, for a caller that will use it somewhere this
/// thread's context cannot be reached.
///
/// # Why the pointer and not another `evaluate`
///
/// [`evaluate`] reads the evaluator off THIS thread's context and calls it here.
/// A caller starting a second thread cannot do that on the far side: a context
/// is thread-local, the new thread has none, and the first thing it would do is
/// abort. A `fn` pointer is `Copy` and `Send`, so taking it here and carrying it
/// across is the whole of what a second thread needs — it installs its own
/// context when it runs, which is what makes the two independent.
///
/// `None` when no host installed one, the same answer [`evaluate`] gives.
pub fn evaluator() -> Option<Evaluator> {
    with_current(|context| context.evaluator)
}

/// Whether a value is an array, from a context already in hand.
///
/// The context-taking half of [`is_array`], and the eighth pair of this shape.
/// It is here because the ambient form takes its own borrow, so asking it from
/// inside [`with_runtime`] is a nested borrow — a panic in an `extern "C"`
/// frame, which cannot unwind and therefore **aborts the process**. Any walk
/// over a value's structure holds a context by construction, so the ambient
/// form is unusable there rather than merely slower.
pub fn is_array_in(context: &Context, value: u64) -> bool {
    Value(value)
        .as_slot()
        .is_some_and(|cell| context.elements_at(cell).is_some())
}

/// Every enumerable own property name of an object, from a context already in
/// hand.
///
/// # Why a host needed this
///
/// `node:worker_threads` copies a value out of one region and rebuilds it in
/// another, and had no way to ask what an object's properties are — so a plain
/// object crossed as a marker, or, worse, as an empty object, which is the
/// answer that looks like it worked. Nothing on this surface enumerated a cell.
///
/// The walk itself is `Object.keys`'s, reached rather than repeated: element
/// indices first as strings, then the shape's properties minus the
/// non-enumerable and the symbol-keyed, then accessors. Two walks would be two
/// answers to what an object's keys are.
pub fn member_names(context: &mut Context, object: u64) -> Vec<String> {
    super::array::key_texts(context, object, true)
        .into_iter()
        .filter_map(|text| text.to_rust())
        .collect()
}

/// The text a value holds **only when it really is a string**, from a context
/// already in hand.
///
/// # Why this is not [`text_in`]
///
/// [`text_in`] is `ToString`: it answers `"42"` for the number `42` and `"true"`
/// for a boolean, which is right for printing and wrong for asking what
/// something is. `node:worker_threads` asked it that second question while
/// copying a value out of a region, so every number crossed as a string — and
/// the copy arrived looking correct, right up to `1 + 2` answering `"12"`.
///
/// That is the shape of defect this repository refuses, and the fix is a
/// predicate rather than a convention about when to call which: a coercion that
/// can be mistaken for a test will be.
pub fn string_in(context: &Context, value: u64) -> Option<String> {
    let slot = Value(value).as_slot()?;
    context.text_at(slot)?.to_rust()
}

/// Whether a value can be called, from a context already in hand.
///
/// # Why a host needed this
///
/// Node's own signatures overload on it: `fs.watch(path[, options], listener)`
/// decides what its second argument IS by asking whether it is a function. A
/// module that cannot ask reads the listener as an options object and registers
/// nothing — which is exactly what `fs.watch` did, silently, for as long as no
/// test called it with the two-argument form every program uses.
///
/// Context-taking because every such check happens while reading arguments, and
/// arguments are read inside a borrow.
pub fn is_callable_in(context: &Context, value: u64) -> bool {
    Value(value)
        .as_slot()
        .is_some_and(|cell| context.callable_at(cell).is_some())
}

/// Publishes one exported binding into the specifier table.
///
/// # Why an export is a write to the table an import reads
///
/// Because there is then ONE place that decides what a specifier resolves to,
/// for a host-provided module and a compiled one alike. The alternative — a
/// second mechanism holding compiled modules' exports — is two answers to that
/// question, and the two would disagree the first time a program re-exported a
/// host module.
///
/// So `export const x = 1` in `./a.ts` puts `x` on the namespace object for
/// `"./a.ts"`, and `import { x } from "./a.ts"` reads it back through
/// [`module_binding`] with nothing new in the path.
///
/// # What is NOT live about it
///
/// A later assignment to the local `x` does not change what the importer sees.
/// A live binding needs the two sides to share a cell, which is the same
/// divergence [`module_binding`] already states — this makes the export as live
/// as the import was, and no more.
///
/// Answers the value it was given, so a caller can publish and bind in one
/// expression rather than emitting a temporary.
#[rtse::entry]
pub fn module_publish(specifier: i64, key: i64, value: u64) -> u64 {
    with_current(|context| {
        let Some(text) = context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|held| Value(held).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
        else {
            return value;
        };
        // The namespace is created on the first export rather than by whoever
        // compiles the module: a module that exports nothing has no namespace
        // to speak of, and `import * as ns` of it answering `undefined` is the
        // honest result of that rather than an empty object pretending.
        let namespace = match context.module_at(&text) {
            Some(namespace) => namespace,
            None => {
                let made = make_object(context);
                context.modules.push((text, made));
                made
            }
        };
        let Some(cell) = Value(namespace).as_slot() else {
            return value;
        };
        let Ok(number) = u32::try_from(key) else {
            return value;
        };
        let Some(key) = context.keys.key(number) else {
            return value;
        };
        super::objects::put(context, cell, crate::object::Key::Name(key), value);
        value
    })
}

/// The namespace a specifier names, from a context already in hand.
///
/// The host-facing half of [`module_namespace`], which takes a literal index a
/// compiled program holds and nothing outside this crate can mint. `undefined`
/// for a specifier nothing registered.
pub fn module_at_name(context: &Context, specifier: &str) -> u64 {
    context.module_at(specifier).unwrap_or_else(|| undefined_of(context))
}

/// Every specifier registered, in registration order.
///
/// # Why a host needed this
///
/// `node:module`'s `builtinModules` is built from `install`'s own list, which is
/// right for it — that list IS what it registered. A program asking the RUNTIME
/// what it can import is a different question, and it includes the modules a
/// compiled program published for itself, which no static list has.
pub fn module_specifiers(context: &Context) -> Vec<String> {
    context.modules.iter().map(|(name, _)| name.clone()).collect()
}

/// Removes a specifier, answering whether one was there.
///
/// # What this does and does not undo
///
/// It makes a later import of that specifier answer `undefined`. It does NOT
/// unrun the module: whatever its body did — a listener registered, a file
/// written, an object another module already holds — has happened and is not
/// reachable from here. Anything already bound keeps working, because an import
/// read the value once.
///
/// So this is "forget the name", which is what a module cache eviction actually
/// is, and calling it `delete` would suggest the module itself went away.
pub fn forget_module(context: &mut Context, specifier: &str) -> bool {
    let before = context.modules.len();
    context.modules.retain(|(name, _)| name != specifier);
    context.modules.len() != before
}

/// `export * from "m"` — every name `m` exports, published here too.
///
/// # Why this is one operation and not a list the compiler emits
///
/// Because the compiler does not know the list. `m`'s exports are decided by
/// `m`'s own body, which has already run by the time this does — the graph is
/// ordered dependencies-first — so the names exist as properties of a namespace
/// and nowhere else at compile time. A version that published nothing was what
/// the refusal this replaces was protecting against.
///
/// `default` is deliberately skipped: `export *` does not forward it, which is
/// the one rule that distinguishes it from copying the namespace wholesale.
#[rtse::entry]
pub fn module_publish_all(specifier: i64, from: i64) -> u64 {
    let source = module_namespace(from);
    let names = super::array::own_keys(source);

    // Read, then write — the keys come out of one borrow and the publication
    // takes another, because `module_publish` interns and interning allocates.
    let pairs = with_current(|context| {
        let mut pairs: Vec<(crate::object::Key, u64)> = Vec::new();
        let Some(cell) = Value(names).as_slot() else {
            return pairs;
        };
        let Some(listed) = context.elements_at(cell).cloned() else {
            return pairs;
        };
        for name in listed {
            let Some(key) = super::computed::property_key(context, Value(name)) else {
                continue;
            };
            if let crate::object::Key::Name(named) = key
                && context
                    .interner
                    .text(named)
                    .and_then(Str::to_rust)
                    .is_some_and(|text| text == "default")
            {
                continue;
            }
            if let Some(source) = Value(source).as_slot()
                && let Some(value) = super::objects::read_property(context, source, key)
            {
                pairs.push((key, value.bits()));
            }
        }
        pairs
    });

    with_current(|context| {
        let Some(text) = context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|held| Value(held).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
        else {
            return;
        };
        let namespace = match context.module_at(&text) {
            Some(namespace) => namespace,
            None => {
                let made = make_object(context);
                context.modules.push((text, made));
                made
            }
        };
        if let Some(cell) = Value(namespace).as_slot() {
            for (key, value) in pairs {
                super::objects::put(context, cell, key, value);
            }
        }
    });
    source
}
