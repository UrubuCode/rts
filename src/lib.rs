pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod diagnostics;
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

pub fn rt_artifacts() -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts()
}
