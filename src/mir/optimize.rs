use super::MirModule;

#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizationReport {
    pub inlined_calls: usize,
    pub removed_noops: usize,
}

pub fn optimize(module: &mut MirModule) -> OptimizationReport {
    let mut removed_noops = 0usize;

    for function in &mut module.functions {
        for block in &mut function.blocks {
            let before = block.statements.len();
            block.statements.retain(|statement| statement.text != "noop");
            let after = block.statements.len();
            removed_noops += before.saturating_sub(after);
        }
    }

    OptimizationReport {
        inlined_calls: 0,
        removed_noops,
    }
}
