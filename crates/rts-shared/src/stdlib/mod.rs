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
pub const JSON5_TS: &str = include_str!("json5.ts");
pub const ITERATOR_TS: &str = include_str!("iterator.ts");

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

/// `structuredClone(value)` — a rts-shared utility GLOBAL FUNCTION (not a
/// primordial; no native syntax). Pure TS over primordials + ambient stdlib
/// (typeof / Array.isArray / Object.keys / instanceof / WeakMap / Map / Set /
/// Date); a deep structural clone with cycle detection. The engine names nothing
/// here — `structuredClone(x)` is an ordinary call of this ambient function.
pub const STRUCTURED_CLONE_TS: &str = include_str!("structured_clone.ts");

/// The global `performance` object (`now()` monotonic ms + `timeOrigin` epoch) —
/// a rts-shared backend utility (no native syntax). A `.ts` singleton like
/// `console`, reading the PRIVATE `engine.now_ms`/`unix_ms` clock bridges.
pub const PERFORMANCE_TS: &str = include_str!("performance.ts");

/// Global timer functions (`setTimeout`/`clearTimeout`/`setInterval`/
/// `clearInterval`/`queueMicrotask`) — rts-shared backend utilities (no native
/// syntax). Pure TS over the private `engine.set_timeout`/… bridges; the fn
/// value rides into the runtime's ordered macro/microtask queues.
pub const TIMERS_TS: &str = include_str!("timers.ts");

/// Web-platform value classes — `FormData`/`Blob`/`File`/`Request`/`Response`
/// — rts-shared backend utilities (no native syntax). Pure `.ts` value holders
/// (insertion-ordered parallel arrays; UTF-8 measured/decoded in TS). `Headers`
/// is NOT here (DRAIN_MOTOR §11): it is a Rust `#[rtse::class]` Registry class
/// (`rts-std/src/globals/headers`), resolved the same way `URL` is — an
/// ordinary ambient global, so `new Headers(...)` in `Request`/`Response`
/// below is unaffected.
pub const WEBAPI_TS: &str = include_str!("webapi.ts");

/// Web event-model classes — `Event`/`EventTarget`/`AbortSignal`/
/// `AbortController`/`MessageChannel`/`MessagePort` — rts-shared backend
/// utilities (no native syntax). Pure `.ts` state machines; `AbortSignal.timeout`
/// rides the real setTimeout queue; MessagePort delivers via queueMicrotask.
pub const EVENTS_TS: &str = include_str!("events.ts");

// Web Streams (`ReadableStream`/`WritableStream`/`TransformStream`/
// `TextEncoderStream`/`TextDecoderStream`) moved to `#[rtse::class]` Rust
// (DRAIN_MOTOR §11, `crates/rts-std/src/globals/streams/`) — `STREAMS_TS`
// removed, `streams.ts` deleted.
