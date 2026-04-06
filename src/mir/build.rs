use crate::hir::nodes::HirModule;
use crate::hir::nodes::{HirFunction, HirImport, HirItem};

use super::cfg::ControlFlowGraph;
use super::{MirFunction, MirModule, MirStatement};

pub fn build(hir: &HirModule) -> MirModule {
    let mut mir = MirModule::default();
    let mut top_level = collect_top_level_statements(hir);

    for function in &hir.functions {
        mir.functions.push(build_stub_function(function));
    }

    if !top_level.is_empty() {
        if let Some(main) = mir
            .functions
            .iter_mut()
            .find(|function| function.name == "main")
        {
            inject_statements_into_function(main, top_level);
            top_level = Vec::new();
        }
    }

    if !top_level.is_empty() {
        top_level.push(MirStatement {
            text: "ret".to_string(),
        });

        mir.functions.push(MirFunction {
            name: "main".to_string(),
            blocks: ControlFlowGraph::linear(top_level).blocks,
        });
    }

    if mir.functions.is_empty() {
        mir.functions.push(MirFunction {
            name: "main".to_string(),
            blocks: ControlFlowGraph::linear(vec![MirStatement {
                text: "ret".to_string(),
            }])
            .blocks,
        });
    }

    mir
}

fn build_stub_function(function: &HirFunction) -> MirFunction {
    MirFunction {
        name: function.name.clone(),
        blocks: ControlFlowGraph::linear(vec![
            MirStatement {
                text: format!("enter {}", function.name),
            },
            MirStatement {
                text: "ret".to_string(),
            },
        ])
        .blocks,
    }
}

fn collect_top_level_statements(hir: &HirModule) -> Vec<MirStatement> {
    let mut statements = Vec::new();

    for item in &hir.items {
        match item {
            HirItem::Import(import) => statements.push(MirStatement {
                text: render_import_statement(import),
            }),
            HirItem::Statement(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    statements.push(MirStatement {
                        text: trimmed.to_string(),
                    });
                }
            }
            HirItem::Function(_) | HirItem::Interface(_) | HirItem::Class(_) => {}
        }
    }

    statements
}

fn render_import_statement(import: &HirImport) -> String {
    let joined = import.names.join(", ");
    format!("import {{{joined}}} from \"{}\";", import.from)
}

fn inject_statements_into_function(function: &mut MirFunction, mut statements: Vec<MirStatement>) {
    if let Some(block) = function.blocks.first_mut() {
        if matches!(
            block
                .statements
                .last()
                .map(|statement| statement.text.trim()),
            Some("ret")
        ) {
            let ret = block.statements.pop();
            block.statements.append(&mut statements);
            if let Some(ret) = ret {
                block.statements.push(ret);
            }
            return;
        }

        block.statements.append(&mut statements);
        return;
    }

    function.blocks = ControlFlowGraph::linear(statements).blocks;
}
