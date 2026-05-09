//! Reflect global — wrappers que aceitam handles de key (em vez de ptr+len)
//! e produzem descriptors no formato JS spec.
//!
//! As demais operacoes Reflect (get/set/has/deleteProperty/ownKeys/
//! getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions) reusam
//! diretamente as fns de `collections.map_*` ou `Object.*` — codegen
//! despacha em `lower_call`, sem precisar de fn dedicada aqui.
//!
//! Issue #218.

pub mod ops;
