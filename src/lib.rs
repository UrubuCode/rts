pub mod abi;
pub mod cache {
    pub use rts_codegen::cache::*;
}
pub mod function_eval_compile {
    pub use rts_codegen::function_eval_compile::*;
}
pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod codegen;
pub mod compile_options;
pub mod diagnostics;
pub mod dotenv {
    pub use rts_cli::dotenv::*;
}
pub mod linker;
pub mod module;
pub mod namespaces;
pub mod nodespace;
pub mod parser;
pub mod pipeline {
    pub use rts_codegen::pipeline::*;
}
pub mod registers {
    pub use rts_cli::registers::*;
}
pub mod runtime;
pub mod type_system;

pub(crate) mod runtime_objects;

pub fn rt_artifacts() -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts()
}
