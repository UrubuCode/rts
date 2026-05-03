//! Globals lowering helpers — Array, Object, Function, etc.
//!
//! Este módulo contém helpers para baixar chamadas de métodos globais do JavaScript
//! para instruções Cranelift. A lógica específica de cada global foi movida para
//! cá, deixando a codegen principal mais limpa e genérica.

pub mod array;
pub mod function;
pub mod object;

// Re-exporta as funções principais para uso na codegen
pub use array::lower_array_builtin;
pub use function::{emit_user_fn_addr, lower_function_handle_method, lower_function_method_call};
pub use object::lower_object_static_call;
