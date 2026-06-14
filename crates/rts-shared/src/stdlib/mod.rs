//! Embedded TypeScript stdlib sources, compiled ahead of the user program by the
//! new engine as declarations-only preludes (engine `include`s). Their top-level
//! classes become ambient and shadow the corresponding native dispatch.

/// Faithful `Map<K,V>` + `Set<T>` (private array fields, `===` keys, shift+pop
/// delete, `get size()`). Registered as an include in the new engine; replaces
/// the deleted native Map/Set dispatch. Parity proven by `stdlib_parity.rs`.
pub const MAP_SET_TS: &str = include_str!("map_set.ts");
