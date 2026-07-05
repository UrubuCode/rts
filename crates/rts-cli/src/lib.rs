//! `rts` CLI library. After the P5 cutover the OLD engine (`rts-codegen-old`) is
//! DELETED; `run`/`eval`/`test` execute through the NEW engine
//! (`rts-codegen-new`). The AOT/IR/SPECS-export commands (`compile`/`ir`/`apis`/
//! `emit-types`) depended on the old engine's module-graph pipeline + ABI SPECS
//! tables and are stubbed pending their re-implementation on the new engine.

pub mod abi {
    pub use rts_engine::abi::*;
    pub mod member {
        pub use rts_engine::abi::member::*;
    }
    pub mod symbols {
        pub use rts_engine::abi::symbols::*;
    }
}
pub mod diagnostics {
    pub use rts_diagnostics::*;
}
pub mod linker {
    pub use rts_linker::*;
    pub use rts_linker::object_linker;
    pub use rts_linker::system_linker;
    pub use rts_linker::toolchain;
}
pub mod namespaces {
    pub use rts_runtime::namespaces::*;
    pub mod runtime {
        pub use rts_runtime::namespaces::runtime::*;
    }
    pub mod test {
        pub use rts_runtime::namespaces::test::*;
    }
}
pub mod compile_options;
pub mod manifest;
pub mod registers;
pub mod dotenv;
pub mod url_entry;
pub mod cli;

pub use compile_options::{CompilationProfile, CompileOptions, opt_level};
pub use rts_parser::FrontendMode;
