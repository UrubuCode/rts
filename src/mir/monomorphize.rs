use super::MirModule;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonomorphizationReport {
    pub specialized_instances: usize,
}

pub fn monomorphize(_module: &mut MirModule) -> MonomorphizationReport {
    MonomorphizationReport {
        specialized_instances: 0,
    }
}
