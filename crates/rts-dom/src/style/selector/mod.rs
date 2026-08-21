//! SELETORES CSS: simples, compostos (`p.card#x`), combinadores (`div > p`,
//! `+`, `~`), atributo (`[a=v]` e operadores) e pseudo-classes — estruturais
//! (`:first-child`, `:nth-of-type`), de estado (`:hover`, `:focus`, `:link`) e
//! funcionais (`:not()`, `:is()`, `:where()`, `:lang()`).
//! O matching que precisa da ÁRVORE (combinadores/pseudo por posição) vive no
//! `Dom` (`matches_complex`); aqui fica o parse + o match puro de um compound.

mod tipos;
mod sintaxe;
mod casamento;

pub use tipos::*;
use sintaxe::*;
pub use casamento::*;
