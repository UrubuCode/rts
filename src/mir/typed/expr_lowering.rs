use super::*;

pub(super) fn lower_expr_with_pool(
    expr: &Expr,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
    constant_pool: &mut ConstantPool,
) -> VReg {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(n) => {
                constant_pool.get_or_create_number_hinted(n.value, current_hint(), next_vreg)
            }
            Lit::Str(s) => constant_pool
                .get_or_create_string(s.value.to_string_lossy().into_owned(), next_vreg),
            Lit::Bool(b) => constant_pool.get_or_create_bool(b.value, next_vreg),
            Lit::Null(_) => constant_pool.get_or_create_null(next_vreg),
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
            } else if let Some(konst) = lookup_top_level_const(&name) {
                emit_const_value_pooled(&konst, next_vreg, constant_pool)
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
            let lhs = lower_expr_with_pool(
                &bin.left,
                original_text,
                instructions,
                next_vreg,
                constant_pool,
            );
            let rhs = lower_expr_with_pool(
                &bin.right,
                original_text,
                instructions,
                next_vreg,
                constant_pool,
            );
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
            let arg = lower_expr_with_pool(
                &unary.arg,
                original_text,
                instructions,
                next_vreg,
                constant_pool,
            );
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::UnaryOp(vreg, op, arg));
            vreg
        }
        Expr::Call(call) => {
            // Caso especial: obj.method(...args) onde `method` é resolvido
            // estaticamente para uma função `Class::method`. O receiver
            // vira o primeiro argumento (this).
            if let Callee::Expr(callee_expr) = &call.callee {
                if let Expr::Member(member) = callee_expr.as_ref() {
                    if let Some(method_short) = member_prop_name(&member.prop) {
                        // Se o receptor é um identificador que coincide com
                        // o nome de uma classe, tratamos como chamada de
                        // método estático (`Calc.add(...)` → `Calc::add(...)`).
                        // Caso contrário, é chamada de método de instância.
                        if let Expr::Ident(ident) = member.obj.as_ref() {
                            let ident_name = ident.sym.to_string();
                            let static_qualified = format!("{}::{}", ident_name, method_short);
                            // Busca se existe `<ident>::<method>` no lookup.
                            let has_static = METHOD_LOOKUP.with(|map| {
                                let map = map.borrow();
                                map.get(method_short.as_str())
                                    .map(|v| v.contains(&static_qualified))
                                    .unwrap_or(false)
                            });
                            if has_static {
                                // Chamada estática: sem receiver.
                                let mut arg_vregs = Vec::new();
                                for arg in &call.args {
                                    let vreg = lower_expr_with_pool(
                                        &arg.expr,
                                        original_text,
                                        instructions,
                                        next_vreg,
                                        constant_pool,
                                    );
                                    arg_vregs.push(vreg);
                                }
                                let vreg = alloc(next_vreg);
                                instructions.push(MirInstruction::Call(
                                    vreg,
                                    static_qualified,
                                    arg_vregs,
                                ));
                                return vreg;
                            }
                        }

                        // Método de instância: resolve por nome único.
                        if let Some(qualified) = lookup_unique_method(&method_short) {
                            let obj_vreg = lower_expr_with_pool(
                                &member.obj,
                                original_text,
                                instructions,
                                next_vreg,
                                constant_pool,
                            );
                            let mut arg_vregs = vec![obj_vreg];
                            for arg in &call.args {
                                let vreg = lower_expr_with_pool(
                                    &arg.expr,
                                    original_text,
                                    instructions,
                                    next_vreg,
                                    constant_pool,
                                );
                                arg_vregs.push(vreg);
                            }
                            let vreg = alloc(next_vreg);
                            instructions.push(MirInstruction::Call(vreg, qualified, arg_vregs));
                            return vreg;
                        }

                        // Nenhum método de classe bateu. Se o nome for um
                        // método JS nativo de String (replaceAll, indexOf,
                        // startsWith, slice, etc.), reescreve como chamada
                        // ao namespace `str.*` com o receiver como primeiro
                        // argumento. O runtime do namespace cuida da checagem
                        // de tipo (string vs outro).
                        if let Some(ns_callee) = lookup_string_method_alias(&method_short) {
                            let obj_vreg = lower_expr_with_pool(
                                &member.obj,
                                original_text,
                                instructions,
                                next_vreg,
                                constant_pool,
                            );
                            let mut arg_vregs = vec![obj_vreg];
                            for arg in &call.args {
                                let vreg = lower_expr_with_pool(
                                    &arg.expr,
                                    original_text,
                                    instructions,
                                    next_vreg,
                                    constant_pool,
                                );
                                arg_vregs.push(vreg);
                            }
                            let vreg = alloc(next_vreg);
                            instructions.push(MirInstruction::Call(
                                vreg,
                                ns_callee.to_string(),
                                arg_vregs,
                            ));
                            return vreg;
                        }
                    }
                }
            }

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
                let vreg = lower_expr_with_pool(
                    &arg.expr,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
                arg_vregs.push(vreg);
            }
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::Call(vreg, callee_str, arg_vregs));
            vreg
        }
        Expr::Paren(paren) => lower_expr_with_pool(
            &paren.expr,
            original_text,
            instructions,
            next_vreg,
            constant_pool,
        ),
        Expr::Assign(assign) => {
            if let Some(name) = extract_simple_assign_target(&assign.left) {
                match assign.op {
                    AssignOp::Assign => {
                        let target_hint = lookup_binding_hint(&name);
                        let vreg = with_hint(target_hint, || {
                            lower_expr_with_pool(
                                &assign.right,
                                original_text,
                                instructions,
                                next_vreg,
                                constant_pool,
                            )
                        });
                        instructions.push(MirInstruction::WriteBind(name, vreg));
                        vreg
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign
                    | AssignOp::ModAssign => {
                        let target_hint = lookup_binding_hint(&name);
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadBinding(load, name.clone()));
                        let rhs = with_hint(target_hint, || {
                            lower_expr_with_pool(
                                &assign.right,
                                original_text,
                                instructions,
                                next_vreg,
                                constant_pool,
                            )
                        });
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
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        vreg
                    }
                }
            } else if let Some((obj_expr, field)) = extract_member_assign_target(&assign.left) {
                // obj.field (op)= value → suporta Assign simples e compound via
                // LoadField + BinOp + StoreField. Reaproveita obj_vreg nas duas
                // leituras (lado esquerdo e armazenamento) em vez de avaliar o
                // obj_expr duas vezes — side effects do obj_expr só rodam 1x.
                let obj_vreg = lower_expr_with_pool(
                    obj_expr,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
                match assign.op {
                    AssignOp::Assign => {
                        let value = lower_expr_with_pool(
                            &assign.right,
                            original_text,
                            instructions,
                            next_vreg,
                            constant_pool,
                        );
                        instructions.push(MirInstruction::StoreField(obj_vreg, field, value));
                        value
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign
                    | AssignOp::ModAssign => {
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadField(load, obj_vreg, field.clone()));
                        let rhs = lower_expr_with_pool(
                            &assign.right,
                            original_text,
                            instructions,
                            next_vreg,
                            constant_pool,
                        );
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
                        instructions.push(MirInstruction::StoreField(obj_vreg, field, result));
                        result
                    }
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
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
                let one =
                    constant_pool.get_or_create_number_hinted(1.0, lookup_binding_hint(&name), next_vreg);
                let op = if update.op == UpdateOp::PlusPlus {
                    MirBinOp::Add
                } else {
                    MirBinOp::Sub
                };
                let result = alloc(next_vreg);
                instructions.push(MirInstruction::BinOp(result, op, load, one));
                instructions.push(MirInstruction::WriteBind(name, result));
                if update.prefix { result } else { load }
            } else if let Expr::Member(member) = update.arg.as_ref() {
                // Suporta `this.x++` / `obj.field--` via LoadField + BinOp + StoreField.
                if let Some(field) = member_prop_name(&member.prop) {
                    let obj_vreg = lower_expr_with_pool(
                        &member.obj,
                        original_text,
                        instructions,
                        next_vreg,
                        constant_pool,
                    );
                    let load = alloc(next_vreg);
                    instructions.push(MirInstruction::LoadField(load, obj_vreg, field.clone()));
                    let one = constant_pool.get_or_create_number(1.0, next_vreg);
                    let op = if update.op == UpdateOp::PlusPlus {
                        MirBinOp::Add
                    } else {
                        MirBinOp::Sub
                    };
                    let result = alloc(next_vreg);
                    instructions.push(MirInstruction::BinOp(result, op, load, one));
                    instructions.push(MirInstruction::StoreField(obj_vreg, field, result));
                    if update.prefix { result } else { load }
                } else {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                    vreg
                }
            } else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                vreg
            }
        }
        Expr::This(_) => {
            // `this` é um parâmetro implícito do método de instância.
            // O lowering de método não-estático injeta `Bind("this", param0)`
            // no entry block antes do corpo, então aqui só precisamos ler o
            // binding. Fora de método de instância, o binding não existe e
            // o runtime vai devolver undefined, que é semântica JS aceitável.
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::LoadBinding(vreg, "this".to_string()));
            vreg
        }
        Expr::New(new_expr) => {
            // `new ClassName(args)` aloca um Object vazio via FN_NEW_INSTANCE
            // e, se a classe declarar um `ClassName::constructor`, invoca-o
            // com `this = obj_handle` + args. O valor da expressão é o
            // handle recém-alocado (não o retorno do constructor).
            let class_name = extract_expr_name(&new_expr.callee);
            let Some(name) = class_name else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                return vreg;
            };
            let instance_vreg = alloc(next_vreg);
            instructions.push(MirInstruction::NewInstance(instance_vreg, name.clone()));

            // Se a classe tem constructor, invoca-o com `this` + args.
            let ctor_qualified = format!("{}::constructor", name);
            let ctor_exists = METHOD_LOOKUP.with(|map| {
                let map = map.borrow();
                map.get("constructor")
                    .map(|v| v.contains(&ctor_qualified))
                    .unwrap_or(false)
            });
            if ctor_exists {
                let mut arg_vregs = vec![instance_vreg];
                if let Some(args) = &new_expr.args {
                    for arg in args {
                        let vreg = lower_expr_with_pool(
                            &arg.expr,
                            original_text,
                            instructions,
                            next_vreg,
                            constant_pool,
                        );
                        arg_vregs.push(vreg);
                    }
                }
                let ret = alloc(next_vreg);
                instructions.push(MirInstruction::Call(ret, ctor_qualified, arg_vregs));
            }

            instance_vreg
        }
        Expr::Member(member) => {
            // Lê o campo: `obj.field` → LoadField(dst, obj, "field")
            if let Some(field) = member_prop_name(&member.prop) {
                let obj_vreg = lower_expr_with_pool(
                    &member.obj,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::LoadField(vreg, obj_vreg, field));
                vreg
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

pub(super) fn lower_expr(
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
                instructions.push(MirInstruction::ConstString(
                    vreg,
                    s.value.to_string_lossy().into_owned(),
                ));
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
            } else if let Some(konst) = lookup_top_level_const(&name) {
                emit_const_value(&konst, instructions, next_vreg)
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
            // Caso especial: obj.method(...args) — ver versão pooled para
            // a explicação completa. Mesma lógica aqui sem o constant_pool.
            if let Callee::Expr(callee_expr) = &call.callee {
                if let Expr::Member(member) = callee_expr.as_ref() {
                    if let Some(method_short) = member_prop_name(&member.prop) {
                        if let Expr::Ident(ident) = member.obj.as_ref() {
                            let ident_name = ident.sym.to_string();
                            let static_qualified = format!("{}::{}", ident_name, method_short);
                            let has_static = METHOD_LOOKUP.with(|map| {
                                let map = map.borrow();
                                map.get(method_short.as_str())
                                    .map(|v| v.contains(&static_qualified))
                                    .unwrap_or(false)
                            });
                            if has_static {
                                let mut arg_vregs = Vec::new();
                                for arg in &call.args {
                                    let vreg = lower_expr(
                                        &arg.expr,
                                        original_text,
                                        instructions,
                                        next_vreg,
                                    );
                                    arg_vregs.push(vreg);
                                }
                                let vreg = alloc(next_vreg);
                                instructions.push(MirInstruction::Call(
                                    vreg,
                                    static_qualified,
                                    arg_vregs,
                                ));
                                return vreg;
                            }
                        }

                        if let Some(qualified) = lookup_unique_method(&method_short) {
                            let obj_vreg =
                                lower_expr(&member.obj, original_text, instructions, next_vreg);
                            let mut arg_vregs = vec![obj_vreg];
                            for arg in &call.args {
                                let vreg =
                                    lower_expr(&arg.expr, original_text, instructions, next_vreg);
                                arg_vregs.push(vreg);
                            }
                            let vreg = alloc(next_vreg);
                            instructions.push(MirInstruction::Call(vreg, qualified, arg_vregs));
                            return vreg;
                        }

                        // Alias para métodos JS nativos de String — ver
                        // versão pooled para detalhes.
                        if let Some(ns_callee) = lookup_string_method_alias(&method_short) {
                            let obj_vreg =
                                lower_expr(&member.obj, original_text, instructions, next_vreg);
                            let mut arg_vregs = vec![obj_vreg];
                            for arg in &call.args {
                                let vreg =
                                    lower_expr(&arg.expr, original_text, instructions, next_vreg);
                                arg_vregs.push(vreg);
                            }
                            let vreg = alloc(next_vreg);
                            instructions.push(MirInstruction::Call(
                                vreg,
                                ns_callee.to_string(),
                                arg_vregs,
                            ));
                            return vreg;
                        }
                    }
                }
            }

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
                        let vreg =
                            lower_expr(&assign.right, original_text, instructions, next_vreg);
                        instructions.push(MirInstruction::WriteBind(name, vreg));
                        vreg
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign
                    | AssignOp::ModAssign => {
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
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        vreg
                    }
                }
            } else if let Some((obj_expr, field)) = extract_member_assign_target(&assign.left) {
                let obj_vreg = lower_expr(obj_expr, original_text, instructions, next_vreg);
                match assign.op {
                    AssignOp::Assign => {
                        let value =
                            lower_expr(&assign.right, original_text, instructions, next_vreg);
                        instructions.push(MirInstruction::StoreField(obj_vreg, field, value));
                        value
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign
                    | AssignOp::ModAssign => {
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadField(load, obj_vreg, field.clone()));
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
                        instructions.push(MirInstruction::StoreField(obj_vreg, field, result));
                        result
                    }
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
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
                let op = if update.op == UpdateOp::PlusPlus {
                    MirBinOp::Add
                } else {
                    MirBinOp::Sub
                };
                let result = alloc(next_vreg);
                instructions.push(MirInstruction::BinOp(result, op, load, one));
                instructions.push(MirInstruction::WriteBind(name, result));
                if update.prefix { result } else { load }
            } else if let Expr::Member(member) = update.arg.as_ref() {
                if let Some(field) = member_prop_name(&member.prop) {
                    let obj_vreg = lower_expr(&member.obj, original_text, instructions, next_vreg);
                    let load = alloc(next_vreg);
                    instructions.push(MirInstruction::LoadField(load, obj_vreg, field.clone()));
                    let one = alloc(next_vreg);
                    instructions.push(MirInstruction::ConstNumber(one, 1.0));
                    let op = if update.op == UpdateOp::PlusPlus {
                        MirBinOp::Add
                    } else {
                        MirBinOp::Sub
                    };
                    let result = alloc(next_vreg);
                    instructions.push(MirInstruction::BinOp(result, op, load, one));
                    instructions.push(MirInstruction::StoreField(obj_vreg, field, result));
                    if update.prefix { result } else { load }
                } else {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                    vreg
                }
            } else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                vreg
            }
        }
        Expr::This(_) => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::LoadBinding(vreg, "this".to_string()));
            vreg
        }
        Expr::New(new_expr) => {
            let class_name = extract_expr_name(&new_expr.callee);
            let Some(name) = class_name else {
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                return vreg;
            };
            let instance_vreg = alloc(next_vreg);
            instructions.push(MirInstruction::NewInstance(instance_vreg, name.clone()));

            let ctor_qualified = format!("{}::constructor", name);
            let ctor_exists = METHOD_LOOKUP.with(|map| {
                let map = map.borrow();
                map.get("constructor")
                    .map(|v| v.contains(&ctor_qualified))
                    .unwrap_or(false)
            });
            if ctor_exists {
                let mut arg_vregs = vec![instance_vreg];
                if let Some(args) = &new_expr.args {
                    for arg in args {
                        let vreg = lower_expr(&arg.expr, original_text, instructions, next_vreg);
                        arg_vregs.push(vreg);
                    }
                }
                let ret = alloc(next_vreg);
                instructions.push(MirInstruction::Call(ret, ctor_qualified, arg_vregs));
            }

            instance_vreg
        }
        Expr::Member(member) => {
            if let Some(field) = member_prop_name(&member.prop) {
                let obj_vreg = lower_expr(&member.obj, original_text, instructions, next_vreg);
                let vreg = alloc(next_vreg);
                instructions.push(MirInstruction::LoadField(vreg, obj_vreg, field));
                vreg
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
