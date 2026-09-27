//! `rts:audio` para o motor novo: a saída de som, o kernel de mixagem e o
//! decodificador OGG/Vorbis.
//!
//! O que é POLÍTICA — vozes, grupos, espacialização, WAV, a cache de clipes —
//! vive no programa, em TypeScript. Este crate tem só o que um script não faz:
//! falar com o dispositivo, e somar amostras num custo que o TS não alcança
//! (20 ns por acesso contra ~1 ns aqui).

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod mix;
pub mod ogg;
pub mod ring;
