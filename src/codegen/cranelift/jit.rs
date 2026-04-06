use crate::mir::MirModule;

#[derive(Debug, Clone)]
pub struct JitReport {
    pub entry_function: String,
    pub compiled_functions: usize,
}

pub fn execute(module: &MirModule, entry_function: &str) -> JitReport {
    // Bootstrap: this is where Cranelift JIT execution is wired in upcoming steps.
    JitReport {
        entry_function: entry_function.to_string(),
        compiled_functions: module.functions.len(),
    }
}
