//! Modulos de compilacao individual de fns / classes / main / MIR routing.
//!
//! `compile_program` (em `lower::func`) orquestra estes passos depois
//! que os passes AST terminam. Cada submodulo lida com uma fatia
//! distinta do pipeline final.

pub mod class;
