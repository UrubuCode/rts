//! `rts:gpu` — compute WGSL sobre o mesmo device do render.
//!
//! # O que esta casca traduz
//!
//! Só uma coisa: **de onde vêm e para onde vão os bytes**. O `crate::compute` do
//! `rts-egui` compila kernel, aloca buffer de GPU, despacha e lê — e recebe e
//! devolve `&[u8]`/`Vec<u8>`, porque quem sabe o que é um buffer do PROGRAMA é o
//! motor, e há dois.
//!
//! No motor antigo os bytes moram num `Entry::Buffer` do `HandleTable` e a
//! travessia é por handle. Aqui moram numa view tipada, e a travessia é
//! `bytes_of` na ida e `write_bytes` na volta — as mesmas funções que o
//! `meshUpload` já usa. Nada disso é novo: é `crate::value::bytes` e o
//! `reuse-check` que o `lib.rs` registra.
//!
//! # Três coisas mudaram, e são as mesmas do resto do porte
//!
//! - **`writeAt` recebe um objeto.** Cinco parâmetros não cabem em quatro slots.
//! - **`read` responde os BYTES**, não escreve num destino que o chamador
//!   passou. Um `Uint8Array` de volta é o que uma linguagem com views tipadas
//!   sabe dizer; a superfície antiga precisava do destino porque não tinha como
//!   devolver um buffer.
//! - **`readPoll` responde `null` / `false` / os bytes**, em vez de `0` / `-1` /
//!   uma contagem. Aquele inteiro com três significados fazia um chamador que
//!   testasse `if (n)` tratar a FALHA como sucesso — e `-1` é verdadeiro.

use rts_core::entry::{self, Provided};
use rts_egui::compute::{self, Poll};

use crate::value::{self, bytes, handle, integer, text};
use crate::window::options;

/// Os membros de `rts:gpu`.
pub const MEMBERS: &[(&str, Provided)] = &[
    ("available", available),
    ("shader", shader),
    ("buffer", buffer),
    ("write", write),
    ("writeAt", write_at),
    ("bindBuffer", bind_buffer),
    ("dispatch", dispatch),
    ("read", read),
    ("readBegin", read_begin),
    ("readPoll", read_poll),
    ("bufferFree", buffer_free),
    ("adapterName", adapter_name),
];

/// `available()` — há GPU utilizável. Cria o device na primeira consulta.
extern "C" fn available(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    value::from_bool(compute::available())
}

/// `shader(wgsl)` — compila um kernel (entry point `main`). O id do pipeline, ou
/// `0` com o erro de validação no stderr.
extern "C" fn shader(_e: u64, _t: u64, source: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    value::from_number(compute::shader(&text(source)) as f64)
}

/// `buffer(bytes)` — um storage buffer na GPU. O id, ou `0`.
extern "C" fn buffer(_e: u64, _t: u64, size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    value::from_number(compute::buffer(integer(size, 0)) as f64)
}

/// `write(gbuf, data)` — sobe uma view tipada inteira para o buffer de GPU.
extern "C" fn write(_e: u64, _t: u64, gbuf: u64, data: u64, _c: u64, _d: u64) -> u64 {
    let Some(data) = bytes(data) else {
        return value::from_bool(false);
    };
    value::from_bool(compute::write(handle(gbuf), &data))
}

/// `writeAt(gbuf, { data, srcOff, dstOff, bytes })` — escrita parcial.
///
/// É o que permite cutucar UM corpo sem reenviar o estado inteiro. `srcOff` e
/// `bytes` recortam a view antes de subir; ausentes, sobe a view toda.
extern "C" fn write_at(_e: u64, _t: u64, gbuf: u64, spec: u64, _c: u64, _d: u64) -> u64 {
    let Some(source) = bytes(value::fields(spec, &["data"])[0]) else {
        return value::from_bool(false);
    };
    let read = options(spec, &["srcOff", "dstOff", "bytes"], &[0.0, 0.0, -1.0]);
    let from = (read[0].max(0.0) as usize).min(source.len());
    let count = match read[2] {
        n if n < 0.0 => source.len() - from,
        n => (n as usize).min(source.len() - from),
    };
    value::from_bool(compute::write_at(
        handle(gbuf),
        read[1] as i64,
        &source[from..from + count],
    ))
}

/// `bindBuffer(pipe, slot, gbuf)` — liga o buffer ao `@binding(slot)` do
/// `@group(0)`.
///
/// `bindBuffer` e não `bind` porque o lowering trata `.bind(` como
/// `Function.prototype.bind` antes de resolver membro de namespace — a mesma
/// razão que o nome tinha na superfície antiga, e ela continua valendo.
extern "C" fn bind_buffer(_e: u64, _t: u64, pipe: u64, slot: u64, gbuf: u64, _d: u64) -> u64 {
    value::from_bool(compute::bind_buffer(
        handle(pipe),
        integer(slot, 0),
        handle(gbuf),
    ))
}

/// `dispatch(pipe, gx, gy, gz)` — submete os workgroups. NÃO espera a GPU:
/// `read` é quem sincroniza.
extern "C" fn dispatch(_e: u64, _t: u64, pipe: u64, gx: u64, gy: u64, gz: u64) -> u64 {
    value::from_bool(compute::dispatch(
        handle(pipe),
        integer(gx, 1),
        integer(gy, 1),
        integer(gz, 1),
    ))
}

/// `read(gbuf, bytes)` — espera a GPU e responde um `Uint8Array` com o que leu,
/// ou `undefined` em erro.
extern "C" fn read(_e: u64, _t: u64, gbuf: u64, size: u64, _c: u64, _d: u64) -> u64 {
    let Some(data) = compute::read(handle(gbuf), integer(size, 0)) else {
        return value::nothing();
    };
    entry::with_runtime(|context| entry::make_bytes(context, &data))
}

/// `readBegin(gbuf, bytes)` — agenda a cópia GPU→staging SEM esperar, e
/// responde um ticket (`0` em erro).
///
/// A física-como-serviço nasce aqui: o jogo agenda no fim do frame e pergunta
/// nos seguintes.
extern "C" fn read_begin(_e: u64, _t: u64, gbuf: u64, size: u64, _c: u64, _d: u64) -> u64 {
    value::from_number(compute::read_begin(handle(gbuf), integer(size, 0)) as f64)
}

/// `readPoll(ticket)` — pergunta sem bloquear.
///
/// `null` = ainda em voo, pergunte de novo. `false` = falhou ou o ticket não
/// existe, e ele foi consumido. Um `Uint8Array` = pronto, e ele foi consumido.
///
/// Três valores distinguíveis em vez do inteiro de três significados da
/// superfície antiga, onde `-1` (falha) passava num `if`.
extern "C" fn read_poll(_e: u64, _t: u64, ticket: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match compute::read_poll(handle(ticket)) {
        Poll::Pending => entry::null_value(),
        Poll::Failed => value::from_bool(false),
        Poll::Done(data) => entry::with_runtime(|context| entry::make_bytes(context, &data)),
    }
}

/// `bufferFree(gbuf)` — libera o buffer e o desliga de todo pipeline, para que
/// um dispatch posterior falhe a validação em vez de usar memória morta.
extern "C" fn buffer_free(_e: u64, _t: u64, gbuf: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    value::from_bool(compute::buffer_free(handle(gbuf)))
}

/// `adapterName()` — o nome do adapter em uso, para depuração. `undefined` sem
/// GPU.
extern "C" fn adapter_name(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match compute::adapter_name() {
        Some(name) => value::from_text(&name),
        None => value::nothing(),
    }
}
