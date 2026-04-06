use crate::mir::MirModule;

#[derive(Debug, Clone, Default)]
pub struct ClifModule {
    pub functions: Vec<String>,
}

impl ClifModule {
    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("; RTS CLIF (bootstrap)\n");

        for function in &self.functions {
            output.push_str(function);
            output.push('\n');
        }

        output
    }
}

pub fn lower_to_clif(mir: &MirModule) -> ClifModule {
    let functions = mir
        .functions
        .iter()
        .map(|function| {
            let mut rendered = format!("function %{}() {{", function.name);

            for block in &function.blocks {
                rendered.push_str(&format!("\n  block {}:", block.label));
                for statement in &block.statements {
                    rendered.push_str(&format!("\n    ; {}", statement.text));
                }
            }

            rendered.push_str("\n}");
            rendered
        })
        .collect();

    ClifModule { functions }
}
