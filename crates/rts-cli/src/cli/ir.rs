//! `rts ir` — Cranelift IR dump. STUBBED at the P5 cutover (old pipeline deleted).

use anyhow::{Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(_input: Option<String>, _options: CompileOptions) -> Result<()> {
    Err(anyhow!(
        "`rts ir` is not yet available on the new engine (cutover in progress)"
    ))
}
