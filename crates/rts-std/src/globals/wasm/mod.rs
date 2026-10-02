//! The `WebAssembly` global.
//!
//! # Why this lives here and not in `rts-core`
//!
//! `rts-core`'s rule 1 is availability — present on every target, including wasm
//! — and `WebAssembly` is not ECMAScript: it is the WebAssembly JS-API, a host
//! surface in the same class as `fetch`, `URL` and the WHATWG event globals, all
//! of which are here for the same reason. On a wasm target the `WebAssembly` a
//! program should see is the EMBEDDER's, not an interpreter this engine carries
//! inside the one it is already running in.
//!
//! # What this lot does and does not answer
//!
//! It answers the five names a package feature-detects and then uses —
//! `validate`, `compile`, `instantiate`, `Module`, `Instance` — plus the three
//! error classes, `Module.exports` and `Module.imports`, and exported functions
//! that are genuinely callable.
//!
//! Four things are deliberately absent, each because answering it would mean a
//! surface that cannot do what its name means (which is the rule the `sync`
//! namespace was deleted under):
//!
//! | absent | why, and what it waits on |
//! |---|---|
//! | `Memory.buffer` as SHARED storage | the bytes are copied in and out around
//! each exported call rather than being one allocation both sides address. That
//! is observationally a shared memory — `memory.rs` has the argument, and the one
//! thing that would have broken it, an import, is answered by mirroring at every
//! traversal of control instead — at two copies of the memory per traversal.
//! Waits on an `ArrayBuffer` over foreign bytes in `rts-core`, which closes this
//! and `napi_create_external_buffer` with one mechanism |
//! | an exported function of MORE THAN FOUR parameters | the fifth and later
//! arguments read `undefined`, hence 0, which is the four-slot limit every native
//! in this workspace has — `instance.rs` states it and `#[rtse::class]` refuses a
//! fifth argument by name. **This is the blocker a real program meets now**, and it
//! is measured rather than predicted: `whatsapp-rust-bridge`'s `hkdf` is reached as
//! `wasm.hkdf(retptr, ptr0, len0, length, addHeapObject(info))`, so `info` arrives
//! as heap slot 0 and the bridge answers `invalid type: unit value, expected struct
//! HkdfInfo`. Waits on a native reading the argument vector, which is not this
//! surface's to add. This row replaced "an import object", which the lot before
//! this one supplied and which stood here stale |
//! | `Table`, `Global` | nothing hands either one IN, which is the only way a
//! module asks for one |
//! | an `i64` as a `BigInt` | crosses as a `Number`. Recorded in `instance.rs` |
//!
//! A program that reaches one of those gets a named failure at the call, never a
//! wrong answer — which is the half of the rule that makes shipping the rest of
//! it honest.

mod errors;
mod imports;
mod instance;
mod memory;
mod module;
mod order;
mod reentry;
mod store;

use rts_core::entry::{self, Context};

/// Installs `WebAssembly` as a global object.
pub fn install(context: &mut Context) {
    let namespace = entry::make_object(context);
    let validate = entry::make_callable(context, module::validate);
    entry::describe_callable(context, validate, "validate", 1);
    entry::put_member(context, namespace, "validate", validate);
    let compile = entry::make_callable(context, compile);
    entry::describe_callable(context, compile, "compile", 1);
    entry::put_member(context, namespace, "compile", compile);
    let instantiate = entry::make_callable(context, instantiate);
    entry::describe_callable(context, instantiate, "instantiate", 1);
    entry::put_member(context, namespace, "instantiate", instantiate);
    let module_class = module::class(context);
    entry::put_member(context, namespace, "Module", module_class);
    let instance_class = instance::class(context);
    entry::put_member(context, namespace, "Instance", instance_class);
    let memory_class = memory::class(context);
    entry::put_member(context, namespace, "Memory", memory_class);
    for which in errors::Which::ALL {
        let class = errors::class(context, which);
        entry::put_member(context, namespace, which.name(), class);
    }
    entry::declare_global(context, "WebAssembly", namespace);
}

/// Raises the right class for a failure that came out of [`imports::define`] or
/// out of linking.
///
/// Which class is not a detail: node answers a `TypeError` when the import
/// NAMESPACE is missing or is not an object, and a `LinkError` when a member of a
/// namespace that exists is missing or is the wrong type. Measured, because the
/// two look like one question and a program branches on them differently.
fn refuse_link(site: &str, why: &str) {
    let message = format!("{site}: {}", imports::plain(why));
    match imports::is_namespace_fault(why) {
        true => entry::throw_type_error(&message),
        false => errors::raise(errors::Which::Link, &message),
    }
}

/// `WebAssembly.compile(bytes)` — a promise of a `Module`.
///
/// Settled before it is returned, which every promise a host builds here does:
/// there is no thread to compile on, so the work has already happened and the
/// promise records its outcome. A program that `await`s it sees no difference;
/// one that measured when the microtask ran would.
extern "C" fn compile(_e: u64, _this: u64, source: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let promise = entry::promise_new();
    let bytes = entry::with_runtime(|context| entry::buffer_source_bytes(context, source));
    let Some(bytes) = bytes else {
        let error =
            errors::make(errors::Which::Compile, "WebAssembly.compile(): expected a BufferSource");
        entry::promise_settle(promise, error, 1);
        return promise;
    };
    match store::compile(&bytes) {
        Ok(at) => {
            let made = module::made(entry::undefined_value(), at);
            entry::promise_settle(promise, made, 0);
        }
        Err(why) => {
            let error =
                errors::make(errors::Which::Compile, &format!("WebAssembly.compile(): {why}"));
            entry::promise_settle(promise, error, 1);
        }
    }
    promise
}

/// `WebAssembly.instantiate(bytes)` — a promise of `{ module, instance }`.
///
/// The JS-API's other overload takes a `Module` and answers the `Instance` alone.
/// Both are served, and which one it is is decided by whether the argument is a
/// `Module` — exactly as the specification decides it.
extern "C" fn instantiate(_e: u64, _this: u64, source: u64, imports: u64, _c: u64, _d: u64) -> u64 {
    let promise = entry::promise_new();
    let absent = entry::undefined_value();
    let (module_at, pair) = match module::index_of(source) {
        Some(at) => (Some(at), false),
        None => {
            let bytes = entry::with_runtime(|context| entry::buffer_source_bytes(context, source));
            match bytes {
                None => (None, true),
                Some(bytes) => match store::compile(&bytes) {
                    Ok(at) => (Some(at), true),
                    Err(why) => {
                        let error = errors::make(
                            errors::Which::Compile,
                            &format!("WebAssembly.instantiate(): {why}"),
                        );
                        entry::promise_settle(promise, error, 1);
                        return promise;
                    }
                },
            }
        }
    };
    let Some(module_at) = module_at else {
        let error = errors::make(
            errors::Which::Compile,
            "WebAssembly.instantiate(): expected a BufferSource",
        );
        entry::promise_settle(promise, error, 1);
        return promise;
    };
    match store::instantiate(module_at, imports) {
        Ok(at) => {
            let made = instance::made(absent, at, module_at);
            let settled = match pair {
                false => made,
                true => {
                    let module_object = module::made(absent, module_at);
                    entry::with_runtime(|context| {
                        let object = entry::make_object(context);
                        entry::put_member(context, object, "module", module_object);
                        entry::put_member(context, object, "instance", made);
                        object
                    })
                }
            };
            entry::promise_settle(promise, settled, 0);
        }
        Err(why) => {
            // The same division `refuse_link` draws, settled rather than thrown:
            // a rejected promise carries the value, so the class still has to be
            // the right one.
            let message = format!("WebAssembly.instantiate(): {}", imports::plain(&why));
            let error = match imports::is_namespace_fault(&why) {
                true => errors::make_type_error(&message),
                false => errors::make(errors::Which::Link, &message),
            };
            entry::promise_settle(promise, error, 1);
        }
    }
    promise
}
