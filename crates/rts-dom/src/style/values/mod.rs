//! Tipos de VALOR do CSS (egui-free): cor, alinhamento, dimensões, lados de caixa.
//! São os tipos que os campos do `ComputedStyle` (ver `props.rs`) carregam. A
//! resolução de unidade relativa é TARDIA ([`Dimension::resolve`] no layout, nunca
//! no parse — north-star risco 5).

mod texto;
mod caixa;
mod display;
mod grelha;
mod fluxo;
mod dimensao;

pub use texto::*;
pub use caixa::*;
pub use display::*;
pub use grelha::*;
pub use fluxo::*;
pub use dimensao::*;

// `grelha.rs` diz `super::lengths::…`, como o ficheiro único dizia.
// Reimportar o nome aqui é o que o mantém a resolver sem tocar no corpo movido.
use super::lengths;
