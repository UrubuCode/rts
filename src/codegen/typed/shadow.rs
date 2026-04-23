use crate::mir::MirInstruction;

#[derive(Debug, Default)]
pub(super) struct ShadowGlobalPlan {
    pub(super) names: Vec<String>,
}

pub(super) fn analyze_shadow_globals(
    instructions: &[MirInstruction],
    function_name: &str,
) -> ShadowGlobalPlan {
    if function_name == "main" {
        return ShadowGlobalPlan::default();
    }

    let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut referenced: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut has_call = false;

    for instruction in instructions {
        match instruction {
            MirInstruction::Bind(name, _, _) => {
                locals.insert(name.clone());
            }
            MirInstruction::LoadBinding(_, name) | MirInstruction::WriteBind(name, _) => {
                referenced.insert(name.clone());
            }
            MirInstruction::Call(_, _, _) => {
                has_call = true;
            }
            _ => {}
        }
    }

    if has_call {
        return ShadowGlobalPlan::default();
    }

    let names: Vec<String> = referenced
        .into_iter()
        .filter(|name| !locals.contains(name))
        .collect();

    ShadowGlobalPlan { names }
}
