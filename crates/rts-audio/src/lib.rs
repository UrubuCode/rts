//! `rts:audio` para o motor novo: a saída de som, o kernel de mixagem e o
//! decodificador OGG/Vorbis.
//!
//! O que é POLÍTICA — vozes, grupos, espacialização, WAV, a cache de clipes —
//! vive no programa, em TypeScript. Este crate tem só o que um script não faz:
//! falar com o dispositivo, e somar amostras num custo que o TS não alcança
//! (20 ns por acesso contra ~1 ns aqui).

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod device;
pub mod mix;
pub mod ogg;
pub mod ring;
pub mod surface;

use rts_core::entry::{self, Context};

/// Registra `rts:audio`. Pelo host, não por um construtor daqui (a mesma razão
/// de `rts_physics::install`).
pub fn install(context: &mut Context) {
    let ns = surface::namespace(context);
    entry::declare_module(context, "rts:audio", ns);
}

/// Fecha as saídas enquanto o processo está inteiro (ver
/// `surface::fechar_todas`). Idempotente; no-op sem saída aberta.
pub fn shutdown() {
    surface::fechar_todas();
}
