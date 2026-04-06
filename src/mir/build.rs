use crate::hir::nodes::HirModule;

use super::cfg::ControlFlowGraph;
use super::{MirFunction, MirModule, MirStatement};

pub fn build(hir: &HirModule) -> MirModule {
    let mut mir = MirModule::default();

    for function in &hir.functions {
        let cfg = ControlFlowGraph::linear(vec![
            MirStatement {
                text: format!("enter {}", function.name),
            },
            MirStatement {
                text: "ret".to_string(),
            },
        ]);

        mir.functions.push(MirFunction {
            name: function.name.clone(),
            blocks: cfg.blocks,
        });
    }

    if mir.functions.is_empty() {
        let cfg = ControlFlowGraph::linear(vec![MirStatement {
            text: "ret".to_string(),
        }]);

        mir.functions.push(MirFunction {
            name: "main".to_string(),
            blocks: cfg.blocks,
        });
    }

    mir
}
