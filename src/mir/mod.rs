pub mod build;
pub mod cfg;
pub mod monomorphize;
pub mod optimize;

#[derive(Debug, Clone, Default)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone, Default)]
pub struct MirFunction {
    pub name: String,
    pub blocks: Vec<cfg::BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct MirStatement {
    pub text: String,
}
