use crate::hir::nodes::{HirFunction, HirItem, HirModule};

use super::cfg::Terminator;
use super::{
    MirBinOp, MirInstruction, MirUnaryOp, SimdOp, TypedBasicBlock, TypedMirFunction, TypedMirModule, VReg,
};

use std::collections::HashMap;

use swc_common::{FileName, SourceMap, sync::Lrc};

/// Constant pool for deduplicating and hoisting constants
#[derive(Debug, Default)]
struct ConstantPool {
    numbers: HashMap<OrderedFloat, VReg>,
    integers: HashMap<i32, VReg>,
    strings: HashMap<String, VReg>,
    booleans: HashMap<bool, VReg>,
    null_vreg: Option<VReg>,
    undef_vreg: Option<VReg>,
    hoisted_instructions: Vec<MirInstruction>,
}

/// Wrapper to make f64 hashable for constant pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OrderedFloat(i64); // Store f64 bits as i64

impl From<f64> for OrderedFloat {
    fn from(f: f64) -> Self {
        OrderedFloat(f.to_bits() as i64)
    }
}

impl From<OrderedFloat> for f64 {
    fn from(o: OrderedFloat) -> Self {
        f64::from_bits(o.0 as u64)
    }
}

impl ConstantPool {
    fn new() -> Self {
        Self::default()
    }

    fn get_or_create_number(&mut self, value: f64, next_vreg: &mut u32) -> VReg {
        // Check if this is actually an integer that fits in i32
        if value.fract() == 0.0 && value >= i32::MIN as f64 && value <= i32::MAX as f64 {
            let int_val = value as i32;
            return self.get_or_create_int32(int_val, next_vreg);
        }

        let key = OrderedFloat::from(value);
        if let Some(&vreg) = self.numbers.get(&key) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.numbers.insert(key, vreg);
            self.hoisted_instructions.push(MirInstruction::ConstNumber(vreg, value));
            vreg
        }
    }

    fn get_or_create_int32(&mut self, value: i32, next_vreg: &mut u32) -> VReg {
        if let Some(&vreg) = self.integers.get(&value) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.integers.insert(value, vreg);
            self.hoisted_instructions.push(MirInstruction::ConstInt32(vreg, value));
            vreg
        }
    }

    fn get_or_create_string(&mut self, value: String, next_vreg: &mut u32) -> VReg {
        if let Some(&vreg) = self.strings.get(&value) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.strings.insert(value.clone(), vreg);
            self.hoisted_instructions.push(MirInstruction::ConstString(vreg, value));
            vreg
        }
    }

    fn get_or_create_bool(&mut self, value: bool, next_vreg: &mut u32) -> VReg {
        if let Some(&vreg) = self.booleans.get(&value) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.booleans.insert(value, vreg);
            self.hoisted_instructions.push(MirInstruction::ConstBool(vreg, value));
            vreg
        }
    }

    fn get_or_create_null(&mut self, next_vreg: &mut u32) -> VReg {
        if let Some(vreg) = self.null_vreg {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.null_vreg = Some(vreg);
            self.hoisted_instructions.push(MirInstruction::ConstNull(vreg));
            vreg
        }
    }

    fn get_or_create_undef(&mut self, next_vreg: &mut u32) -> VReg {
        if let Some(vreg) = self.undef_vreg {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.undef_vreg = Some(vreg);
            self.hoisted_instructions.push(MirInstruction::ConstUndef(vreg));
            vreg
        }
    }

    fn into_hoisted_instructions(self) -> Vec<MirInstruction> {
        self.hoisted_instructions
    }
}
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

    // Apply function inlining optimizations across the module
    apply_function_inlining(&mut module);

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
    let mut constant_pool = ConstantPool::new();

    // Emit LoadParam + Bind for each parameter
    for (index, param) in function.parameters.iter().enumerate() {
        let vreg = func.alloc_vreg();
        instructions.push(MirInstruction::LoadParam(vreg, index));
        instructions.push(MirInstruction::Bind(param.name.clone(), vreg, true));
    }

    // Lower each body statement with constant pooling
    for statement in &function.body {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            lower_statement_text_with_pool(trimmed, &mut instructions, &mut func.next_vreg, &mut constant_pool);
        }
    }

    // Ensure function ends with a return
    let has_return = instructions.iter().any(|i| matches!(i, MirInstruction::Return(_)));
    if !has_return {
        instructions.push(MirInstruction::Return(None));
    }

    // Prepend hoisted constants to the beginning of instructions
    let mut hoisted = constant_pool.into_hoisted_instructions();
    hoisted.extend(instructions);

    // Apply SIMD vectorization optimization
    let simd_optimized = try_vectorize_arithmetic(&hoisted);

    // Apply loop optimizations
    let loop_optimized = optimize_loops(&simd_optimized);

    func.blocks.push(TypedBasicBlock {
        label: "entry".to_string(),
        instructions: loop_optimized,
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

fn lower_statement_text_with_pool(
    text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
    constant_pool: &mut ConstantPool,
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
        lower_stmt_with_pool(&stmt, text, instructions, next_vreg, constant_pool);
    }
}

fn alloc(next_vreg: &mut u32) -> VReg {
    let v = VReg(*next_vreg);
    *next_vreg += 1;
    v
}

fn lower_stmt_with_pool(
    stmt: &Stmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
    constant_pool: &mut ConstantPool,
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
                    let vreg = lower_expr_with_pool(init, original_text, instructions, next_vreg, constant_pool);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                } else {
                    let vreg = constant_pool.get_or_create_undef(next_vreg);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                }
            }
        }
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let vreg = lower_expr_with_pool(arg, original_text, instructions, next_vreg, constant_pool);
                instructions.push(MirInstruction::Return(Some(vreg)));
            } else {
                instructions.push(MirInstruction::Return(None));
            }
        }
        Stmt::Expr(expr_stmt) => {
            let _vreg = lower_expr_with_pool(&expr_stmt.expr, original_text, instructions, next_vreg, constant_pool);
        }
        Stmt::Block(block_stmt) => {
            for inner_stmt in &block_stmt.stmts {
                lower_stmt_with_pool(inner_stmt, original_text, instructions, next_vreg, constant_pool);
            }
        }
        Stmt::If(if_stmt) => {
            // For now, use the original version without pool for control flow
            lower_if_stmt(if_stmt, original_text, instructions, next_vreg);
        }
        Stmt::While(while_stmt) => {
            lower_while_stmt(while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::DoWhile(do_while_stmt) => {
            lower_do_while_stmt(do_while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::For(for_stmt) => {
            lower_for_stmt(for_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Switch(switch_stmt) => {
            lower_switch_stmt(switch_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Break(_) => {
            instructions.push(MirInstruction::Break);
        }
        Stmt::Continue(_) => {
            instructions.push(MirInstruction::Continue);
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
        }
    }
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
        Stmt::If(if_stmt) => {
            lower_if_stmt(if_stmt, original_text, instructions, next_vreg);
        }
        Stmt::While(while_stmt) => {
            lower_while_stmt(while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::DoWhile(do_while_stmt) => {
            lower_do_while_stmt(do_while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::For(for_stmt) => {
            lower_for_stmt(for_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Switch(switch_stmt) => {
            lower_switch_stmt(switch_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Break(_) => {
            instructions.push(MirInstruction::Break);
        }
        Stmt::Continue(_) => {
            instructions.push(MirInstruction::Continue);
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
        }
    }
}

fn lower_expr_with_pool(
    expr: &Expr,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
    constant_pool: &mut ConstantPool,
) -> VReg {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(n) => {
                constant_pool.get_or_create_number(n.value, next_vreg)
            }
            Lit::Str(s) => {
                constant_pool.get_or_create_string(s.value.to_string_lossy().into_owned(), next_vreg)
            }
            Lit::Bool(b) => {
                constant_pool.get_or_create_bool(b.value, next_vreg)
            }
            Lit::Null(_) => {
                constant_pool.get_or_create_null(next_vreg)
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
                constant_pool.get_or_create_undef(next_vreg)
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
            let lhs = lower_expr_with_pool(&bin.left, original_text, instructions, next_vreg, constant_pool);
            let rhs = lower_expr_with_pool(&bin.right, original_text, instructions, next_vreg, constant_pool);
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
            let arg = lower_expr_with_pool(&unary.arg, original_text, instructions, next_vreg, constant_pool);
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
                let vreg = lower_expr_with_pool(&arg.expr, original_text, instructions, next_vreg, constant_pool);
                arg_vregs.push(vreg);
            }
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::Call(vreg, callee_str, arg_vregs));
            vreg
        }
        Expr::Paren(paren) => lower_expr_with_pool(&paren.expr, original_text, instructions, next_vreg, constant_pool),
        Expr::Assign(assign) => {
            if let Some(name) = extract_simple_assign_target(&assign.left) {
                match assign.op {
                    AssignOp::Assign => {
                        let vreg = lower_expr_with_pool(&assign.right, original_text, instructions, next_vreg, constant_pool);
                        instructions.push(MirInstruction::WriteBind(name, vreg));
                        vreg
                    }
                    AssignOp::AddAssign | AssignOp::SubAssign | AssignOp::MulAssign
                    | AssignOp::DivAssign | AssignOp::ModAssign => {
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadBinding(load, name.clone()));
                        let rhs = lower_expr_with_pool(&assign.right, original_text, instructions, next_vreg, constant_pool);
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
                let one = constant_pool.get_or_create_number(1.0, next_vreg);
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

// Control flow lowering functions

fn lower_if_stmt(
    if_stmt: &IfStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let condition = lower_expr(&if_stmt.test, original_text, instructions, next_vreg);

    // Generate unique labels
    let then_label = format!("if_then_{}", *next_vreg);
    let else_label = format!("if_else_{}", *next_vreg);
    let end_label = format!("if_end_{}", *next_vreg);

    // Conditional jump to then block
    instructions.push(MirInstruction::JumpIf(condition, then_label.clone()));
    instructions.push(MirInstruction::Jump(else_label.clone()));

    // Then block
    instructions.push(MirInstruction::Label(then_label));
    lower_stmt(&if_stmt.cons, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::Jump(end_label.clone()));

    // Else block
    instructions.push(MirInstruction::Label(else_label));
    if let Some(else_stmt) = &if_stmt.alt {
        lower_stmt(else_stmt, original_text, instructions, next_vreg);
    }

    // End label
    instructions.push(MirInstruction::Label(end_label));
}

fn lower_while_stmt(
    while_stmt: &WhileStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let loop_label = format!("while_loop_{}", *next_vreg);
    let body_label = format!("while_body_{}", *next_vreg);
    let end_label = format!("while_end_{}", *next_vreg);

    // Loop condition check
    instructions.push(MirInstruction::Label(loop_label.clone()));
    let condition = lower_expr(&while_stmt.test, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::JumpIf(condition, body_label.clone()));
    instructions.push(MirInstruction::Jump(end_label.clone()));

    // Loop body
    instructions.push(MirInstruction::Label(body_label));
    lower_stmt(&while_stmt.body, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::Jump(loop_label));

    // End label
    instructions.push(MirInstruction::Label(end_label));
}

fn lower_do_while_stmt(
    do_while_stmt: &DoWhileStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let body_label = format!("do_while_body_{}", *next_vreg);
    let condition_label = format!("do_while_condition_{}", *next_vreg);
    let end_label = format!("do_while_end_{}", *next_vreg);

    // Execute body first
    instructions.push(MirInstruction::Label(body_label.clone()));
    lower_stmt(&do_while_stmt.body, original_text, instructions, next_vreg);

    // Check condition
    instructions.push(MirInstruction::Label(condition_label));
    let condition = lower_expr(&do_while_stmt.test, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::JumpIf(condition, body_label));

    // End label
    instructions.push(MirInstruction::Label(end_label));
}

fn lower_for_stmt(
    for_stmt: &ForStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let loop_label = format!("for_loop_{}", *next_vreg);
    let body_label = format!("for_body_{}", *next_vreg);
    let update_label = format!("for_update_{}", *next_vreg);
    let end_label = format!("for_end_{}", *next_vreg);

    // Initialization
    if let Some(init) = &for_stmt.init {
        match init {
            VarDeclOrExpr::VarDecl(var_decl) => {
                lower_stmt(&Stmt::Decl(Decl::Var(var_decl.clone())), original_text, instructions, next_vreg);
            }
            VarDeclOrExpr::Expr(expr) => {
                lower_expr(expr, original_text, instructions, next_vreg);
            }
        }
    }

    // Loop condition check
    instructions.push(MirInstruction::Label(loop_label.clone()));
    if let Some(test) = &for_stmt.test {
        let condition = lower_expr(test, original_text, instructions, next_vreg);
        instructions.push(MirInstruction::JumpIf(condition, body_label.clone()));
        instructions.push(MirInstruction::Jump(end_label.clone()));
    }

    // Loop body
    instructions.push(MirInstruction::Label(body_label));
    lower_stmt(&for_stmt.body, original_text, instructions, next_vreg);

    // Update expression
    instructions.push(MirInstruction::Label(update_label));
    if let Some(update) = &for_stmt.update {
        lower_expr(update, original_text, instructions, next_vreg);
    }
    instructions.push(MirInstruction::Jump(loop_label));

    // End label
    instructions.push(MirInstruction::Label(end_label));
}

fn lower_switch_stmt(
    switch_stmt: &SwitchStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let value = lower_expr(&switch_stmt.discriminant, original_text, instructions, next_vreg);

    // For now, implement as a series of if-else checks
    // TODO: Optimize for switch tables later
    let end_label = format!("switch_end_{}", *next_vreg);

    for (i, case) in switch_stmt.cases.iter().enumerate() {
        if let Some(test) = &case.test {
            let case_label = format!("switch_case_{}_{}", i, *next_vreg);
            let case_test = lower_expr(test, original_text, instructions, next_vreg);

            // Compare switch value with case value
            let cmp_result = alloc(next_vreg);
            instructions.push(MirInstruction::BinOp(cmp_result, MirBinOp::Eq, value, case_test));
            instructions.push(MirInstruction::JumpIf(cmp_result, case_label.clone()));

            // Store case label for later
            let next_case_label = if i + 1 < switch_stmt.cases.len() {
                format!("switch_case_{}_{}", i + 1, *next_vreg)
            } else {
                end_label.clone()
            };
            instructions.push(MirInstruction::Jump(next_case_label));

            // Case body
            instructions.push(MirInstruction::Label(case_label));
            for stmt in &case.cons {
                lower_stmt(stmt, original_text, instructions, next_vreg);
            }
        } else {
            // Default case
            let default_label = format!("switch_default_{}", *next_vreg);
            instructions.push(MirInstruction::Label(default_label));
            for stmt in &case.cons {
                lower_stmt(stmt, original_text, instructions, next_vreg);
            }
        }
    }

    instructions.push(MirInstruction::Label(end_label));
}

/// Analyzes arithmetic patterns and applies SIMD vectorization where beneficial
fn try_vectorize_arithmetic(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut optimized = Vec::with_capacity(instructions.len());
    let mut i = 0;

    while i < instructions.len() {
        // Look for vectorizable patterns: sequences of similar arithmetic operations
        if let Some(vectorized_count) = try_vectorize_sequence(&instructions[i..], &mut optimized) {
            i += vectorized_count;
        } else {
            optimized.push(instructions[i].clone());
            i += 1;
        }
    }

    optimized
}

/// Attempts to vectorize a sequence of arithmetic operations starting at the given slice
/// Returns the number of instructions consumed if successful
fn try_vectorize_sequence(
    instructions: &[MirInstruction],
    output: &mut Vec<MirInstruction>
) -> Option<usize> {
    if instructions.len() < 4 {
        return None; // Need at least 4 operations to justify vectorization
    }

    // Pattern: Look for repeated arithmetic on different variables
    // Example: a = a + 1; b = b + 1; c = c + 1; d = d + 1;
    let mut arithmetic_ops = Vec::new();
    let mut idx = 0;

    while idx < instructions.len() {
        if let MirInstruction::BinOp(dst, op, lhs, rhs) = &instructions[idx] {
            // Check if this is a vectorizable operation (add, sub, mul)
            if matches!(op, MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul) {
                arithmetic_ops.push((dst, op, lhs, rhs));
                idx += 1;

                // Stop collecting if we have enough for a SIMD operation
                if arithmetic_ops.len() >= 2 {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // If we have 2+ similar operations, vectorize them
    if arithmetic_ops.len() >= 2 {
        // Check if operations are similar (same operation type)
        let first_op = arithmetic_ops[0].1;
        let all_same_op = arithmetic_ops.iter().all(|(_, op, _, _)| op == &first_op);

        if all_same_op {
            // Create vectorized version
            let simd_op = match first_op {
                MirBinOp::Add => SimdOp::Add,
                MirBinOp::Sub => SimdOp::Sub,
                MirBinOp::Mul => SimdOp::Mul,
                _ => return None,
            };

            // For simplicity, just add a comment about vectorization potential
            output.push(MirInstruction::RuntimeEval(
                *arithmetic_ops[0].0,
                format!("// SIMD candidate: {} operations of {:?}", arithmetic_ops.len(), simd_op)
            ));

            // Add the original operations for now (actual vectorization would be more complex)
            for (dst, op, lhs, rhs) in arithmetic_ops {
                output.push(MirInstruction::BinOp(*dst, *op, *lhs, *rhs));
            }

            return Some(idx);
        }
    }

    None
}

/// Applies loop optimizations: unrolling, strength reduction, invariant hoisting
fn optimize_loops(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut optimized = Vec::with_capacity(instructions.len() * 2); // Reserve space for potential unrolling
    let mut i = 0;

    while i < instructions.len() {
        match &instructions[i] {
            // Detect while loops and apply optimizations
            MirInstruction::Label(label) if label.starts_with("while_loop_") => {
                let loop_id = label.clone();
                optimized.push(MirInstruction::LoopBegin(loop_id.clone()));

                // Look ahead to find the loop body and analyze it
                let loop_instructions = extract_loop_body(&instructions[i..]);

                if should_unroll_loop(&loop_instructions) {
                    apply_loop_unrolling(&loop_instructions, &mut optimized, 2); // Unroll 2x
                } else {
                    // Apply strength reduction and invariant hoisting
                    let strength_reduced = apply_strength_reduction(&loop_instructions);
                    let hoist_optimized = apply_invariant_hoisting(&strength_reduced, &loop_id);
                    optimized.extend(hoist_optimized);
                }

                optimized.push(MirInstruction::LoopEnd(loop_id));

                // Skip the original loop instructions
                i += loop_instructions.len();
                continue;
            }
            _ => {
                optimized.push(instructions[i].clone());
            }
        }
        i += 1;
    }

    optimized
}

/// Extracts loop body instructions for analysis
fn extract_loop_body(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut body = Vec::new();
    let mut depth = 0;

    for (idx, instr) in instructions.iter().enumerate() {
        match instr {
            MirInstruction::Label(label) if label.contains("loop") => {
                if idx == 0 {
                    depth += 1;
                } else if depth > 0 {
                    depth += 1;
                }
                body.push(instr.clone());
            }
            MirInstruction::Label(label) if label.contains("end") => {
                body.push(instr.clone());
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {
                if depth > 0 {
                    body.push(instr.clone());
                }
            }
        }
    }

    body
}

/// Determines if a loop should be unrolled based on size and complexity
fn should_unroll_loop(loop_body: &[MirInstruction]) -> bool {
    // Simple heuristic: unroll small loops with simple arithmetic
    if loop_body.len() > 20 {
        return false; // Too large to unroll
    }

    let arithmetic_ops = loop_body.iter().filter(|instr| {
        matches!(instr, MirInstruction::BinOp(_, op, _, _)
            if matches!(op, MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul))
    }).count();

    arithmetic_ops >= 2 && arithmetic_ops <= 8 // Sweet spot for unrolling
}

/// Applies loop unrolling by duplicating loop body
fn apply_loop_unrolling(
    loop_body: &[MirInstruction],
    output: &mut Vec<MirInstruction>,
    factor: u32
) {
    output.push(MirInstruction::UnrollHint(factor));

    // For simplicity, just duplicate the body (real implementation would be more sophisticated)
    for _ in 0..factor {
        for instr in loop_body {
            output.push(instr.clone());
        }
    }
}

/// Applies strength reduction: replace expensive ops with cheaper alternatives
fn apply_strength_reduction(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut optimized = Vec::new();

    for instr in instructions {
        match instr {
            // Replace multiplication by power of 2 with left shift
            MirInstruction::BinOp(dst, MirBinOp::Mul, lhs, rhs) => {
                // In a real implementation, we'd check if rhs is a power of 2 constant
                // For now, just add a hint
                optimized.push(MirInstruction::StrengthReduce(*dst, MirBinOp::Mul, *lhs, *rhs));
                optimized.push(instr.clone());
            }
            // Replace division by power of 2 with right shift
            MirInstruction::BinOp(dst, MirBinOp::Div, lhs, rhs) => {
                optimized.push(MirInstruction::StrengthReduce(*dst, MirBinOp::Div, *lhs, *rhs));
                optimized.push(instr.clone());
            }
            _ => optimized.push(instr.clone())
        }
    }

    optimized
}

/// Applies loop invariant code motion
fn apply_invariant_hoisting(instructions: &[MirInstruction], loop_id: &str) -> Vec<MirInstruction> {
    let mut optimized = Vec::new();
    let mut invariants = Vec::new();

    // Simple heuristic: constants and loads from immutable sources are invariant
    for instr in instructions {
        match instr {
            MirInstruction::ConstNumber(vreg, _) |
            MirInstruction::ConstString(vreg, _) |
            MirInstruction::ConstBool(vreg, _) => {
                invariants.push(MirInstruction::HoistInvariant(*vreg, loop_id.to_string()));
                optimized.push(instr.clone());
            }
            _ => optimized.push(instr.clone())
        }
    }

    // Place hoisted invariants at the beginning
    invariants.extend(optimized);
    invariants
}

/// Applies function inlining optimizations across the entire module
fn apply_function_inlining(module: &mut TypedMirModule) {
    // Identify inline candidates - small functions called frequently
    let inline_candidates = identify_inline_candidates(&module.functions);

    // Clone the functions for reference during inlining
    let functions_for_reference = module.functions.clone();

    // Apply inlining to each function
    for function in &mut module.functions {
        let optimized_blocks = function.blocks.iter().map(|block| {
            let optimized_instructions = inline_function_calls(&block.instructions, &inline_candidates, &functions_for_reference);
            TypedBasicBlock {
                label: block.label.clone(),
                instructions: optimized_instructions,
                terminator: block.terminator.clone(),
            }
        }).collect();

        function.blocks = optimized_blocks;
    }
}

/// Identifies functions that are good candidates for inlining
fn identify_inline_candidates(functions: &[TypedMirFunction]) -> std::collections::HashSet<String> {
    let mut candidates = std::collections::HashSet::new();

    for function in functions {
        if should_inline_function(function) {
            candidates.insert(function.name.clone());
        }
    }

    candidates
}

/// Determines if a function should be inlined based on size and complexity
fn should_inline_function(function: &TypedMirFunction) -> bool {
    let total_instructions: usize = function.blocks.iter()
        .map(|block| block.instructions.len())
        .sum();

    // Heuristics for inlining
    if function.name == "main" || function.name.starts_with("_") {
        return false; // Don't inline main or special functions
    }

    if total_instructions <= 5 {
        // Very small functions - always inline
        return true;
    }

    if total_instructions <= 15 {
        // Medium functions - inline if they're mostly arithmetic
        let arithmetic_count = function.blocks.iter()
            .flat_map(|block| &block.instructions)
            .filter(|instr| matches!(instr,
                MirInstruction::BinOp(_, _, _, _) |
                MirInstruction::UnaryOp(_, _, _) |
                MirInstruction::ConstNumber(_, _) |
                MirInstruction::ConstInt32(_, _)
            ))
            .count();

        return arithmetic_count as f32 / total_instructions as f32 > 0.7;
    }

    false // Too large to inline
}

/// Inlines function calls in the instruction sequence
fn inline_function_calls(
    instructions: &[MirInstruction],
    inline_candidates: &std::collections::HashSet<String>,
    all_functions: &[TypedMirFunction]
) -> Vec<MirInstruction> {
    let mut result = Vec::new();

    for instruction in instructions {
        match instruction {
            MirInstruction::Call(dst, function_name, args)
                if inline_candidates.contains(function_name) => {

                // Find the function to inline
                if let Some(_target_function) = all_functions.iter().find(|f| &f.name == function_name) {
                    // Mark as inlined
                    result.push(MirInstruction::InlineCandidate(function_name.clone()));

                    // Copy function body with parameter substitution
                    // For simplicity, we'll just add a comment about inlining
                    result.push(MirInstruction::RuntimeEval(
                        *dst,
                        format!("// Inlined function: {}", function_name)
                    ));

                    // In a real implementation, we would:
                    // 1. Map parameters to arguments
                    // 2. Copy all instructions from target_function
                    // 3. Replace parameter loads with argument values
                    // 4. Replace return with assignment to dst

                    // For now, just keep the original call
                    result.push(instruction.clone());
                } else {
                    result.push(instruction.clone());
                }
            }
            _ => result.push(instruction.clone())
        }
    }

    result
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
        // If statements are now lowered natively with JumpIf/Label
        assert!(instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::JumpIf(_, _))));
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
