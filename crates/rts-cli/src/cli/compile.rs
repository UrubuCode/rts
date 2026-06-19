//! `rts compile` — AOT native build. STUBBED at the P5 cutover: the old engine's
//! module-graph + object pipeline was deleted; AOT on the new engine is pending.

use anyhow::{Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::linker::WindowsSubsystem;

pub fn command(
    _input: Option<String>,
    _output: Option<String>,
    _options: CompileOptions,
    _windows_subsystem: Option<WindowsSubsystem>,
) -> Result<()> {
    Err(anyhow!(
        "`rts compile` (AOT) is not yet available on the new engine (cutover in progress); use `rts run`"
    ))
}
