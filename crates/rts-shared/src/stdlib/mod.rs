//! Embedded TypeScript stdlib sources, compiled ahead of the user program by the
//! new engine as declarations-only preludes (engine `include`s). Their top-level
//! classes become ambient and shadow the corresponding native dispatch.

/// Faithful `Map<K,V>` + `Set<T>` (private array fields, `===` keys, shift+pop
/// delete, `get size()`). Registered as an include in the new engine; replaces
/// the deleted native Map/Set dispatch. Parity proven by `stdlib_parity.rs`.
pub const MAP_SET_TS: &str = include_str!("map_set.ts");

/// `JSON` (stringify/parse) — a rts-shared utility (not a primordial; no native
/// syntax). Pure TS over primordials only (typeof / Object.keys / Array.isArray /
/// recursion); the engine names nothing JSON-specific, it just runs the generics.
pub const JSON_TS: &str = include_str!("json.ts");

/// `WeakMap`/`WeakSet` — STRONG-reference collections for now (#217 tracks the
/// real weak path), backed by private arrays with `===` keys, like `MAP_SET_TS`.
/// Non-iterable surface only (no `size`/`keys`/`values`/`forEach`). Ambient `.ts`
/// classes so `new WeakMap()` is an ordinary user-class construction.
pub const WEAKMAP_SET_TS: &str = include_str!("weakmap_set.ts");

/// The global `console` object (`log`/`info`/`debug`/`dir` → stdout,
/// `warn`/`error` → stderr) — a rts-shared backend utility, NOT a primordial: it
/// has no native syntax and prints through the backend, so it lives here with the
/// other non-primordial `.ts` stdlib (moved out of `rts-primitives`). Written in
/// `.ts` so the front-end NAMES nothing about `console` — `console.log(...)` is an
/// ordinary member call on this ambient object. The two irreducible operations
/// (per-value display rendering + the capture-aware line print) are re-exposed
/// PRIVATELY via the `engine.display` / `engine.print_line` / `engine.eprint_line`
/// bridges the bodies call; the variadic space-join is plain `.ts`. Replaces the
/// former hardcoded `is_console_ident` + `lower_console_log` codegen path.
pub const CONSOLE_TS: &str = include_str!("console.ts");

/// `Reflect` (get/set/has) — a rts-shared utility (not a primordial; no native
/// syntax). Pure TS over primordials only (`target[key]` dynamic access +
/// `Object.keys`); `Reflect.get`/`set` on a Proxy fire its traps because the
/// dynamic property trampolines detect the proxy. Descriptor / prototype / apply
/// reflectors are a later increment (#218).
pub const REFLECT_TS: &str = include_str!("reflect.ts");
