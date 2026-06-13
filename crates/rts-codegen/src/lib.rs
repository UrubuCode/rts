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

/// N-API (.node native addons) — re-export via facade. Ver
/// docs/specs/napi-implementation.md.
pub mod napi {
    pub use rts_runtime::napi::*;
}

pub mod linker {
    pub use rts_linker::*;
    pub use rts_linker::object_linker;
    pub use rts_linker::system_linker;
    pub use rts_linker::toolchain;
}
/// Node.js builtin shims (`node:fs`, `node:path`, …) — movido pro crate
/// `rts-node`. Re-exportado aqui para os consumidores do codegen
/// (`crate::nodespace::node_lookup`/`ns_prefix_for`) seguirem intocados.
pub mod nodespace {
    pub use rts_node::*;
}
pub mod runtime;
pub mod bundle;
pub mod cache;
pub mod codegen;
pub mod mir_codegen;
pub mod compile_options;
pub mod eval_jit;
pub mod function_eval_compile;
pub mod module;
pub mod pipeline;
pub mod type_system;

pub use codegen::*;
pub use compile_options::{CompileOptions, CompilationProfile, opt_level};
pub use rts_parser::FrontendMode;

use std::path::PathBuf;
use std::sync::OnceLock;
use anyhow::Result;

pub type RuntimeArtifactsHook = fn() -> Result<PathBuf>;
static RUNTIME_ARTIFACTS_HOOK: OnceLock<RuntimeArtifactsHook> = OnceLock::new();

pub fn register_runtime_artifacts(f: RuntimeArtifactsHook) {
    let _ = RUNTIME_ARTIFACTS_HOOK.set(f);
}

pub(crate) fn runtime_artifacts() -> Result<PathBuf> {
    match RUNTIME_ARTIFACTS_HOOK.get() {
        Some(f) => f(),
        None => anyhow::bail!("runtime_artifacts hook not registered"),
    }
}
