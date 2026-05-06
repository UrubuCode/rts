pub mod abi;

pub mod diagnostics {
    pub use rts_diagnostics::*;
}

pub mod parser {
    pub use rts_parser::*;
    pub use rts_ast::ast;
    pub use rts_ast::span;
}

pub mod namespaces {
    pub use rts_runtime::namespaces::*;
    pub mod runtime {
        pub use rts_runtime::namespaces::runtime::*;
        pub mod eval_jit {
            pub use crate::eval_jit::*;
        }
    }
}

pub mod nodespace;
pub mod runtime;
pub mod bundle;
pub mod codegen;
pub mod compile_options;
pub mod eval_jit;
pub mod function_eval_compile;
pub mod module;
pub mod type_system;

pub use codegen::*;
pub use compile_options::{CompileOptions, CompilationProfile, opt_level};
pub use rts_parser::FrontendMode;
