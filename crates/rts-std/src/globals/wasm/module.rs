//! `WebAssembly.Module`, `WebAssembly.validate`, and the two static describers.
//!
//! A `Module` is a JavaScript object carrying one number — the index of the
//! compiled module in `store.rs` — stamped as `__wasmModule__`. That is
//! `storage.rs`'s shape and `storage.rs` says why: a host crate outside
//! `rts-core` has no cell table to key an `Aside<T>` by, so the state lives in a
//! table here and the object carries the key.

use super::{errors, store};
use rts_core::entry;

/// The property a `Module` carries its index in.
pub(super) const STAMP: &str = "__wasmModule__";

/// `WebAssembly.Module` — the constructor, with its two statics.
pub(super) fn class(context: &mut entry::Context) -> u64 {
    let prototype = entry::make_prototype(context, "Module", &[]);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, "Module", 1);
    let exports = entry::make_callable(context, static_exports);
    entry::describe_callable(context, exports, "exports", 1);
    entry::put_member(context, ctor, "exports", exports);
    let imports = entry::make_callable(context, static_imports);
    entry::describe_callable(context, imports, "imports", 1);
    entry::put_member(context, ctor, "imports", imports);
    ctor
}

/// `new WebAssembly.Module(bytes)`.
///
/// Raises a `CompileError` for bytes that are not a module this engine accepts,
/// which is what the JS-API says and what a program catches.
extern "C" fn construct(_e: u64, this: u64, source: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(bytes) = entry::with_runtime(|context| entry::buffer_source_bytes(context, source)) else {
        errors::raise(errors::Which::Compile, "WebAssembly.Module(): expected a BufferSource");
        return entry::undefined_value();
    };
    match store::compile(&bytes) {
        Ok(at) => made(this, at),
        Err(why) => {
            errors::raise(errors::Which::Compile, &format!("WebAssembly.Module(): {why}"));
            entry::undefined_value()
        }
    }
}

/// A `Module` object over an already-compiled index.
///
/// `WebAssembly.compile` reaches this too, which is why it is not inside
/// [`construct`]: that one owns reading the argument, this one owns the object.
pub(super) fn made(this: u64, at: usize) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Module", &[]);
        let object = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, object, STAMP, entry::make_number(at as f64));
        object
    })
}

/// The index a `Module` object carries, for a caller that was handed one.
pub(super) fn index_of(value: u64) -> Option<usize> {
    let held = entry::with_runtime(|context| entry::get_member(context, value, STAMP));
    entry::number_of(held).map(|n| n as usize)
}

/// `WebAssembly.validate(bytes)` — true or false, never a throw.
pub(super) extern "C" fn validate(_e: u64, _this: u64, source: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let bytes = entry::with_runtime(|context| entry::buffer_source_bytes(context, source));
    // An argument that is not a BufferSource is `false` rather than a `TypeError`,
    // which is what Node answers: `validate` is the question "would this compile",
    // and bytes that are not even bytes would not.
    let answer = bytes.is_some_and(|bytes| store::validates(&bytes));
    entry::boolean_value(answer)
}

/// `WebAssembly.Module.exports(module)` — one descriptor per export.
extern "C" fn static_exports(_e: u64, _this: u64, module: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(at) = index_of(module) else {
        return refuse("WebAssembly.Module.exports(): not a Module");
    };
    let described = store::module_exports(at);
    let rows: Vec<u64> = described
        .iter()
        .map(|(name, kind)| descriptor(&[("name", name.as_str()), ("kind", kind.text())]))
        .collect();
    entry::with_runtime(|context| entry::make_array_in(context, rows))
}

/// `WebAssembly.Module.imports(module)` — one descriptor per import.
extern "C" fn static_imports(_e: u64, _this: u64, module: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(at) = index_of(module) else {
        return refuse("WebAssembly.Module.imports(): not a Module");
    };
    let described = store::module_imports(at);
    let rows: Vec<u64> = described
        .iter()
        .map(|(from, name, kind)| {
            descriptor(&[("module", from.as_str()), ("name", name.as_str()), ("kind", kind.text())])
        })
        .collect();
    entry::with_runtime(|context| entry::make_array_in(context, rows))
}

/// One descriptor object, built from pairs of strings.
///
/// Every value in the JS-API's two descriptors is a string, which is what lets
/// one function serve both: the keys differ and the shape does not.
fn descriptor(fields: &[(&str, &str)]) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        for (key, value) in fields {
            let held = entry::make_string(context, value);
            entry::put_member(context, object, key, held);
        }
        object
    })
}

/// Raises a `TypeError` and answers `undefined`.
///
/// A `TypeError` rather than one of the three wasm errors, because the argument
/// being the wrong KIND of thing is the language's complaint and not the module's
/// — which is the division the JS-API draws and Node follows.
fn refuse(message: &str) -> u64 {
    entry::throw_type_error(message);
    entry::undefined_value()
}
