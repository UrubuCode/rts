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

/// Embedded TypeScript source of the PRIMORDIAL `Boolean.prototype` methods
/// (`toString`/`valueOf`). The new engine `include`s this declarations-only
/// prelude: a method called on a PRIMITIVE bool receiver (`true.toString()`) is
/// routed into this ambient `class Boolean`'s method with the primitive BOXED as
/// `this` (shape-based dispatch, NOT JS prototypes — the engine resolves the
/// method on the class at compile time). The method bodies read `this` AS THE
/// PRIMITIVE boolean. This file is the PROVER of the primitive → prelude-`.ts`-
/// class method-dispatch mechanism (String/Number follow the same pattern). The
/// wrapper object `new Boolean(x)` stays the engine's wrapper trampoline.
pub const BOOLEAN_TS: &str = include_str!("boolean.ts");

/// Embedded TypeScript source of the PRIMORDIAL `Number.prototype` methods
/// (`valueOf`/`toString`/`toFixed`/`toPrecision`/`toExponential`/`toLocaleString`).
/// `number` is a PRIMITIVE (native literal syntax), so the VALUE stays unboxed —
/// only the METHOD library moves here. A method called on a PRIMITIVE number
/// receiver (`(5).toFixed(2)`) is routed into this ambient `class Number`'s method
/// with the primitive BOXED as `this` (shape-based dispatch, NOT JS prototypes).
/// The irreducible numeric FORMATTING stays in Rust (`number.rs`,
/// `__RTS_FN_GL_NUMBER_*`) and is re-exposed PRIVATELY via the `engine.num_*`
/// helpers the bodies call — one source of truth. Same pattern as `BOOLEAN_TS`;
/// `String` follows next. The wrapper object `new Number(x)` stays the engine's
/// wrapper trampoline.
pub const NUMBER_TS: &str = include_str!("number.ts");

/// Embedded TypeScript source of the PRIMORDIAL `String.prototype` methods
/// (case/trim/charAt/charCodeAt/at/repeat/slice/substring/indexOf/lastIndexOf/
/// includes/startsWith/endsWith/padStart/padEnd/concat/replace/replaceAll).
/// `string` is a PRIMITIVE (native literal syntax `""`), so the VALUE stays a
/// `TAG_STR` PolyValue — only the METHOD library moves here. A method called on a
/// PRIMITIVE string receiver (`"abc".toUpperCase()`) is routed into this ambient
/// `class String`'s method with the primitive BOXED as `this` (shape-based
/// dispatch, NOT JS prototypes). The irreducible Unicode-aware string logic stays
/// in Rust (`string/`, `__RTS_FN_GL_STRING_*`) and is re-exposed PRIVATELY via the
/// `engine.str_*` helpers the bodies call — one source of truth. Same pattern as
/// `NUMBER_TS`. `.length` stays an engine direct read; `split` (array) + the
/// regex-first methods stay on the engine's dispatch paths. The wrapper object
/// `new String(x)` stays the engine's wrapper trampoline.
pub const STRING_TS: &str = include_str!("string.ts");

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
