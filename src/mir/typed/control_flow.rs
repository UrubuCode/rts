use super::*;

pub(super) fn extract_callee_name(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(expr) => extract_expr_name(expr),
        _ => None,
    }
}

pub(super) fn extract_expr_name(expr: &Expr) -> Option<String> {
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

pub(super) fn extract_simple_assign_target(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => match simple {
            SimpleAssignTarget::Ident(ident) => Some(ident.id.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Extrai o nome de um campo acessado via `MemberProp`. Suporta apenas
/// acesso por identificador literal (`obj.field`) — acesso computado
/// (`obj[key]`) ainda cai em `RuntimeEval`.
pub(super) fn member_prop_name(prop: &MemberProp) -> Option<String> {
    match prop {
        MemberProp::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

/// Se o target do assign for `obj.field`, retorna (obj_expr, field_name).
pub(super) fn extract_member_assign_target<'a>(
    target: &'a AssignTarget,
) -> Option<(&'a Expr, String)> {
    match target {
        AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
            let field = member_prop_name(&member.prop)?;
            Some((&member.obj, field))
        }
        _ => None,
    }
}

pub(super) fn map_bin_op(op: BinaryOp) -> Option<MirBinOp> {
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
        // `==` (abstract) e `===` (strict) sao mapeados para a mesma
        // operacao MirBinOp::Eq. O `binop_dispatch` runtime nao aplica
        // coercion elaborada ainda (usa PartialEq direto em RuntimeValue),
        // entao na pratica o comportamento e strict-like. Para TS onde
        // type checker ja garante tipos compativeis, isso e suficiente.
        // O mesmo vale para `!=` vs `!==`.
        BinaryOp::EqEq => Some(MirBinOp::Eq),
        BinaryOp::EqEqEq => Some(MirBinOp::Eq),
        BinaryOp::NotEq => Some(MirBinOp::Ne),
        BinaryOp::NotEqEq => Some(MirBinOp::Ne),
        BinaryOp::LogicalAnd => Some(MirBinOp::LogicAnd),
        BinaryOp::LogicalOr => Some(MirBinOp::LogicOr),
        _ => None,
    }
}

pub(super) fn map_unary_op(op: UnaryOp) -> Option<MirUnaryOp> {
    match op {
        UnaryOp::Minus => Some(MirUnaryOp::Negate),
        UnaryOp::Plus => Some(MirUnaryOp::Positive),
        UnaryOp::Bang => Some(MirUnaryOp::Not),
        _ => None,
    }
}

// Control flow lowering functions

pub(super) fn lower_if_stmt(
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

pub(super) fn lower_while_stmt(
    while_stmt: &WhileStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let id = *next_vreg;
    let header_label = format!("while_loop_{}", id);
    let body_label = format!("while_body_{}", id);
    let end_label = format!("while_end_{}", id);
    // Reserva um vreg para garantir que o id é único mesmo que nada seja alocado dentro.
    let _ = alloc(next_vreg);

    // Header: avalia teste e branch (condição live em cada iteração).
    instructions.push(MirInstruction::Label(header_label.clone()));
    let condition = lower_expr(&while_stmt.test, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::JumpIfNot(condition, end_label.clone()));

    // Body
    instructions.push(MirInstruction::Label(body_label));
    lower_stmt(&while_stmt.body, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::Jump(header_label));

    // End
    instructions.push(MirInstruction::Label(end_label));
}

pub(super) fn lower_do_while_stmt(
    do_while_stmt: &DoWhileStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let id = *next_vreg;
    let body_label = format!("do_while_body_{}", id);
    let condition_label = format!("do_while_condition_{}", id);
    let end_label = format!("do_while_end_{}", id);
    let _ = alloc(next_vreg);

    // Body
    instructions.push(MirInstruction::Label(body_label.clone()));
    lower_stmt(&do_while_stmt.body, original_text, instructions, next_vreg);

    // Continue target = condition check.
    instructions.push(MirInstruction::Label(condition_label));
    let condition = lower_expr(&do_while_stmt.test, original_text, instructions, next_vreg);
    instructions.push(MirInstruction::JumpIf(condition, body_label));

    instructions.push(MirInstruction::Label(end_label));
}

pub(super) fn lower_for_stmt(
    for_stmt: &ForStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let id = *next_vreg;
    let header_label = format!("for_loop_{}", id);
    let body_label = format!("for_body_{}", id);
    let update_label = format!("for_update_{}", id);
    let end_label = format!("for_end_{}", id);
    let _ = alloc(next_vreg);

    // Init (opcional)
    if let Some(init) = &for_stmt.init {
        match init {
            VarDeclOrExpr::VarDecl(var_decl) => {
                let fake = Stmt::Decl(Decl::Var(var_decl.clone()));
                lower_stmt(&fake, original_text, instructions, next_vreg);
            }
            VarDeclOrExpr::Expr(expr) => {
                let _ = lower_expr(expr, original_text, instructions, next_vreg);
            }
        }
    }

    // Header: avalia teste
    instructions.push(MirInstruction::Label(header_label.clone()));
    if let Some(test) = &for_stmt.test {
        let condition = lower_expr(test, original_text, instructions, next_vreg);
        instructions.push(MirInstruction::JumpIfNot(condition, end_label.clone()));
    }

    // Body
    instructions.push(MirInstruction::Label(body_label));
    lower_stmt(&for_stmt.body, original_text, instructions, next_vreg);

    // Update (continue target)
    instructions.push(MirInstruction::Label(update_label));
    if let Some(update) = &for_stmt.update {
        let _ = lower_expr(update, original_text, instructions, next_vreg);
    }
    instructions.push(MirInstruction::Jump(header_label));

    // End
    instructions.push(MirInstruction::Label(end_label));
}

pub(super) fn lower_switch_stmt(
    switch_stmt: &SwitchStmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    let id = *next_vreg;
    let body_label = format!("switch_body_{}", id);
    let end_label = format!("switch_end_{}", id);
    let _ = alloc(next_vreg);

    let discriminant = lower_expr(
        &switch_stmt.discriminant,
        original_text,
        instructions,
        next_vreg,
    );

    // Precomputa labels para cada case (inclusive default).
    let mut case_labels: Vec<String> = Vec::with_capacity(switch_stmt.cases.len());
    let mut default_index: Option<usize> = None;
    for (idx, case) in switch_stmt.cases.iter().enumerate() {
        case_labels.push(format!("switch_case_{}_{}", id, idx));
        if case.test.is_none() {
            default_index = Some(idx);
        }
    }

    // Tabela de comparação: testa cada case que tem test; se não bateu, cai no default ou no fim.
    // Marca o início do escopo do switch para que Break dentro vire Jump(switch_end_N).
    instructions.push(MirInstruction::Label(body_label));
    for (idx, case) in switch_stmt.cases.iter().enumerate() {
        if let Some(test) = case.test.as_deref() {
            let case_value = lower_expr(test, original_text, instructions, next_vreg);
            let cmp = alloc(next_vreg);
            instructions.push(MirInstruction::BinOp(
                cmp,
                MirBinOp::Eq,
                discriminant,
                case_value,
            ));
            instructions.push(MirInstruction::JumpIf(cmp, case_labels[idx].clone()));
        }
    }
    // Nenhum case explícito matched: pula para default se existir, senão para o fim.
    match default_index {
        Some(idx) => instructions.push(MirInstruction::Jump(case_labels[idx].clone())),
        None => instructions.push(MirInstruction::Jump(end_label.clone())),
    }

    // Emite os corpos em ordem com fall-through entre eles.
    for (idx, case) in switch_stmt.cases.iter().enumerate() {
        instructions.push(MirInstruction::Label(case_labels[idx].clone()));
        for stmt in &case.cons {
            lower_stmt(stmt, original_text, instructions, next_vreg);
        }
        // Fall-through para o próximo case acontece naturalmente (Jump para o próximo label
        // seria redundante com a ordem de emissão — mas Cranelift exige término explícito).
        if idx + 1 < case_labels.len() {
            instructions.push(MirInstruction::Jump(case_labels[idx + 1].clone()));
        } else {
            instructions.push(MirInstruction::Jump(end_label.clone()));
        }
    }

    instructions.push(MirInstruction::Label(end_label));
}
