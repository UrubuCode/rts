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
/// The CONTAINING BLOCK as an entity — two extents, each DEFINITE or
/// INDEFINITE, and a percentage that says which axis it is on. It sits beside
/// `dimensao` rather than inside it because the split is the same one
/// `lengths`/`parse` already draws: whoever adds a UNIT touches `dimensao`,
/// whoever changes what a percentage is resolved AGAINST touches this.
mod containing_block;

pub use texto::*;
pub use caixa::*;
pub use display::*;
pub use grelha::*;
pub use fluxo::*;
pub use dimensao::*;
pub use containing_block::*;

// `grelha.rs` diz `super::lengths::…`, como o ficheiro único dizia.
// Reimportar o nome aqui é o que o mantém a resolver sem tocar no corpo movido.
use super::lengths;
