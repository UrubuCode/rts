//! `http_server` namespace — servidor HTTP/1.1 nativo via actix-web.
//!
//! Ponte sync TS → async actix: `serve(addr, handler)` bloqueia, sobe
//! runtime tokio interno, dispatcha cada request num `spawn_blocking`
//! que invoca o `handler_fn` extern "C" do TS. O handler chama
//! `respond(req, status, ct, body)` que entrega o response via oneshot.
//!
//! Suporta keep-alive, pipelining HTTP/1.1, parsing de headers, body
//! arbitrario — tudo da pilha actix. Workers = `available_parallelism()`.

pub mod abi;
pub mod ops;
