//! rts-primitives — classes PRIMORDIAIS da linguagem (o único conjunto que o
//! motor/codegen pode citar diretamente). Tudo o mais resolve dinamicamente pelo
//! Registry. Conjunto: String, Object, Array, Function, Promise, Boolean, Number,
//! Error (+ TypeError/RangeError/ReferenceError/SyntaxError/URIError/EvalError/
//! AggregateError).
//!
//! Fase 2 (extração incremental): os módulos primordiais migram de `rts-shared`
//! p/ cá um a um, com gate de build+suíte a cada passo. Crate só depende de
//! `rts-engine` (universal/wasm-safe).

pub mod gc_surface;

/// Embedded TypeScript source of the PRIMORDIAL `Error` family (Error +
/// TypeError/RangeError/ReferenceError/SyntaxError/URIError/EvalError/
/// AggregateError). The new engine `include`s this as a declarations-only
/// prelude: the user's `new Error("x")` constructs this `.ts` class (a shape-
/// based object), `.message`/`.name`/`.stack` are ordinary slots, `.stack` is a
/// REAL `engine.trace_capture()` trace, `toString()` is the `.ts` method, and
/// `instanceof` rides the normal user-class inheritance chain. This replaces the
/// former hardcoded codegen synth + `__rtsadp_err_*` trampolines.
///
/// Must be concatenated BEFORE the Map/Set stdlib so the error SUBCLASSES (which
/// `extends Error`) see the `Error` base declared first (one merged prelude
/// program; declaration order within the include string matters).
pub const ERROR_TS: &str = include_str!("error.ts");

// Object is now Rust-only — the `.ts` prelude (`OBJECT_TS`, `object.ts`) was
// DELETED, following the same migration Boolean/Number/String proved.
// `hasOwnProperty`/`isPrototypeOf`/`propertyIsEnumerable` were ALREADY native
// (never read the `.ts` class): `front/run/method.rs::try_object_protocol_method`
// routes them straight to `__rtsadp_has_own`/`__rtsadp_is_prototype_of`/
// `__rtsadp_prop_is_enumerable` before any class dispatch runs. `Object(x)` /
// `new Object(x)` (the former `ObjectFactory`) is now the Rust trampoline
// `__rtsadp_obj_factory` (`rts-runtime/src/adapters/value/objops.rs`), called
// directly from `front/run/globals.rs` / `front/run/newexpr.rs` — no Registry
// class needed, since it never dispatches by method name.
//
// GAP NOT CLOSED BY THIS MIGRATION: the `.ts` class's `toString()` /
// `toLocaleString()` / `valueOf()` INSTANCE METHODS (explicit `obj.toString()`
// call syntax on a plain/whole-heap object) had no Registry/native replacement
// registered at the time of this migration — `front/run/method.rs`'s
// `try_primitive_class_method(.., "Object", ..)` MIGRATED branch
// (`registry::class_member("Object", ..)`) resolves nothing because no
// `#[rtse::class("Object", ..)]` exists. The RUNTIME-side piece already exists
// (`rtsadp_dyn_to_string` in `rts-runtime/src/adapters/value/dyndispatch.rs`
// already returns `"[object Object]"` for a keyed object via the engine's ONE
// ToString path — reuse it, do not duplicate); `valueOf` needs no call at all
// (identity — return the receiver unchanged). The missing piece is WIRING in
// `front/run/method.rs`'s `is_whole_heap_value` branch, mirroring
// `try_object_protocol_method`'s pattern. See
// `crates/rts-codegen-new/src/front/run/tests/object_class.rs::to_string_default_tag`.

// Boolean is now a pure-Rust `#[rtse::class("Boolean", value)]` value-class
// (`boolean.rs`) — the `.ts` prelude (`BOOLEAN_TS`) was DELETED, following the
// SAME migration `String` proved (`string/value_class.rs`). Instance dispatch:
// proven (`try_primitive_class_method`, MIGRATED branch) + the `Boolean(x)`
// call-without-`new` form via `#[rtse::functioncall]`
// (`registry::class_functioncall`). The macro generates every Boolean symbol;
// nothing hardcoded in the engine.

// Number is now a pure-Rust `#[rtse::class("Number", value)]` value-class
// (`number/mod.rs`) — the `.ts` prelude (`NUMBER_TS`) was DELETED, following the
// SAME migration Boolean/String proved. Instance dispatch: proven
// (`try_primitive_class_method`, MIGRATED branch); the `Number(x)` call-without-
// `new` form stays on `front/run/globals.rs`'s `"Number"` arm (see
// `number/mod.rs`'s module doc for why it does NOT declare
// `#[rtse::functioncall]`, unlike Boolean). The macro generates every Number
// ctor/instance-method symbol; statics/constants (`isNaN`/`MAX_SAFE_INTEGER`/…)
// are hand-written in `number/statics.rs` (a `const` cannot live in a
// `#[rtse::class]` `impl` block) and merge onto the same Registry class entry.

// String is now a pure-Rust `#[rtse::class("String", value)]` value-class
// (`string/value_class.rs` + `string/strops.rs`) — the `.ts` prelude (`STRING_TS`)
// was DELETED. Instance dispatch: proven (`try_primitive_class_method`) +
// dynamic/computed/method-as-value (`runtime_ci` + `funcops::prim_method_value`) +
// wrapper ToPrimitive; variadic statics via `globals::string_static_call`. The
// macro generates every String symbol; nothing hardcoded in the engine.

pub mod array;
pub mod arraybuffer;
pub mod boolean;
pub mod error;
pub mod finalization_registry;
pub mod function;
pub mod number;
pub mod object;
pub mod promise;
pub mod proxy;
pub mod reflect;
pub mod regexp;
pub mod string;
pub mod symbol;
pub mod weakref;
