//! Testes do motor de estilo — migrados intactos do `style.rs` monolítico na
//! divisão em submódulos (a API pública é a mesma via reexports do `mod.rs`).

use super::*;

mod valores;
mod cores_e_caixa;
mod slots;
mod cascade;
mod tabela_e_display;
