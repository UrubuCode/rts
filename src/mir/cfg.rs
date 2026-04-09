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
    /// Conditional branch: if condition then true_block else false_block
    Branch {
        condition: super::VReg,
        true_block: String,
        false_block: String,
    },
    /// Switch statement with multiple cases and default
    Switch {
        value: super::VReg,
        cases: Vec<(i64, String)>, // (value, block_label)
        default_block: String,
    },
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
