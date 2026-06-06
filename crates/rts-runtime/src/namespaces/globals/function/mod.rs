//! Function — GlobalClassSpec para o tipo primitivo JS Function (#359).
//!
//! Cobre `.call`, `.apply`, `.bind`, `.name`, `.length` e o constructor
//! `new Function("a", "b", "return a+b")` via runtime.eval.
//!
//! Limitacoes vs Node:
//! - `.toString()` retorna `"function <name>() { [native code] }"` (RTS nao
//!   preserva source de fns declaradas estaticamente, exceto as criadas
//!   via `new Function`).
//! - `.prototype` nao existe (RTS separa classes de functions).
//! - `arguments` object nao existe (use rest params).
//! - `this` em fn declarations nao-arrow chamadas via `.call(thisArg)`:
//!   thisArg eh ignorado se a fn original nao for method de classe (RTS
//!   fns nao tem slot reservado pra this implicito).

pub mod abi;
pub mod ops;
pub mod props;
