use anyhow::Result;

use crate::compile_options::CompileOptions;
use crate::module_system::ModuleGraph;

pub use super::bootstrap::RunReport;

pub fn run_entry(graph: &ModuleGraph, options: CompileOptions) -> Result<RunReport> {
    let program = super::bootstrap::compile_graph(graph, options)?;
    Ok(super::bootstrap::execute(&program))
}

pub fn run_embedded_program(payload: &[u8]) -> Result<RunReport> {
    let program = super::bootstrap::BootstrapProgram::decode(payload)?;
    Ok(super::bootstrap::execute(&program))
}
