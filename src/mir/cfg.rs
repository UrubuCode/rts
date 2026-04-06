#[derive(Debug, Clone, Default)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub label: String,
    pub statements: Vec<crate::mir::MirStatement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, Default)]
pub enum Terminator {
    #[default]
    Return,
    Goto(String),
}

impl ControlFlowGraph {
    pub fn linear(statements: Vec<crate::mir::MirStatement>) -> Self {
        Self {
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements,
                terminator: Terminator::Return,
            }],
        }
    }
}
