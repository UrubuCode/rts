//! Testes do motor de estilo — migrados intactos do `style.rs` monolítico na
//! divisão em submódulos (a API pública é a mesma via reexports do `mod.rs`).

use super::*;

mod at_rules_p;
mod cascade;
mod computed_corpus;
mod cores_e_caixa;
mod declaracao_espaco_antes_dois_pontos;
mod grid_placement_shorthands;
mod slots;
mod tabela_e_display;
mod valores;
