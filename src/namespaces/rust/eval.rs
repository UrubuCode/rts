use crate::namespaces::DispatchOutcome;
use crate::namespaces::abi;
use crate::namespaces::value::RuntimeValue;
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

#[derive(Debug, Clone)]
enum Flow {
    Normal(RuntimeValue),
    Break,
    Continue,
    Return(RuntimeValue),
}

pub(crate) fn eval_expression_text(expr: &str) -> RuntimeValue {
    let wrapped = format!("{expr};");
    let Some(statements) = parse_statements(&wrapped) else {
        return RuntimeValue::Undefined;
    };
    let Some(Stmt::Expr(expr_stmt)) = statements.first() else {
        return RuntimeValue::Undefined;
    };
    eval_expr(&expr_stmt.expr)
}

pub(crate) fn eval_statement_text(stmt: &str) -> RuntimeValue {
    let Some(statements) = parse_statements(stmt) else {
        return RuntimeValue::Undefined;
    };
    match eval_statements(&statements) {
        Flow::Normal(value) | Flow::Return(value) => value,
        Flow::Break | Flow::Continue => RuntimeValue::Undefined,
    }
}

fn parse_statements(source_text: &str) -> Option<Vec<Stmt>> {
    let cm: Lrc<SourceMap> = Default::default();
    let source = cm.new_source_file(FileName::Anon.into(), source_text.to_string());
    let mut parser = Parser::new(
        Syntax::Typescript(TsSyntax::default()),
        StringInput::from(&*source),
        None,
    );
    parser.parse_script().ok().map(|script| script.body)
}

fn eval_statements(statements: &[Stmt]) -> Flow {
    let mut last = RuntimeValue::Undefined;
    for statement in statements {
        match eval_stmt(statement) {
            Flow::Normal(value) => last = value,
            Flow::Break => return Flow::Break,
            Flow::Continue => return Flow::Continue,
            Flow::Return(value) => return Flow::Return(value),
        }
    }
    Flow::Normal(last)
}

fn eval_stmt(statement: &Stmt) -> Flow {
    match statement {
        Stmt::Decl(Decl::Var(var_decl)) => {
            for decl in &var_decl.decls {
                let Some(name) = pat_ident_name(&decl.name) else {
                    continue;
                };
                let value = decl
                    .init
                    .as_deref()
                    .map(eval_expr)
                    .unwrap_or(RuntimeValue::Undefined);
                let mutable = var_decl.kind != VarDeclKind::Const;
                abi::bind_runtime_identifier_value(name.as_str(), value, mutable);
            }
            Flow::Normal(RuntimeValue::Undefined)
        }
        Stmt::Expr(expr_stmt) => Flow::Normal(eval_expr(&expr_stmt.expr)),
        Stmt::Block(block_stmt) => eval_statements(&block_stmt.stmts),
        Stmt::If(if_stmt) => {
            let condition = eval_expr(&if_stmt.test);
            if condition.truthy() {
                eval_stmt(&if_stmt.cons)
            } else if let Some(alt) = &if_stmt.alt {
                eval_stmt(alt)
            } else {
                Flow::Normal(RuntimeValue::Undefined)
            }
        }
        Stmt::While(while_stmt) => {
            let mut last = RuntimeValue::Undefined;
            loop {
                if !eval_expr(&while_stmt.test).truthy() {
                    break;
                }
                match eval_stmt(&while_stmt.body) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => continue,
                    Flow::Return(value) => return Flow::Return(value),
                }
            }
            Flow::Normal(last)
        }
        Stmt::DoWhile(do_while_stmt) => {
            let mut last = RuntimeValue::Undefined;
            loop {
                match eval_stmt(&do_while_stmt.body) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => {}
                    Flow::Return(value) => return Flow::Return(value),
                }
                if !eval_expr(&do_while_stmt.test).truthy() {
                    break;
                }
            }
            Flow::Normal(last)
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                match init {
                    VarDeclOrExpr::VarDecl(var_decl) => {
                        let _ = eval_stmt(&Stmt::Decl(Decl::Var(var_decl.clone())));
                    }
                    VarDeclOrExpr::Expr(expr) => {
                        let _ = eval_expr(expr);
                    }
                }
            }

            let mut last = RuntimeValue::Undefined;
            loop {
                if let Some(test) = &for_stmt.test {
                    if !eval_expr(test).truthy() {
                        break;
                    }
                }

                match eval_stmt(&for_stmt.body) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => {}
                    Flow::Return(value) => return Flow::Return(value),
                }

                if let Some(update) = &for_stmt.update {
                    let _ = eval_expr(update);
                }
            }
            Flow::Normal(last)
        }
        Stmt::Switch(switch_stmt) => eval_switch_stmt(switch_stmt),
        Stmt::Break(_) => Flow::Break,
        Stmt::Continue(_) => Flow::Continue,
        Stmt::Return(return_stmt) => {
            let value = return_stmt
                .arg
                .as_deref()
                .map(eval_expr)
                .unwrap_or(RuntimeValue::Undefined);
            Flow::Return(value)
        }
        _ => Flow::Normal(RuntimeValue::Undefined),
    }
}

fn eval_switch_stmt(switch_stmt: &SwitchStmt) -> Flow {
    let discriminant = eval_expr(&switch_stmt.discriminant);
    let mut default_index = None;
    let mut start_index = None;

    for (index, case) in switch_stmt.cases.iter().enumerate() {
        if case.test.is_none() {
            default_index = Some(index);
            continue;
        }
        if start_index.is_none() {
            let Some(test) = case.test.as_deref() else {
                continue;
            };
            let case_value = eval_expr(test);
            if strict_equal(&discriminant, &case_value) {
                start_index = Some(index);
            }
        }
    }

    let Some(mut index) = start_index.or(default_index) else {
        return Flow::Normal(RuntimeValue::Undefined);
    };

    let mut last = RuntimeValue::Undefined;
    while index < switch_stmt.cases.len() {
        for statement in &switch_stmt.cases[index].cons {
            match eval_stmt(statement) {
                Flow::Normal(value) => last = value,
                Flow::Break => return Flow::Normal(last),
                Flow::Continue => return Flow::Continue,
                Flow::Return(value) => return Flow::Return(value),
            }
        }
        index += 1;
    }

    Flow::Normal(last)
}

fn eval_expr(expr: &Expr) -> RuntimeValue {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(number) => RuntimeValue::Number(number.value),
            Lit::Str(string) => RuntimeValue::String(string.value.to_string_lossy().into_owned()),
            Lit::Bool(boolean) => RuntimeValue::Bool(boolean.value),
            Lit::Null(_) => RuntimeValue::Null,
            _ => RuntimeValue::Undefined,
        },
        Expr::Ident(ident) => {
            let name = ident.sym.to_string();
            if name == "undefined" {
                RuntimeValue::Undefined
            } else {
                abi::read_runtime_identifier_value(name.as_str())
            }
        }
        Expr::Paren(paren) => eval_expr(&paren.expr),
        Expr::Seq(sequence) => {
            let mut last = RuntimeValue::Undefined;
            for item in &sequence.exprs {
                last = eval_expr(item);
            }
            last
        }
        Expr::Bin(binary) => eval_bin_expr(binary),
        Expr::Unary(unary) => eval_unary_expr(unary),
        Expr::Assign(assign) => eval_assign_expr(assign),
        Expr::Update(update) => eval_update_expr(update),
        Expr::Call(call) => eval_call_expr(call),
        _ => RuntimeValue::Undefined,
    }
}

fn eval_unary_expr(unary: &UnaryExpr) -> RuntimeValue {
    let value = eval_expr(&unary.arg);
    match unary.op {
        UnaryOp::Minus => RuntimeValue::Number(-value.to_number()),
        UnaryOp::Plus => RuntimeValue::Number(value.to_number()),
        UnaryOp::Bang => RuntimeValue::Bool(!value.truthy()),
        _ => RuntimeValue::Undefined,
    }
}

fn eval_bin_expr(binary: &BinExpr) -> RuntimeValue {
    if matches!(binary.op, BinaryOp::LogicalAnd) {
        let lhs = eval_expr(&binary.left);
        if lhs.truthy() {
            return eval_expr(&binary.right);
        }
        return lhs;
    }

    if matches!(binary.op, BinaryOp::LogicalOr) {
        let lhs = eval_expr(&binary.left);
        if lhs.truthy() {
            return lhs;
        }
        return eval_expr(&binary.right);
    }

    let lhs = eval_expr(&binary.left);
    let rhs = eval_expr(&binary.right);

    match binary.op {
        BinaryOp::Add => {
            if lhs.is_string_like() || rhs.is_string_like() {
                RuntimeValue::String(format!(
                    "{}{}",
                    lhs.to_runtime_string(),
                    rhs.to_runtime_string()
                ))
            } else {
                RuntimeValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        BinaryOp::Sub => RuntimeValue::Number(lhs.to_number() - rhs.to_number()),
        BinaryOp::Mul => RuntimeValue::Number(lhs.to_number() * rhs.to_number()),
        BinaryOp::Div => RuntimeValue::Number(lhs.to_number() / rhs.to_number()),
        BinaryOp::Mod => RuntimeValue::Number(lhs.to_number() % rhs.to_number()),
        BinaryOp::Lt => RuntimeValue::Bool(lhs.to_number() < rhs.to_number()),
        BinaryOp::LtEq => RuntimeValue::Bool(lhs.to_number() <= rhs.to_number()),
        BinaryOp::Gt => RuntimeValue::Bool(lhs.to_number() > rhs.to_number()),
        BinaryOp::GtEq => RuntimeValue::Bool(lhs.to_number() >= rhs.to_number()),
        BinaryOp::EqEqEq | BinaryOp::EqEq => RuntimeValue::Bool(strict_equal(&lhs, &rhs)),
        BinaryOp::NotEqEq | BinaryOp::NotEq => RuntimeValue::Bool(!strict_equal(&lhs, &rhs)),
        _ => RuntimeValue::Undefined,
    }
}

fn eval_assign_expr(assign: &AssignExpr) -> RuntimeValue {
    let Some(name) = assign_target_name(&assign.left) else {
        return RuntimeValue::Undefined;
    };

    let rhs = eval_expr(&assign.right);
    let next = match assign.op {
        AssignOp::Assign => rhs,
        AssignOp::AddAssign => {
            let lhs = abi::read_runtime_identifier_value(name.as_str());
            if lhs.is_string_like() || rhs.is_string_like() {
                RuntimeValue::String(format!(
                    "{}{}",
                    lhs.to_runtime_string(),
                    rhs.to_runtime_string()
                ))
            } else {
                RuntimeValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        AssignOp::SubAssign => {
            let lhs = abi::read_runtime_identifier_value(name.as_str());
            RuntimeValue::Number(lhs.to_number() - rhs.to_number())
        }
        AssignOp::MulAssign => {
            let lhs = abi::read_runtime_identifier_value(name.as_str());
            RuntimeValue::Number(lhs.to_number() * rhs.to_number())
        }
        AssignOp::DivAssign => {
            let lhs = abi::read_runtime_identifier_value(name.as_str());
            RuntimeValue::Number(lhs.to_number() / rhs.to_number())
        }
        AssignOp::ModAssign => {
            let lhs = abi::read_runtime_identifier_value(name.as_str());
            RuntimeValue::Number(lhs.to_number() % rhs.to_number())
        }
        _ => RuntimeValue::Undefined,
    };

    abi::write_runtime_identifier_value(name.as_str(), next.clone());
    next
}

fn eval_update_expr(update: &UpdateExpr) -> RuntimeValue {
    let Expr::Ident(ident) = update.arg.as_ref() else {
        return RuntimeValue::Undefined;
    };

    let name = ident.sym.to_string();
    let current = abi::read_runtime_identifier_value(name.as_str()).to_number();
    let next = match update.op {
        UpdateOp::PlusPlus => current + 1.0,
        UpdateOp::MinusMinus => current - 1.0,
    };
    abi::write_runtime_identifier_value(name.as_str(), RuntimeValue::Number(next));

    if update.prefix {
        RuntimeValue::Number(next)
    } else {
        RuntimeValue::Number(current)
    }
}

fn eval_call_expr(call: &CallExpr) -> RuntimeValue {
    let Some(callee) = callee_name(&call.callee) else {
        return RuntimeValue::Undefined;
    };

    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        args.push(eval_expr(&arg.expr));
    }

    let Some(outcome) = crate::namespaces::dispatch(callee.as_str(), &args) else {
        return RuntimeValue::Undefined;
    };

    match outcome {
        DispatchOutcome::Value(value) => value,
        DispatchOutcome::Emit(message) => {
            if callee == "io.stderr_write" {
                eprint!("{message}");
            } else if callee == "io.stdout_write" {
                print!("{message}");
            } else {
                println!("{message}");
            }
            RuntimeValue::Undefined
        }
        DispatchOutcome::Panic(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn strict_equal(lhs: &RuntimeValue, rhs: &RuntimeValue) -> bool {
    lhs == rhs
}

fn pat_ident_name(pattern: &Pat) -> Option<String> {
    match pattern {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        _ => None,
    }
}

fn assign_target_name(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => match simple {
            SimpleAssignTarget::Ident(ident) => Some(ident.id.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn callee_name(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(expr) => expr_name(expr),
        _ => None,
    }
}

fn expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Member(member) => {
            let object = expr_name(&member.obj)?;
            let property = match &member.prop {
                MemberProp::Ident(ident) => ident.sym.to_string(),
                _ => return None,
            };
            Some(format!("{object}.{property}"))
        }
        _ => None,
    }
}
