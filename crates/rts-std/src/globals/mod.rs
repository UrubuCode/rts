//! Globais JS platform-divergent que vivem na camada BACKEND (`rts-std`): tocam
//! I/O (console→io), tempo/timers, rede (fetch), perf, ou modelam recursos de
//! plataforma (headers/eventos). NÃO universais (não
//! rodam em wasm sem backend). Cada um expõe `register*`/`register_*_class_spec`,
//! chamado pelo registro em `rts-codegen/src/abi/mod.rs` via o facade
//! `rts-runtime::namespaces::globals`.
//!
//! Externs do collector do runtime (`__RTS_FN_RT_ERROR_SET`) e da função em
//! rts-shared (`__RTS_FN_RT_INVOKE_AUTO`) são resolvidos por link (`extern "C"`).
//!
//! DRAIN_MOTOR (2026-07-24): `blob`/`form_data`/`message_channel`/
//! `readable_stream` removidos numa sessão anterior — cada `register_*_class_spec`
//! nunca era chamado (não wired), e nenhum outro item do módulo tinha caller real;
//! a superfície viva era o `.ts` interino. `blob` (Blob+File) e `form_data`
//! REIMPLEMENTADOS como `#[rtse::class]` (DRAIN_MOTOR §11), em paridade com o
//! `.ts` rico (`crates/rts-shared/src/stdlib/webapi.ts`). `headers` JÁ
//! reimplementado. `event_target`/`abort` REIMPLEMENTADOS (§11 correção):
//! `EventTarget`/`Event` + `AbortSignal extends EventTarget` +
//! `AbortController` agora em paridade com o `.ts` rico (options `{once}`,
//! ordem de dispatch, `once` removido ANTES do callback, `!defaultPrevented`) —
//! o macro ganhou `extends` (composição+forwarding) que destravou isso.
//! `events.ts` teve as 4 classes removidas (MessageChannel/MessagePort ficam).
//! `message_channel` segue apenas `.ts` (fora de escopo desta sessão).
//! `streams` (Web Streams, 9 classes) REIMPLEMENTADO (§11): substitui o
//! `.ts` interino (`STREAMS_TS`) — ver `streams/mod.rs`.

pub mod abort;
pub mod blob;
pub mod console;
pub mod event_target;
pub mod form_data;
pub mod performance;
pub mod events;
pub mod headers;
pub mod streams;
pub mod text_encoding;
pub mod timers;
pub mod imgdec;
pub mod fetch;
