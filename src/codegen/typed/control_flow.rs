use crate::mir::MirInstruction;

#[derive(Debug, Clone)]
struct LoopControlContext {
    end_label: String,
    continue_label: String,
}

fn loop_context_from_start_label(label: &str) -> Option<LoopControlContext> {
    if let Some(id) = label.strip_prefix("while_loop_") {
        return Some(LoopControlContext {
            end_label: format!("while_end_{}", id),
            continue_label: format!("while_loop_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("do_while_body_") {
        return Some(LoopControlContext {
            end_label: format!("do_while_end_{}", id),
            continue_label: format!("do_while_condition_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("for_loop_") {
        return Some(LoopControlContext {
            end_label: format!("for_end_{}", id),
            continue_label: format!("for_update_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("switch_body_") {
        let end = format!("switch_end_{}", id);
        return Some(LoopControlContext {
            end_label: end.clone(),
            continue_label: end,
        });
    }
    None
}

pub(super) fn rewrite_loop_control(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut rewritten = Vec::with_capacity(instructions.len());
    let mut loop_stack: Vec<LoopControlContext> = Vec::new();

    for instruction in instructions {
        match instruction {
            MirInstruction::Label(name) => {
                if let Some(ctx) = loop_context_from_start_label(name) {
                    loop_stack.push(ctx);
                }
                rewritten.push(instruction.clone());
                if let Some(top) = loop_stack.last() {
                    if &top.end_label == name {
                        loop_stack.pop();
                    }
                }
            }
            MirInstruction::Break => {
                if let Some(top) = loop_stack.last() {
                    rewritten.push(MirInstruction::Jump(top.end_label.clone()));
                } else {
                    rewritten.push(instruction.clone());
                }
            }
            MirInstruction::Continue => {
                if let Some(top) = loop_stack.last() {
                    rewritten.push(MirInstruction::Jump(top.continue_label.clone()));
                } else {
                    rewritten.push(instruction.clone());
                }
            }
            _ => rewritten.push(instruction.clone()),
        }
    }

    rewritten
}
