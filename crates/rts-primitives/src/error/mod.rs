//! Error class family (Error/TypeError/RangeError/ReferenceError/SyntaxError/
//! URIError/EvalError/AggregateError).
//!
//! Os externs `__RTS_FN_GL_*ERROR*` vivem em `instance.rs`/`rt.rs` e são
//! chamados diretamente pelo motor novo. Os 8 builders `register_*_class_spec`
//! (que montavam `Member` tables hand-written pro Registry) foram removidos em
//! DRAIN_MOTOR — nunca eram chamados por nenhum path de REGISTER/populate;
//! dispatch real de `new Error(...)`/`.message`/`.name`/`.toString`/etc é
//! hardcoded no front-end (`crates/rts-codegen-new/src/front/run/`).

pub mod instance;
