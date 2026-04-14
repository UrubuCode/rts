use super::*;

pub fn typed(hir: &HirModule) -> TypedMirModule {
    // Varre top-level procurando `const X = literal` para propagar inlining em todas as funções.
    let consts = collect_top_level_consts(hir);
    TOP_LEVEL_CONSTS.with(|map| *map.borrow_mut() = consts);

    // Indexa métodos de classe por nome curto → lista de nomes qualificados.
    // Permite resolver `obj.method(args)` para `Class::method` quando houver
    // exatamente uma classe no módulo com o método.
    let methods = collect_method_lookup(hir);
    METHOD_LOOKUP.with(|map| *map.borrow_mut() = methods);

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
            HirItem::Statement(stmt) => {
                if let Some(parsed) = &stmt.stmt {
                    lower_stmt(
                        parsed,
                        &stmt.text,
                        &mut top_level_instructions,
                        &mut top_level_vreg,
                    );
                } else {
                    let trimmed = stmt.text.trim();
                    if !trimmed.is_empty() {
                        lower_statement_text(
                            trimmed,
                            &mut top_level_instructions,
                            &mut top_level_vreg,
                        );
                    }
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
        if let Some(main) = module.functions.iter_mut().find(|f| f.name == "main") {
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
            param_is_numeric: Vec::new(),
            blocks: vec![TypedBasicBlock {
                label: "entry".to_string(),
                instructions: top_level_instructions,
                terminator: Terminator::Return,
            }],
            next_vreg: top_level_vreg,
            source_file: None,
            source_line: 0,
        });
    }

    // If no functions at all, create empty main
    if module.functions.is_empty() {
        module.functions.push(TypedMirFunction {
            name: "main".to_string(),
            param_count: 0,
            param_is_numeric: Vec::new(),
            blocks: vec![TypedBasicBlock {
                label: "entry".to_string(),
                instructions: vec![MirInstruction::Return(None)],
                terminator: Terminator::Return,
            }],
            next_vreg: 0,
            source_file: None,
            source_line: 0,
        });
    }

    module
}

/// Retorna true se o tipo anotado é garantidamente numérico (number/i32/f64/etc.).
/// Usado pra decidir se um parâmetro pode ser unboxed para NativeF64 uma única
/// vez no entry block, eliminando FN_UNBOX_NUMBER em cada uso dentro de loops.
///
/// Conservador: sem anotação ou com tipo desconhecido, devolve false — o
/// parâmetro permanece Handle e o adapt_to_kind genérico do BinOp cuida de
/// qualquer conversão necessária.
fn is_numeric_type_annotation(ann: Option<&crate::hir::annotations::TypeAnnotation>) -> bool {
    let Some(ann) = ann else {
        return false;
    };
    matches!(
        ann.name.as_str(),
        "number" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64" | "i16" | "u16" | "i8" | "u8"
    )
}

fn build_typed_function(function: &HirFunction) -> TypedMirFunction {
    let param_is_numeric = function
        .parameters
        .iter()
        .map(|p| is_numeric_type_annotation(p.type_annotation.as_ref()))
        .collect::<Vec<_>>();

    let mut func = TypedMirFunction {
        name: function.name.clone(),
        param_count: function.parameters.len(),
        param_is_numeric,
        blocks: Vec::new(),
        next_vreg: 0,
        source_file: function.loc.as_ref().map(|loc| loc.file.clone()),
        source_line: function.loc.as_ref().map(|loc| loc.line).unwrap_or(0),
    };

    let mut instructions: Vec<MirInstruction> = Vec::new();
    let mut constant_pool = ConstantPool::new();

    // Emit LoadParam + Bind for each parameter
    for (index, param) in function.parameters.iter().enumerate() {
        let vreg = func.alloc_vreg();
        instructions.push(MirInstruction::LoadParam(vreg, index));
        instructions.push(MirInstruction::Bind(param.name.clone(), vreg, true));
    }

    // Lower each body statement. Se o HIR ja carrega o Stmt SWC pre-parseado
    // (vindo do lowering do parser), consumimos direto — sem re-parse. Se
    // nao (casos raros de lowering sintetico), caimos no caminho de texto
    // que usa `try_parse_statement` como fallback.
    for statement in &function.body {
        if let Some(stmt) = &statement.stmt {
            lower_stmt_with_pool(
                stmt,
                &statement.text,
                &mut instructions,
                &mut func.next_vreg,
                &mut constant_pool,
            );
        } else {
            let trimmed = statement.text.trim();
            if !trimmed.is_empty() {
                lower_statement_text_with_pool(
                    trimmed,
                    &mut instructions,
                    &mut func.next_vreg,
                    &mut constant_pool,
                );
            }
        }
    }

    // Ensure function ends with a return
    let has_return = instructions
        .iter()
        .any(|i| matches!(i, MirInstruction::Return(_)));
    if !has_return {
        instructions.push(MirInstruction::Return(None));
    }

    // Prepend hoisted constants to the beginning of instructions
    let mut hoisted = constant_pool.into_hoisted_instructions();
    hoisted.extend(instructions);

    // Emit diagnostic warnings for any RuntimeEval fallbacks — o compilador
    // caiu em avaliacao dinamica para essas construcoes. Isso sinaliza ao
    // usuario que parte do codigo nao foi compilada nativamente.
    emit_runtime_eval_warnings(function, &hoisted);

    func.blocks.push(TypedBasicBlock {
        label: "entry".to_string(),
        instructions: hoisted,
        terminator: Terminator::Return,
    });

    func
}

/// Varre as instrucoes de uma funcao apos o lowering e emite um
/// `RichDiagnostic::warning` para cada `RuntimeEval` encontrado.
/// Usa o `loc` da funcao como span do warning — nao e perfeito, mas
/// localiza pelo menos a funcao que contem o fallback.
fn emit_runtime_eval_warnings(function: &HirFunction, instructions: &[MirInstruction]) {
    let Some(loc) = function.loc.as_ref() else {
        return;
    };
    let span = loc.to_span();

    for inst in instructions {
        if let MirInstruction::RuntimeEval(_, text) = inst {
            let snippet = first_line_snippet(text);
            let category = classify_runtime_eval(text);
            crate::diagnostics::reporter::emit(
                crate::diagnostics::reporter::RichDiagnostic::warning(
                    category.code,
                    format!("{} em '{}'", category.label, function.name),
                )
                .with_span(span)
                .with_note(format!("trecho: {snippet}"))
                .with_note(
                    "este trecho cai em avaliacao dinamica (RuntimeEval) — \
                     performance degradada, sem checagem de tipos",
                ),
            );
        }
    }
}

struct RuntimeEvalCategory {
    code: &'static str,
    label: &'static str,
}

fn classify_runtime_eval(text: &str) -> RuntimeEvalCategory {
    let t = text.trim_start();
    if t.starts_with("for") && t.contains(" in ") {
        RuntimeEvalCategory {
            code: "W003",
            label: "for-in nao compilado nativamente",
        }
    } else if t.starts_with("for") && t.contains(" of ") {
        RuntimeEvalCategory {
            code: "W004",
            label: "for-of nao compilado nativamente",
        }
    } else if t.starts_with("try") {
        RuntimeEvalCategory {
            code: "W005",
            label: "try/catch nao compilado nativamente",
        }
    } else if t.starts_with("throw") {
        RuntimeEvalCategory {
            code: "W006",
            label: "throw nao compilado nativamente",
        }
    } else if t.starts_with("async") || t.contains("await ") {
        RuntimeEvalCategory {
            code: "W007",
            label: "async/await nao compilado nativamente",
        }
    } else if t.contains("=>") {
        RuntimeEvalCategory {
            code: "W008",
            label: "arrow function nao compilada nativamente",
        }
    } else if t.starts_with('`') || t.contains("${") {
        RuntimeEvalCategory {
            code: "W009",
            label: "template literal nao compilado nativamente",
        }
    } else if t.starts_with("switch") {
        RuntimeEvalCategory {
            code: "W010",
            label: "switch nao compilado nativamente",
        }
    } else if t.starts_with("class") {
        RuntimeEvalCategory {
            code: "W011",
            label: "class expression nao compilada nativamente",
        }
    } else {
        RuntimeEvalCategory {
            code: "W001",
            label: "construcao nao compilada nativamente",
        }
    }
}

fn first_line_snippet(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.len() > 80 {
        format!("{}...", &line[..77])
    } else {
        line.to_string()
    }
}

fn inject_into_typed_main(main: &mut TypedMirFunction, statements: &mut Vec<MirInstruction>) {
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

pub(super) fn try_parse_statement(text: &str) -> Option<Vec<Stmt>> {
    // Fallback usado apenas em casos raros onde o parser interno nao
    // propagou o `Stmt` estruturado (ex: testes que constroem HIR
    // manualmente). No caminho feliz de `rts run` / `rts compile`,
    // esta funcao nao e chamada — o Stmt ja vem pronto no `HirStmt`.
    let cm: Lrc<SourceMap> = Default::default();
    let source = cm.new_source_file(FileName::Anon.into(), text.to_string());
    let mut parser = Parser::new(
        Syntax::Typescript(TsSyntax::default()),
        StringInput::from(&*source),
        None,
    );
    parser.parse_script().ok().map(|script| script.body)
}

fn lower_statement_text(text: &str, instructions: &mut Vec<MirInstruction>, next_vreg: &mut u32) {
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

