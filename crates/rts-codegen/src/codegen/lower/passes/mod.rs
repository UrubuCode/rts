//! Passes de transformacao do AST antes do codegen.
//!
//! Cada submodulo expoe um `pub(crate) fn <name>(program: &mut Program)`
//! aplicavel ao programa inteiro. Ordem de aplicacao eh definida em
//! `lower::func::compile_program` — alterar com cuidado, alguns passes
//! dependem de transformacoes anteriores.

pub mod args;
pub mod async_expand;
pub mod destructuring;
pub mod hoist_fn;
pub mod object_methods;
pub mod parallelism;
pub mod static_fields;
