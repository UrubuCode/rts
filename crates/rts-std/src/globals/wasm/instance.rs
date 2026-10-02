//! `WebAssembly.Instance`, its `exports` object, and the one trampoline every
//! exported wasm function is reached through.
//!
//! # The trampoline, and why there is only one
//!
//! An exported function has to be an ordinary JavaScript callable — `typeof` says
//! `"function"`, it is called with `(a, b)` and nothing in `functions::call` finds
//! out that the body is wasm. A native here is a bare `extern "C"` function
//! pointer, so N exports cannot be N Rust functions.
//!
//! What distinguishes them is the ENVIRONMENT. `entry::closure_new(code,
//! environment)` is the entry point compiled closures are built with, and it takes
//! the environment a native is otherwise given `undefined` for — so each export is
//! the same code address over a different number, and that number is its row in
//! `store.rs`'s export table. `native.rs` says "a native closes over nothing. The
//! slot exists because every callable has one"; this is the first thing to use it.
//!
//! The alternative was a table of pre-generated trampolines (`export_0`,
//! `export_1`, …) each holding its index as a constant, which is what a host with
//! no closure entry point has to do. It caps the number of live exports at
//! whatever the table's length is, and the cap is reached by a program, not by a
//! test — `sharp`'s Emscripten module exports several hundred.

use super::{errors, store};
use rts_core::entry;
use wasmi::core::ValueType;

/// The property an `Instance` carries its index in. See `module.rs` for why the
/// state is a stamped number rather than an `Aside<T>`.
const STAMP: &str = "__wasmInstance__";

/// `WebAssembly.Instance` — the constructor.
pub(super) fn class(context: &mut entry::Context) -> u64 {
    let prototype = entry::make_prototype(context, "Instance", &[]);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, "Instance", 1);
    ctor
}

/// `new WebAssembly.Instance(module, imports?)`.
extern "C" fn construct(_e: u64, this: u64, module: u64, imports: u64, _c: u64, _d: u64) -> u64 {
    let Some(module_at) = super::module::index_of(module) else {
        entry::throw_type_error("WebAssembly.Instance(): first argument must be a Module");
        return entry::undefined_value();
    };
    match store::instantiate(module_at, imports) {
        Ok(at) => made(this, at, module_at),
        Err(why) => {
            super::refuse_link("WebAssembly.Instance()", &why);
            entry::undefined_value()
        }
    }
}

/// An `Instance` object over an already-live instance, with its `exports` filled.
///
/// `WebAssembly.instantiate` reaches this too, which is why it is not inside
/// [`construct`].
pub(super) fn made(this: u64, at: usize, module_at: usize) -> u64 {
    let described = store::exports_of(at, module_at);
    let memories = store::exported_memories(at);
    let object = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Instance", &[]);
        let object = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, object, STAMP, entry::make_number(at as f64));
        object
    });
    // The exports object is built OUTSIDE the borrow above, because every
    // function in it is a `closure_new` and that takes a borrow of its own.
    let exports = entry::with_runtime(entry::make_object);
    for (name, kind) in &described {
        // Only functions this lot. A memory export is left out rather than
        // answered with something that cannot share its bytes — see the fixture's
        // header, and `store.rs`'s note on what the next lot adds.
        let held = match kind {
            store::Kind::Function => {
                let row = store::remember_export(at, name);
                exported(row, name)
            }
            store::Kind::Memory => match memories.iter().find(|(found, _)| found == name) {
                Some((_, memory)) => super::memory::made(entry::undefined_value(), at, *memory),
                None => continue,
            },
            // A table or a global is reachable only through an import object,
            // which this lot does not supply — `mod.rs` has the table of what is
            // absent and why each one waits on the same thing.
            store::Kind::Table | store::Kind::Global => continue,
        };
        entry::with_runtime(|context| entry::put_member(context, exports, name, held));
    }
    entry::with_runtime(|context| {
        entry::put_member(context, object, "exports", exports);
        object
    });
    object
}

/// One exported wasm function as a JavaScript callable.
fn exported(row: usize, name: &str) -> u64 {
    let environment = entry::make_number(row as f64);
    let function = entry::closure_new(call_export as *const () as usize as i64, environment);
    let arity = store::signature(row).map_or(0, |(params, _)| params.len() as u32);
    entry::with_runtime(|context| entry::describe_callable(context, function, name, arity));
    function
}

/// The one body every exported wasm function runs.
///
/// `environment` is the row in `store.rs`'s export table that [`exported`] closed
/// over; everything else about the call is read from the signature there.
extern "C" fn call_export(environment: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let Some(row) = entry::number_of(environment).map(|n| n as usize) else {
        return entry::undefined_value();
    };
    let Some((params, results)) = store::signature(row) else {
        errors::raise(errors::Which::Runtime, "a wasm export disappeared from under its callable");
        return entry::undefined_value();
    };
    // The four slots a native is handed. A wasm function of more than four
    // parameters reads `undefined` for the rest and therefore 0, which is the
    // same limit every native here has — `#[rtse::class]` refuses a fifth
    // argument by name, and reading the argument vector is what lifts it.
    let slots = [a, b, c, d];
    let mut arguments = Vec::with_capacity(params.len());
    for (at, kind) in params.iter().enumerate() {
        let held = slots.get(at).copied().unwrap_or_else(entry::undefined_value);
        arguments.push(coerced(held, *kind));
    }
    // JavaScript's bytes in, wasm's bytes out. `memory.rs` has why this is
    // observationally a shared memory and the one thing that would break it.
    let memories = super::memory::rows_of_instance(store::instance_of(row).unwrap_or(usize::MAX));
    for memory in &memories {
        super::memory::sync_in(*memory);
    }
    let produced = store::call(row, &arguments);
    for memory in &memories {
        super::memory::sync_out(*memory);
    }
    match produced {
        Ok(produced) => answer(&produced, results),
        Err(why) => {
            // A trap is a `RuntimeError` — the third of the three, and the only
            // one a successfully linked module can raise. UNLESS the trap is an
            // imported JavaScript function that threw, in which case the program's
            // own value is still in flight and raising here would replace it: see
            // `imports::invoke`, which asks rather than takes for that reason.
            if entry::thrown() == 0 {
                errors::raise(errors::Which::Runtime, &why);
            }
            entry::undefined_value()
        }
    }
}

/// A JavaScript argument as the wasm value its parameter asks for.
///
/// `ToNumber` first, which is the JS-API's own conversion and not wasmi's:
/// `add("4", 5)` answers 9 in Node, and the `as i32` below is wasm's wrapping
/// rather than a saturation — `add(2147483647, 1)` is `-2147483648`.
fn coerced(value: u64, kind: ValueType) -> wasmi::Value {
    let number = entry::number_for_host(value);
    match kind {
        ValueType::I32 => wasmi::Value::I32(number as i64 as i32),
        ValueType::I64 => wasmi::Value::I64(number as i64),
        ValueType::F32 => wasmi::Value::F32((number as f32).into()),
        ValueType::F64 => wasmi::Value::F64(number.into()),
        // A reference parameter has no number to be made from one. `null` is
        // what the JS-API converts an absent `externref` to, and a `funcref`
        // parameter is unreachable from this lot because nothing hands a
        // function in.
        ValueType::FuncRef | ValueType::ExternRef => wasmi::Value::I32(0),
    }
}

/// What a call answers: nothing, one value, or — which wasmi allows and the
/// JS-API maps to an array — several.
fn answer(produced: &[wasmi::Value], results: usize) -> u64 {
    match results {
        0 => entry::undefined_value(),
        1 => produced.first().map_or_else(entry::undefined_value, number_of),
        _ => {
            let rows: Vec<u64> = produced.iter().map(number_of).collect();
            entry::with_runtime(|context| entry::make_array_in(context, rows))
        }
    }
}

/// One wasm value as a JavaScript number.
///
/// An `i64` crosses as a `Number` and not a `BigInt`, which is a stated
/// divergence: the JS-API's BigInt integration says `i64` is a `BigInt`, and
/// `entry::make_bigint` takes text this would have to format. It is listed in the
/// fixture rather than left to be found, and no module in the way of this lot's
/// purpose returns one.
fn number_of(value: &wasmi::Value) -> u64 {
    let number = match value {
        wasmi::Value::I32(n) => f64::from(*n),
        wasmi::Value::I64(n) => *n as f64,
        wasmi::Value::F32(n) => f64::from(n.to_float()),
        wasmi::Value::F64(n) => n.to_float(),
        wasmi::Value::FuncRef(_) | wasmi::Value::ExternRef(_) => 0.0,
    };
    entry::make_number(number)
}
