//! The import object: a JavaScript function callable from inside a wasm body.
//!
//! # Why this is the piece everything real needs
//!
//! `wasm-bindgen` is how a Rust library reaches JavaScript, and a module it
//! produces imports one function per thing it wants done. The bridge
//! `@whiskeysockets/baileys` carries imports **99** of them, and 35 of those read
//! the linear memory. So "a module with an import raises a `LinkError`" was not a
//! gap at the edge of the surface; it was the surface not being reachable.
//!
//! # The memory, and the rule that makes it correct
//!
//! `memory.rs` mirrors the linear memory into a JavaScript `ArrayBuffer` by
//! copying, and its correctness rested on one sentence: between calls only
//! JavaScript touches the memory, during a call only wasm does. **An import
//! breaks that sentence**, which that module says in writing, and 35 of those 99
//! functions are exactly the break: they read bytes the wasm body has just
//! written, from inside the call.
//!
//! The rule that restores it is to mirror at every **traversal of control**
//! rather than at every call:
//!
//! | control passes | what happens |
//! |---|---|
//! | JavaScript → wasm (a call) | JavaScript's bytes go in |
//! | wasm → JavaScript (an import) | wasm's bytes come out, BEFORE the callee runs |
//! | JavaScript → wasm (the import returns) | JavaScript's bytes go back in |
//! | wasm → JavaScript (the call returns) | wasm's bytes come out |
//!
//! Whoever is running is the only one who can have written, so each traversal
//! carries what the side that just ran produced. Measured against node 22 by
//! `tests/claude-webassembly-imports.test.ts`, whose decisive case is a wasm body
//! that writes a byte, calls a JavaScript import that READS it and writes another,
//! and then returns — both directions inside one call.
//!
//! # Why no raw pointer, and what that avoids
//!
//! The obvious implementation of a shared memory is to hand JavaScript an
//! `ArrayBuffer` over `wasmi`'s own bytes. It is also unsound here: during a
//! callback `wasmi` holds a mutable borrow of the store those bytes live in, so a
//! `&[u8]` built from a raw pointer would alias it. `Caller` is the safe way in —
//! `wasmi` hands it to every host function precisely so one can read and write the
//! caller's memory — and it is what this module uses. The cost is the copies; the
//! alternative was undefined behaviour that works until it does not.

use super::store::HostState;
use rts_core::entry;
use std::sync::Mutex;
use wasmi::core::ValueType;
use wasmi::{Caller, Func, FuncType, Linker, Module, Store, Value};

/// Every JavaScript function an instance imports, by the row its host function
/// closed over.
///
/// A `u64` here is a reference the collector is not told about, so each one is
/// rooted with `entry::hold_current` — `rts-core`'s rule 10, and the class
/// `docs/engine/lost-roots.md` is about. Nothing is ever released: an import lives
/// as long as its instance, and this surface frees no instance (see `store.rs`).
static CALLBACKS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Defines every import a module asks for, from a JavaScript import object.
///
/// Answers the message a `LinkError` or `TypeError` should carry when it cannot.
pub(super) fn define(
    linker: &mut Linker<HostState>,
    store: &mut Store<HostState>,
    module: &Module,
    imports: u64,
) -> Result<(), String> {
    let wanted: Vec<(String, String, wasmi::ExternType)> = module
        .imports()
        .map(|import| (import.module().to_owned(), import.name().to_owned(), import.ty().clone()))
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let absent = entry::undefined_value();
    if imports == absent {
        let (from, name, _) = &wanted[0];
        return Err(format!("import object is required, for `{from}`::`{name}`"));
    }
    for (from, name, ty) in &wanted {
        let found = member(imports, from, name)?;
        match ty {
            wasmi::ExternType::Func(signature) => {
                let row = remember(found);
                let func = host_function(store, signature.clone(), row);
                linker
                    .define(from, name, func)
                    .map_err(|error| format!("`{from}`::`{name}`: {error}"))?;
            }
            // A table, a global or a memory handed IN. Each is a value this
            // surface can build but not yet accept from a program, and the
            // message names which one rather than reporting a missing import —
            // `mod.rs` keeps the list of what is absent and why.
            other => {
                let kind = super::store::kind_of_type(other).text();
                return Err(format!(
                    "`{from}`::`{name}`: importing a {kind} is not supported yet, only a function"
                ));
            }
        }
    }
    Ok(())
}

/// `imports[from][name]`, refusing the two ways it can be wrong.
///
/// The two refusals are different in the language and measured that way: a
/// MISSING namespace is a `TypeError` in node, where a missing member of a
/// namespace that exists is a `LinkError`. The caller turns the message into one
/// or the other by reading [`is_namespace_fault`].
fn member(imports: u64, from: &str, name: &str) -> Result<u64, String> {
    let namespace = entry::with_runtime(|context| entry::get_member(context, imports, from));
    let is_object = entry::with_runtime(|context| entry::is_object(context, namespace));
    if !is_object {
        return Err(format!("{NAMESPACE_FAULT}module=\"{from}\" is not an object or function"));
    }
    let found = entry::with_runtime(|context| entry::get_member(context, namespace, name));
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, found));
    match callable {
        true => Ok(found),
        false => Err(format!("`{from}`::`{name}` is not a function")),
    }
}

/// The marker that says a message is about a missing NAMESPACE.
///
/// A marker in the text rather than a second error type, because the message is
/// what a program reads and the distinction is only in which class raises it —
/// two `Result` kinds for one string would be the second answer this workspace
/// keeps refusing.
const NAMESPACE_FAULT: &str = "\u{1}";

/// Whether a message from [`define`] is the `TypeError` case.
pub(super) fn is_namespace_fault(message: &str) -> bool {
    message.contains(NAMESPACE_FAULT)
}

/// The message without its marker.
pub(super) fn plain(message: &str) -> String {
    message.replace(NAMESPACE_FAULT, "")
}

/// Records a JavaScript callable, answering the row a host function closes over.
fn remember(callable: u64) -> usize {
    entry::hold_current(callable);
    let mut held = CALLBACKS.lock().expect("the import callback lock");
    held.push(callable);
    held.len() - 1
}

/// One host function over the JavaScript callable at `row`.
fn host_function(store: &mut Store<HostState>, signature: FuncType, row: usize) -> Func {
    let results_wanted: Vec<ValueType> = signature.results().to_vec();
    Func::new(store, signature, move |mut caller: Caller<'_, HostState>, given, produced| {
        let memory_row = caller.data().row;
        // wasm → JavaScript: what the body has written so far.
        mirror_out(&mut caller, memory_row);
        // The `Caller` is registered for the duration of the JavaScript call and
        // only for it: an export of THIS instance called from inside the callee
        // has no other `&mut Store` to run on, and `__wbindgen_malloc` is that
        // call. The mirroring above and below stays OUTSIDE the closure because
        // `reentry`'s invariant is that this borrow is not touched while the
        // frame is registered — folding them in would break it.
        let answer = super::reentry::with_active(&mut caller, memory_row, || invoke(row, given));
        // JavaScript → wasm: what the callee wrote, before the body resumes.
        mirror_in(&mut caller, memory_row);
        let answer = answer?;
        for (slot, kind) in produced.iter_mut().zip(results_wanted.iter()) {
            *slot = as_wasm(answer, *kind);
        }
        Ok(())
    })
}

/// Calls the JavaScript function, with the wasm arguments as numbers.
///
/// Rule 8 of `rts-core`'s README applies here and is the reason for the second
/// question: a native that calls user code asks whether it threw BEFORE reading
/// the answer. A throw becomes a trap, which unwinds the wasm body and comes back
/// out of `store::call` as the `RuntimeError` the JS-API says it is.
fn invoke(row: usize, given: &[Value]) -> Result<u64, wasmi::core::Trap> {
    let callable = CALLBACKS
        .lock()
        .expect("the import callback lock")
        .get(row)
        .copied()
        .ok_or_else(|| wasmi::core::Trap::new("an imported function disappeared"))?;
    let absent = entry::undefined_value();
    let mut slots = [absent; 4];
    for (slot, value) in slots.iter_mut().zip(given.iter()) {
        *slot = as_number(value);
    }
    // Four, because that is what a call carries here. A wasm import of more
    // parameters gets `undefined` for the rest — the same limit every native in
    // this workspace has, and `wasm-bindgen` emits none above four.
    let produced = entry::call(callable, absent, slots[0], slots[1], slots[2], slots[3]);
    // ASKED, not taken. `entry::thrown()` reads the tag without clearing the slot,
    // so the throw stays in flight and the compiled frame above `store::call`
    // re-raises the program's OWN value — where `take_thrown` here would consume
    // it and leave `call_export` to invent a `RuntimeError` in its place, turning
    // the program's `TypeError` into a different error of a different class.
    match entry::thrown() != 0 {
        true => Err(wasmi::core::Trap::new("an imported function threw")),
        false => Ok(produced),
    }
}

/// A wasm value as a JavaScript number.
fn as_number(value: &Value) -> u64 {
    let number = match value {
        Value::I32(n) => f64::from(*n),
        Value::I64(n) => *n as f64,
        Value::F32(n) => f64::from(n.to_float()),
        Value::F64(n) => n.to_float(),
        Value::FuncRef(_) | Value::ExternRef(_) => 0.0,
    };
    entry::make_number(number)
}

/// A JavaScript value as the wasm value a result slot asks for.
fn as_wasm(value: u64, kind: ValueType) -> Value {
    let number = entry::number_for_host(value);
    match kind {
        ValueType::I32 => Value::I32(number as i64 as i32),
        ValueType::I64 => Value::I64(number as i64),
        ValueType::F32 => Value::F32((number as f32).into()),
        ValueType::F64 => Value::F64(number.into()),
        ValueType::FuncRef | ValueType::ExternRef => Value::I32(0),
    }
}

/// Copies the caller's linear memory into the JavaScript buffer.
fn mirror_out(caller: &mut Caller<'_, HostState>, row: usize) {
    let Some(memory) = caller.get_export("memory").and_then(wasmi::Extern::into_memory) else {
        return;
    };
    let bytes = memory.data(&caller).to_vec();
    super::memory::push_to_js(row, &bytes);
}

/// Copies the JavaScript buffer back into the caller's linear memory.
fn mirror_in(caller: &mut Caller<'_, HostState>, row: usize) {
    let Some(memory) = caller.get_export("memory").and_then(wasmi::Extern::into_memory) else {
        return;
    };
    let Some(bytes) = super::memory::pull_from_js(row) else { return };
    let window = memory.data_mut(caller);
    let count = bytes.len().min(window.len());
    window[..count].copy_from_slice(&bytes[..count]);
}
