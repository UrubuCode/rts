use crate::hir::nodes::{HirFunction, HirItem, HirModule};

use super::cfg::Terminator;
use super::{
    MirBinOp, MirInstruction, MirUnaryOp, TypedBasicBlock, TypedMirFunction, TypedMirModule, VReg,
};

use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

pub fn typed_build(hir: &HirModule) -> TypedMirModule {
    let mut module = TypedMirModule::default();
    let mut top_level_instructions: Vec<MirInstruction> = Vec::new();
    let mut top_level_vreg: u32 = 0;

    // Process items for top-level statements and imports
    for item in &hir.items {
        match item {
            HirItem::Import(import) => {
                top_level_instructions.push(MirInstruction::Import {
                    names: import.names.clone(),
                    from: import.from.clone(),
                });
            }
            HirItem::Statement(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    lower_statement_text(trimmed, &mut top_level_instructions, &mut top_level_vreg);
                }
            }
            HirItem::Function(_) | HirItem::Interface(_) | HirItem::Class(_) => {}
        }
    }

    // Build typed functions from hir.functions
    for function in &hir.functions {
        module.functions.push(build_typed_function(function));
    }

    // Inject top-level statements into main if it exists
    if !top_level_instructions.is_empty() {
        if let Some(main) = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
        {
            inject_into_typed_main(main, &mut top_level_instructions);
            top_level_instructions = Vec::new();
        }
    }

    // Create synthetic main if there are remaining top-level statements
    if !top_level_instructions.is_empty() {
        top_level_instructions.push(MirInstruction::Return(None));
        module.functions.push(TypedMirFunction {
            name: "main".to_string(),
            param_count: 0,
            blocks: vec![TypedBasicBlock {
                label: "entry".to_string(),
                instructions: top_level_instructions,
                terminator: Terminator::Return,
            }],
            next_vreg: top_level_vreg,
        });
    }

    // If no functions at all, create empty main
    if module.functions.is_empty() {
        module.functions.push(TypedMirFunction {
            name: "main".to_string(),
            param_count: 0,
            blocks: vec![TypedBasicBlock {
                label: "entry".to_string(),
                instructions: vec![MirInstruction::Return(None)],
                terminator: Terminator::Return,
            }],
            next_vreg: 0,
        });
    }

    module
}

fn build_typed_function(function: &HirFunction) -> TypedMirFunction {
    let mut func = TypedMirFunction {
        name: function.name.clone(),
        param_count: function.parameters.len(),
        blocks: Vec::new(),
        next_vreg: 0,
    };

    let mut instructions: Vec<MirInstruction> = Vec::new();

    // Emit LoadParam + Bind for each parameter
    for (index, param) in function.parameters.iter().enumerate() {
        let vreg = func.alloc_vreg();
        instructions.push(MirInstruction::LoadParam(vreg, index));
        instructions.push(MirInstruction::Bind(param.name.clone(), vreg, true));
    }

    // Lower each body statement
    for statement in &function.body {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            lower_statement_text(trimmed, &mut instructions, &mut func.next_vreg);
        }
    }

    // Ensure function ends with a return
    let has_return = instructions.iter().any(|i| matches!(i, MirInstruction::Return(_)));
    if !has_return {
        instructions.push(MirInstruction::Return(None));
    }

    func.blocks.push(TypedBasicBlock {
        label: "entry".to_string(),
        instructions,
        terminator: Terminator::Return,
    });

    func
}

fn inject_into_typed_main(
    main: &mut TypedMirFunction,
    statements: &mut Vec<MirInstruction>,
) {
    if let Some(block) = main.blocks.first_mut() {
        // Insert before the final Return if present
        let last_is_return = block
            .instructions
            .last()
            .map(|i| matches!(i, MirInstruction::Return(_)))
            .unwrap_or(false);

        if last_is_return {
            let ret = block.instructions.pop();
            block.instructions.append(statements);
            if let Some(ret) = ret {
                block.instructions.push(ret);
            }
        } else {
            block.instructions.append(statements);
        }
        return;
    }

    main.blocks.push(TypedBasicBlock {
        label: "entry".to_string(),
        instructions: std::mem::take(statements),
        terminator: Terminator::Return,
    });
}

fn try_parse_statement(text: &str) -> Option<Vec<Stmt>> {
    let cm: Lrc<SourceMap> = Default::default();
    let source = cm.new_source_file(FileName::Anon.into(), text.to_string());
    let mut parser = Parser::new(
        Syntax::Typescript(TsSyntax::default()),
        StringInput::from(&*source),
        None,
    );
    parser.parse_script().ok().map(|script| script.body)
}

fn lower_statement_text(
    text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let stmts = match try_parse_statement(text) {
        Some(s) if !s.is_empty() => s,
        _ => {
            // Parse failure: emit RuntimeEval
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, text.to_string()));
            return;
        }
    };

    for stmt in stmts {
        lower_stmt(&stmt, text, instructions, next_vreg);
    }
}

fn alloc(next_vreg: &mut u32) -> VReg {
    let v = VReg(*next_vreg);
    *next_vreg += 1;
    v
}

fn lower_stmt(
    stmt: &Stmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    match stmt {
        Stmt::Decl(Decl::Var(var_decl)) => {
            let mutable = var_decl.kind != VarDeclKind::Const;
            for decl in &var_decl.decls {
                let name = match &decl.name {
                    Pat::Ident(ident) => ident.id.sym.to_string(),
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        continue;
                    }
                };
                if let Some(init) = &decl.init {
                    let vreg = lower_expr(init, original_text, instructions, next_vreg);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                } else {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::ConstUndef(vreg));
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                }
            }
        }
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let vreg = lower_expr(arg, original_text, instructions, next_vreg);
                instructions.push(MirInstruction::Return(Some(vreg)));
            } else {
                instructions.push(MirInstruction::Return(None));
            }
        }
        Stmt::Expr(expr_stmt) => {
            let _vreg = lower_expr(&expr_stmt.expr, original_text, instructions, next_vreg);
        }
        Stmt::Block(block_stmt) => {
            for inner_stmt in &block_stmt.stmts {
                lower_stmt(inner_stmt, original_text, instructions, next_vreg);
            }
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
        }
    }
}

fn lower_expr(
    expr: &Expr,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) -> VReg {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(n) => {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::ConstNumber(vreg, n.value));
                vreg
            }
            Lit::Str(s) => {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::ConstString(vreg, s.value.to_string_lossy().into_owned()));
                vreg
            }
            Lit::Bool(b) => {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::ConstBool(vreg, b.value));
                vreg
            }
            Lit::Null(_) => {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::ConstNull(vreg));
                vreg
            }
            _ => {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                vreg
            }
        },
        Expr::Ident(ident) => {
            let name = ident.sym.to_string();
            if name == "undefined" {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::ConstUndef(vreg));
                vreg
            } else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::LoadBinding(vreg, name));
                vreg
            }
        }
        Expr::Bin(bin) => {
            let op = match map_bin_op(bin.op) {
                Some(op) => op,
                None => {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                    return vreg;
                }
            };
            let lhs = lower_expr(&bin.left, original_text, instructions, next_vreg);
            let rhs = lower_expr(&bin.right, original_text, instructions, next_vreg);
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::BinOp(vreg, op, lhs, rhs));
            vreg
        }
        Expr::Unary(unary) => {
            let op = match map_unary_op(unary.op) {
                Some(op) => op,
                None => {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                    return vreg;
                }
            };
            let arg = lower_expr(&unary.arg, original_text, instructions, next_vreg);
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::UnaryOp(vreg, op, arg));
            vreg
        }
        Expr::Call(call) => {
            let callee_name = extract_callee_name(&call.callee);
            let callee_str = match callee_name {
                Some(name) => name,
                None => {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                    return vreg;
                }
            };
            let mut arg_vregs = Vec::new();
            for arg in &call.args {
                let vreg = lower_expr(&arg.expr, original_text, instructions, next_vreg);
                arg_vregs.push(vreg);
            }
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::Call(vreg, callee_str, arg_vregs));
            vreg
        }
        Expr::Paren(paren) => lower_expr(&paren.expr, original_text, instructions, next_vreg),
        Expr::Assign(assign) => {
            if let Some(name) = extract_simple_assign_target(&assign.left) {
                match assign.op {
                    AssignOp::Assign => {
                        let vreg = lower_expr(&assign.right, original_text, instructions, next_vreg);
                        instructions.push(MirInstruction::WriteBind(name, vreg));
                        vreg
                    }
                    AssignOp::AddAssign | AssignOp::SubAssign | AssignOp::MulAssign
                    | AssignOp::DivAssign | AssignOp::ModAssign => {
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadBinding(load, name.clone()));
                        let rhs = lower_expr(&assign.right, original_text, instructions, next_vreg);
                        let op = match assign.op {
                            AssignOp::AddAssign => MirBinOp::Add,
                            AssignOp::SubAssign => MirBinOp::Sub,
                            AssignOp::MulAssign => MirBinOp::Mul,
                            AssignOp::DivAssign => MirBinOp::Div,
                            AssignOp::ModAssign => MirBinOp::Mod,
                            _ => unreachable!(),
                        };
                        let result = alloc(next_vreg);
                        instructions.push(MirInstruction::BinOp(result, op, load, rhs));
                        instructions.push(MirInstruction::WriteBind(name, result));
                        result
                    }
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        vreg
                    }
                }
            } else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                vreg
            }
        }
        Expr::Update(update) => {
            if let Expr::Ident(ident) = update.arg.as_ref() {
                let name = ident.sym.to_string();
                let load = alloc(next_vreg);
                instructions.push(MirInstruction::LoadBinding(load, name.clone()));
                let one = alloc(next_vreg);
                instructions.push(MirInstruction::ConstNumber(one, 1.0));
                let op = if update.op == UpdateOp::PlusPlus { MirBinOp::Add } else { MirBinOp::Sub };
                let result = alloc(next_vreg);
                instructions.push(MirInstruction::BinOp(result, op, load, one));
                instructions.push(MirInstruction::WriteBind(name, result));
                if update.prefix { result } else { load }
            } else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                vreg
            }
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
            vreg
        }
    }
}

fn extract_callee_name(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(expr) => extract_expr_name(expr),
        _ => None,
    }
}

fn extract_expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Member(member) => {
            let obj = extract_expr_name(&member.obj)?;
            let prop = match &member.prop {
                MemberProp::Ident(ident) => ident.sym.to_string(),
                _ => return None,
            };
            Some(format!("{}.{}", obj, prop))
        }
        _ => None,
    }
}

fn extract_simple_assign_target(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => match simple {
            SimpleAssignTarget::Ident(ident) => Some(ident.id.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn map_bin_op(op: BinaryOp) -> Option<MirBinOp> {
    match op {
        BinaryOp::Add => Some(MirBinOp::Add),
        BinaryOp::Sub => Some(MirBinOp::Sub),
        BinaryOp::Mul => Some(MirBinOp::Mul),
        BinaryOp::Div => Some(MirBinOp::Div),
        BinaryOp::Mod => Some(MirBinOp::Mod),
        BinaryOp::Gt => Some(MirBinOp::Gt),
        BinaryOp::GtEq => Some(MirBinOp::Gte),
        BinaryOp::Lt => Some(MirBinOp::Lt),
        BinaryOp::LtEq => Some(MirBinOp::Lte),
        BinaryOp::EqEqEq => Some(MirBinOp::Eq),
        BinaryOp::NotEqEq => Some(MirBinOp::Ne),
        BinaryOp::LogicalAnd => Some(MirBinOp::LogicAnd),
        BinaryOp::LogicalOr => Some(MirBinOp::LogicOr),
        _ => None,
    }
}

fn map_unary_op(op: UnaryOp) -> Option<MirUnaryOp> {
    match op {
        UnaryOp::Minus => Some(MirUnaryOp::Negate),
        UnaryOp::Plus => Some(MirUnaryOp::Positive),
        UnaryOp::Bang => Some(MirUnaryOp::Not),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::hir::nodes::{HirFunction, HirItem, HirModule, HirParameter};

    use super::*;

    fn build_simple_module(statements: Vec<&str>) -> HirModule {
        HirModule {
            items: statements
                .into_iter()
                .map(|s| HirItem::Statement(s.to_string()))
                .collect(),
            functions: Vec::new(),
            imports: Vec::new(),
            classes: Vec::new(),
            interfaces: Vec::new(),
        }
    }

    #[test]
    fn lowers_numeric_constant_declaration() {
        let hir = build_simple_module(vec!["const x = 42;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have ConstNumber + Bind (+ Return at end)
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 42.0)));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::Bind(name, _, false) if name == "x")));
    }

    #[test]
    fn lowers_string_constant() {
        let hir = build_simple_module(vec![r#"const msg = "hello";"#]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::ConstString(_, s) if s == "hello")));
    }

    #[test]
    fn lowers_binary_expression() {
        let hir = build_simple_module(vec!["const y = 1 + 2;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _))));
    }

    #[test]
    fn lowers_function_call() {
        let hir = build_simple_module(vec!["io.print(42);"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::Call(_, name, _) if name == "io.print")));
    }

    #[test]
    fn falls_back_to_runtime_eval_for_if_statement() {
        let hir = build_simple_module(vec!["if (true) { io.print(1); }"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::RuntimeEval(_, _))));
    }

    #[test]
    fn lowers_function_with_parameters() {
        let hir = HirModule {
            items: vec![HirItem::Function(HirFunction {
                name: "add".to_string(),
                parameters: vec![
                    HirParameter {
                        name: "a".to_string(),
                        type_annotation: None,
                        variadic: false,
                    },
                    HirParameter {
                        name: "b".to_string(),
                        type_annotation: None,
                        variadic: false,
                    },
                ],
                return_type: None,
                body: vec!["return a + b;".to_string()],
            })],
            functions: vec![HirFunction {
                name: "add".to_string(),
                parameters: vec![
                    HirParameter {
                        name: "a".to_string(),
                        type_annotation: None,
                        variadic: false,
                    },
                    HirParameter {
                        name: "b".to_string(),
                        type_annotation: None,
                        variadic: false,
                    },
                ],
                return_type: None,
                body: vec!["return a + b;".to_string()],
            }],
            imports: Vec::new(),
            classes: Vec::new(),
            interfaces: Vec::new(),
        };
        let mir = typed_build(&hir);
        let add_fn = mir
            .functions
            .iter()
            .find(|f| f.name == "add")
            .expect("add function");
        assert_eq!(add_fn.param_count, 2);
        let instructions = &add_fn.blocks[0].instructions;
        // Should have LoadParam + Bind for each parameter
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::LoadParam(_, 0))));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::LoadParam(_, 1))));
        // Should have Return
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::Return(Some(_)))));
    }

    #[test]
    fn lowers_simple_assignment() {
        let hir = build_simple_module(vec!["let x = 1;", "x = 2;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have WriteBind for the assignment
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x")));
        // The value 2 should be a ConstNumber
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 2.0)));
    }

    #[test]
    fn lowers_compound_assignment() {
        let hir = build_simple_module(vec!["let x = 10;", "x += 5;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have LoadBinding + BinOp(Add) + WriteBind
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::LoadBinding(_, name) if name == "x")));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _))));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x")));
    }

    #[test]
    fn lowers_postfix_increment() {
        let hir = build_simple_module(vec!["let i = 0;", "i++;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have LoadBinding + ConstNumber(1) + BinOp(Add) + WriteBind
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 1.0)));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _))));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "i")));
    }

    #[test]
    fn lowers_prefix_decrement() {
        let hir = build_simple_module(vec!["let i = 5;", "--i;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Sub, _, _))));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "i")));
    }

    #[test]
    fn lowers_mul_assign() {
        let hir = build_simple_module(vec!["let x = 3;", "x *= 4;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Mul, _, _))));
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x")));
    }
}
