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

/// Embedded TypeScript source of the PRIMORDIAL `Object` instance-method library
/// + factory. Object is NOT a primitive with an autobox — every `{}` is already a
/// shape-based object, so the methods read `this` AS THE OBJECT (no `__prim`).
/// `obj.hasOwnProperty(k)`/`.toString()`/`.valueOf()`/… route into this ambient
/// `class Object` via the OBJECT-receiver dispatch in the new engine, with the
/// object as `this`; presence checks ride the shape-aware `engine.obj_has` bridge.
/// The STATIC surface (`Object.keys`/…) stays codegen-native (shape-based) and is
/// transparent to this instance-only class.
pub const OBJECT_TS: &str = include_str!("object.ts");

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
