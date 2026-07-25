//! Function — classe global do tipo primitivo JS Function (#359).
//!
//! Os externs `__RTS_FN_GL_FUNCTION_*` vivem em `ops.rs`/`props.rs` e são
//! chamados diretamente pelo motor novo (`rts-adapters`/`rts-codegen-new`).
//! O builder `register_function_class_spec` (que montava um `Member` table
//! hand-written pro Registry) foi removido em DRAIN_MOTOR — nunca era
//! chamado por nenhum path de REGISTER/populate; dispatch real de `.call`/
//! `.apply`/`.bind`/etc é hardcoded no front-end.
//!
//! Cobre `.call`, `.apply`, `.bind`, `.name`, `.length`, `.prototype` e o
//! constructor `new Function("a", "b", "return a+b")` via runtime.eval.
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

pub mod ops;
pub mod props;
