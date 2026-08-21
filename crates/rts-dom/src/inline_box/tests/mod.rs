//! Os testes do `inline_box`, movidos sem alteração de conteúdo.
//!
//! Os `use` do topo vivem aqui porque os três submódulos os partilham — no
//! original estavam ao nível do `mod tests`.

mod caixa;
mod imagem;
mod quebra;

    use crate::table::tests::{geometria, rect, textos};
