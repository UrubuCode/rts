use crate::hir::nodes::{HirFunction, HirItem, HirModule};

use super::cfg::Terminator;
use super::{
    MirBinOp, MirInstruction, MirUnaryOp, TypedBasicBlock, TypedMirFunction, TypedMirModule, VReg,
};

use std::cell::RefCell;
use std::collections::HashMap;

use swc_common::{FileName, SourceMap, sync::Lrc};

/// Constant pool for deduplicating and hoisting constants
#[derive(Debug, Default)]
struct ConstantPool {
    numbers: HashMap<OrderedFloat, VReg>,
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
        let key = OrderedFloat::from(value);
        if let Some(&vreg) = self.numbers.get(&key) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.numbers.insert(key, vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstNumber(vreg, value));
            vreg
        }
    }

    fn get_or_create_string(&mut self, value: String, next_vreg: &mut u32) -> VReg {
        if let Some(&vreg) = self.strings.get(&value) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.strings.insert(value.clone(), vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstString(vreg, value));
            vreg
        }
    }

    fn get_or_create_bool(&mut self, value: bool, next_vreg: &mut u32) -> VReg {
        if let Some(&vreg) = self.booleans.get(&value) {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.booleans.insert(value, vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstBool(vreg, value));
            vreg
        }
    }

    fn get_or_create_null(&mut self, next_vreg: &mut u32) -> VReg {
        if let Some(vreg) = self.null_vreg {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.null_vreg = Some(vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstNull(vreg));
            vreg
        }
    }

    fn get_or_create_undef(&mut self, next_vreg: &mut u32) -> VReg {
        if let Some(vreg) = self.undef_vreg {
            vreg
        } else {
            let vreg = alloc(next_vreg);
            self.undef_vreg = Some(vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstUndef(vreg));
            vreg
        }
    }

    fn into_hoisted_instructions(self) -> Vec<MirInstruction> {
        self.hoisted_instructions
    }
}
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

#[derive(Debug, Clone)]
enum ConstValue {
    Number(f64),
    Bool(bool),
    Str(String),
    Null,
}

thread_local! {
    /// Constantes top-level conhecidas durante a construção do MIR tipado.
    /// Usado para inlinar referências a `const X = literal` em qualquer função.
    static TOP_LEVEL_CONSTS: RefCell<HashMap<String, ConstValue>> = RefCell::new(HashMap::new());

    /// Lookup de métodos de classe: nome do método → lista de nomes
    /// qualificados (`"Class::method"`). Usado pelo lowering de
    /// `obj.method(args)` para resolver a função estaticamente quando há
    /// exatamente uma classe no módulo com aquele método. Ambiguidade
    /// (dois `Class::method`) cai em `RuntimeEval` — resolução dinâmica
    /// por tipo fica para um passo futuro.
    static METHOD_LOOKUP: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
}

fn lookup_top_level_const(name: &str) -> Option<ConstValue> {
    TOP_LEVEL_CONSTS.with(|map| map.borrow().get(name).cloned())
}

/// Retorna o nome qualificado (`"Class::method"`) se houver exatamente
/// um método com esse nome no módulo. Caso contrário (zero ou ambíguo)
/// retorna None — o caller deve cair em `RuntimeEval`.
fn lookup_unique_method(method_name: &str) -> Option<String> {
    METHOD_LOOKUP.with(|map| {
        let map = map.borrow();
        match map.get(method_name) {
            Some(candidates) if candidates.len() == 1 => Some(candidates[0].clone()),
            _ => None,
        }
    })
}

/// Aliases de métodos JS nativos de `String` para o namespace `str`.
/// Quando `obj.method(args)` não resolve via `METHOD_LOOKUP` mas o nome
/// está nesta tabela, o lowering emite `Call("str.<snake>", [obj, args])`.
/// Essa reescrita é puramente sintática — o runtime `str.*` decide em
/// tempo de execução via pattern match. Se `obj` não for uma String, o
/// comportamento depende do handler específico (geralmente retorna
/// undefined/empty).
///
/// A tabela lista explicitamente cada par (JS method, namespace callee)
/// em vez de usar `camelToSnake`, porque alguns nomes do namespace foram
/// abreviados (`toUpperCase` → `to_upper`, não `to_upper_case`).
const STRING_METHOD_ALIASES: &[(&str, &str)] = &[
    ("replaceAll", "str.replace_all"),
    ("replace", "str.replace"),
    ("indexOf", "str.index_of"),
    ("lastIndexOf", "str.last_index_of"),
    ("startsWith", "str.starts_with"),
    ("endsWith", "str.ends_with"),
    ("includes", "str.includes"),
    ("toUpperCase", "str.to_upper"),
    ("toLowerCase", "str.to_lower"),
    ("padStart", "str.pad_start"),
    ("padEnd", "str.pad_end"),
    ("charAt", "str.char_at"),
    ("trimStart", "str.trim_start"),
    ("trimEnd", "str.trim_end"),
    ("slice", "str.slice"),
    ("trim", "str.trim"),
    ("split", "str.split"),
    ("repeat", "str.repeat"),
];

fn lookup_string_method_alias(method_name: &str) -> Option<&'static str> {
    STRING_METHOD_ALIASES
        .iter()
        .find(|(js, _)| *js == method_name)
        .map(|(_, ns)| *ns)
}

fn collect_method_lookup(hir: &HirModule) -> HashMap<String, Vec<String>> {
    let mut lookup: HashMap<String, Vec<String>> = HashMap::new();
    for class in &hir.classes {
        for method in &class.methods {
            // method.name vem como "Class::method" — extrair o sufixo.
            if let Some(idx) = method.name.rfind("::") {
                let short = &method.name[idx + 2..];
                lookup
                    .entry(short.to_string())
                    .or_default()
                    .push(method.name.clone());
            }
        }
    }
    lookup
}

fn collect_top_level_consts(hir: &HirModule) -> HashMap<String, ConstValue> {
    let mut consts: HashMap<String, ConstValue> = HashMap::new();

    for item in &hir.items {
        let HirItem::Statement(text) = item else {
            continue;
        };
        let Some(stmts) = try_parse_statement(text.trim()) else {
            continue;
        };
        for stmt in stmts {
            let Stmt::Decl(Decl::Var(var_decl)) = stmt else {
                continue;
            };
            if var_decl.kind != VarDeclKind::Const {
                continue;
            }
            for decl in &var_decl.decls {
                let Pat::Ident(ident) = &decl.name else {
                    continue;
                };
                let Some(init) = &decl.init else {
                    continue;
                };
                if let Some(value) = literal_const_value(init) {
                    consts.insert(ident.id.sym.to_string(), value);
                }
            }
        }
    }

    consts
}

fn emit_const_value(
    value: &ConstValue,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) -> VReg {
    let vreg = alloc(next_vreg);
    match value {
        ConstValue::Number(n) => {
            if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 {
                instructions.push(MirInstruction::ConstInt32(vreg, *n as i32));
            } else {
                instructions.push(MirInstruction::ConstNumber(vreg, *n));
            }
        }
        ConstValue::Bool(b) => instructions.push(MirInstruction::ConstBool(vreg, *b)),
        ConstValue::Str(s) => instructions.push(MirInstruction::ConstString(vreg, s.clone())),
        ConstValue::Null => instructions.push(MirInstruction::ConstNull(vreg)),
    }
    vreg
}

fn emit_const_value_pooled(
    value: &ConstValue,
    next_vreg: &mut u32,
    pool: &mut ConstantPool,
) -> VReg {
    match value {
        ConstValue::Number(n) => pool.get_or_create_number(*n, next_vreg),
        ConstValue::Bool(b) => pool.get_or_create_bool(*b, next_vreg),
        ConstValue::Str(s) => pool.get_or_create_string(s.clone(), next_vreg),
        ConstValue::Null => pool.get_or_create_null(next_vreg),
    }
}

fn literal_const_value(expr: &Expr) -> Option<ConstValue> {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(n) => Some(ConstValue::Number(n.value)),
            Lit::Bool(b) => Some(ConstValue::Bool(b.value)),
            Lit::Str(s) => Some(ConstValue::Str(s.value.to_string_lossy().into_owned())),
            Lit::Null(_) => Some(ConstValue::Null),
            _ => None,
        },
        // Unary minus sobre literal numérico: `const X = -1;`
        Expr::Unary(unary) if matches!(unary.op, UnaryOp::Minus) => match unary.arg.as_ref() {
            Expr::Lit(Lit::Num(n)) => Some(ConstValue::Number(-n.value)),
            _ => None,
        },
        Expr::Paren(paren) => literal_const_value(&paren.expr),
        _ => None,
    }
}

pub fn typed_build(hir: &HirModule) -> TypedMirModule {
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

    // Lower each body statement with constant pooling
    for statement in &function.body {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            lower_statement_text_with_pool(
                trimmed,
                &mut instructions,
                &mut func.next_vreg,
                &mut constant_pool,
            );
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
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        continue;
                    }
                };
                if let Some(init) = &decl.init {
                    let vreg = lower_expr_with_pool(
                        init,
                        original_text,
                        instructions,
                        next_vreg,
                        constant_pool,
                    );
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                } else {
                    let vreg = constant_pool.get_or_create_undef(next_vreg);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                }
            }
        }
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let vreg = lower_expr_with_pool(
                    arg,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
                instructions.push(MirInstruction::Return(Some(vreg)));
            } else {
                instructions.push(MirInstruction::Return(None));
            }
        }
        Stmt::Expr(expr_stmt) => {
            let _vreg = lower_expr_with_pool(
                &expr_stmt.expr,
                original_text,
                instructions,
                next_vreg,
                constant_pool,
            );
        }
        Stmt::Block(block_stmt) => {
            for inner_stmt in &block_stmt.stmts {
                lower_stmt_with_pool(
                    inner_stmt,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
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
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
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
            Lit::Num(n) => constant_pool.get_or_create_number(n.value, next_vreg),
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
                            instructions
                                .push(MirInstruction::Call(vreg, qualified, arg_vregs));
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
                        let vreg = lower_expr_with_pool(
                            &assign.right,
                            original_text,
                            instructions,
                            next_vreg,
                            constant_pool,
                        );
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
                        instructions.push(MirInstruction::StoreField(
                            obj_vreg,
                            field,
                            value,
                        ));
                        value
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign
                    | AssignOp::ModAssign => {
                        let load = alloc(next_vreg);
                        instructions.push(MirInstruction::LoadField(
                            load,
                            obj_vreg,
                            field.clone(),
                        ));
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
                        instructions.push(MirInstruction::StoreField(
                            obj_vreg, field, result,
                        ));
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
                let one = constant_pool.get_or_create_number(1.0, next_vreg);
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
                    instructions.push(MirInstruction::LoadField(
                        load,
                        obj_vreg,
                        field.clone(),
                    ));
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
                instructions
                    .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
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
                                let vreg = lower_expr(
                                    &arg.expr,
                                    original_text,
                                    instructions,
                                    next_vreg,
                                );
                                arg_vregs.push(vreg);
                            }
                            let vreg = alloc(next_vreg);
                            instructions
                                .push(MirInstruction::Call(vreg, qualified, arg_vregs));
                            return vreg;
                        }

                        // Alias para métodos JS nativos de String — ver
                        // versão pooled para detalhes.
                        if let Some(ns_callee) = lookup_string_method_alias(&method_short) {
                            let obj_vreg = lower_expr(
                                &member.obj,
                                original_text,
                                instructions,
                                next_vreg,
                            );
                            let mut arg_vregs = vec![obj_vreg];
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
                        instructions.push(MirInstruction::LoadField(
                            load,
                            obj_vreg,
                            field.clone(),
                        ));
                        let rhs =
                            lower_expr(&assign.right, original_text, instructions, next_vreg);
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
                        instructions.push(MirInstruction::StoreField(
                            obj_vreg, field, result,
                        ));
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
                    instructions.push(MirInstruction::LoadField(
                        load,
                        obj_vreg,
                        field.clone(),
                    ));
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
                instructions
                    .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
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
                        let vreg =
                            lower_expr(&arg.expr, original_text, instructions, next_vreg);
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

/// Extrai o nome de um campo acessado via `MemberProp`. Suporta apenas
/// acesso por identificador literal (`obj.field`) — acesso computado
/// (`obj[key]`) ainda cai em `RuntimeEval`.
fn member_prop_name(prop: &MemberProp) -> Option<String> {
    match prop {
        MemberProp::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

/// Se o target do assign for `obj.field`, retorna (obj_expr, field_name).
fn extract_member_assign_target<'a>(
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

fn lower_do_while_stmt(
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

fn lower_for_stmt(
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

fn lower_switch_stmt(
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
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 42.0))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Bind(name, _, false) if name == "x"))
        );
    }

    #[test]
    fn lowers_string_constant() {
        let hir = build_simple_module(vec![r#"const msg = "hello";"#]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::ConstString(_, s) if s == "hello"))
        );
    }

    #[test]
    fn lowers_binary_expression() {
        let hir = build_simple_module(vec!["const y = 1 + 2;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _)))
        );
    }

    #[test]
    fn lowers_function_call() {
        let hir = build_simple_module(vec!["io.print(42);"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Call(_, name, _) if name == "io.print"))
        );
    }

    #[test]
    fn falls_back_to_runtime_eval_for_if_statement() {
        let hir = build_simple_module(vec!["if (true) { io.print(1); }"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // If statements are now lowered natively with JumpIf/Label
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::JumpIf(_, _)))
        );
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
                loc: None,
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
                loc: None,
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
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadParam(_, 0)))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadParam(_, 1)))
        );
        // Should have Return
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Return(Some(_))))
        );
    }

    #[test]
    fn lowers_simple_assignment() {
        let hir = build_simple_module(vec!["let x = 1;", "x = 2;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have WriteBind for the assignment
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x"))
        );
        // The value 2 should be a ConstNumber
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 2.0))
        );
    }

    #[test]
    fn lowers_compound_assignment() {
        let hir = build_simple_module(vec!["let x = 10;", "x += 5;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have LoadBinding + BinOp(Add) + WriteBind
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadBinding(_, name) if name == "x"))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _)))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x"))
        );
    }

    #[test]
    fn lowers_postfix_increment() {
        let hir = build_simple_module(vec!["let i = 0;", "i++;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // Should have LoadBinding + ConstNumber(1) + BinOp(Add) + WriteBind
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::ConstNumber(_, v) if *v == 1.0))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _)))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "i"))
        );
    }

    #[test]
    fn lowers_prefix_decrement() {
        let hir = build_simple_module(vec!["let i = 5;", "--i;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Sub, _, _)))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "i"))
        );
    }

    #[test]
    fn lowers_mul_assign() {
        let hir = build_simple_module(vec!["let x = 3;", "x *= 4;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Mul, _, _)))
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::WriteBind(name, _) if name == "x"))
        );
    }

    #[test]
    fn lowers_class_method_via_parser() {
        // Full pipeline: parser -> HIR -> typed MIR. O corpo do método
        // (`return a + b;`) precisa chegar como instrução MIR real agora
        // que o pedaço 0a foi implementado.
        let source = r#"
            class Calc {
                static add(a: number, b: number): number {
                    return a + b;
                }
            }
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);

        // lower empurra métodos para module.functions com nome qualificado
        let method = hir
            .functions
            .iter()
            .find(|f| f.name == "Calc::add")
            .expect("Calc::add deve aparecer em hir.functions");
        assert_eq!(method.parameters.len(), 2);
        assert!(!method.body.is_empty(), "body do método deve ter snippets");

        let mir = typed_build(&hir);
        let typed = mir
            .functions
            .iter()
            .find(|f| f.name == "Calc::add")
            .expect("Calc::add deve aparecer no MIR tipado");
        assert_eq!(typed.param_count, 2);

        let instructions = &typed.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _))),
            "corpo do método deve emitir BinOp::Add"
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Return(Some(_)))),
            "corpo do método deve emitir Return com valor"
        );
    }

    #[test]
    fn lowers_new_expression_to_new_instance() {
        let hir = build_simple_module(vec!["const c = new Counter();"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::NewInstance(_, name) if name == "Counter")),
            "new Counter() deve emitir NewInstance(_, \"Counter\")"
        );
    }

    #[test]
    fn lowers_member_read_to_load_field() {
        let hir = build_simple_module(vec!["const c = new Box();", "const x = c.value;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadField(_, _, name) if name == "value")),
            "c.value deve emitir LoadField(_, _, \"value\")"
        );
    }

    #[test]
    fn lowers_member_assign_to_store_field() {
        let hir = build_simple_module(vec!["const c = new Box();", "c.value = 42;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::StoreField(_, name, _) if name == "value")),
            "c.value = 42 deve emitir StoreField(_, \"value\", _)"
        );
    }

    #[test]
    fn lowers_member_compound_assign_to_load_binop_store() {
        let hir = build_simple_module(vec!["const c = new Box();", "c.n += 5;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        // `c.n += 5` → LoadField + ConstNumber(5) + BinOp(Add) + StoreField
        let has_load = instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::LoadField(_, _, name) if name == "n"));
        let has_add = instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::BinOp(_, MirBinOp::Add, _, _)));
        let has_store = instructions
            .iter()
            .any(|i| matches!(i, MirInstruction::StoreField(_, name, _) if name == "n"));
        assert!(
            has_load && has_add && has_store,
            "compound assign deve emitir LoadField + BinOp + StoreField"
        );
    }

    #[test]
    fn instance_method_has_implicit_this_param() {
        // Método de instância deve ter `this` injetado como parâmetro 0.
        // O corpo acessa `this.count`, que vira LoadBinding("this") +
        // LoadField(_, _, "count").
        let source = r#"
            class Box {
                value: number;
                get(): number {
                    return this.value;
                }
            }
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);

        let method = hir
            .functions
            .iter()
            .find(|f| f.name == "Box::get")
            .expect("Box::get deve aparecer");
        assert_eq!(method.parameters.len(), 1, "this é injetado como param 0");
        assert_eq!(method.parameters[0].name, "this");

        let mir = typed_build(&hir);
        let typed = mir
            .functions
            .iter()
            .find(|f| f.name == "Box::get")
            .expect("Box::get deve aparecer no MIR tipado");
        assert_eq!(typed.param_count, 1);

        let instructions = &typed.blocks[0].instructions;
        // Espera-se Bind("this", ...) do entry + LoadBinding("this") + LoadField(_, _, "value")
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Bind(name, _, _) if name == "this")),
            "entry deve bindear `this`"
        );
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadField(_, _, name) if name == "value")),
            "corpo deve ler this.value via LoadField"
        );
    }

    #[test]
    fn static_method_has_no_implicit_this_param() {
        // Método estático NÃO recebe `this`.
        let source = r#"
            class Util {
                static double(x: number): number {
                    return x + x;
                }
            }
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);

        let method = hir
            .functions
            .iter()
            .find(|f| f.name == "Util::double")
            .expect("Util::double deve aparecer");
        assert_eq!(
            method.parameters.len(),
            1,
            "static method mantém só o param declarado"
        );
        assert_eq!(method.parameters[0].name, "x");
    }

    #[test]
    fn new_expression_with_constructor_invokes_it() {
        // new Point(3, 4) deve emitir NewInstance + Call("Point::constructor", [instance, 3, 4]).
        let source = r#"
            class Point {
                x: number;
                y: number;
                constructor(ix: number, iy: number) {
                    this.x = ix;
                    this.y = iy;
                }
            }
            const p = new Point(3, 4);
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);
        let mir = typed_build(&hir);
        let main = mir
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("main");
        let instructions = &main.blocks[0].instructions;

        assert!(
            instructions.iter().any(|i| matches!(
                i,
                MirInstruction::NewInstance(_, name) if name == "Point"
            )),
            "deve emitir NewInstance(_, \"Point\")"
        );
        assert!(
            instructions.iter().any(|i| matches!(
                i,
                MirInstruction::Call(_, callee, args)
                    if callee == "Point::constructor" && args.len() == 3
            )),
            "deve emitir Call(_, \"Point::constructor\", [instance, 3, 4])"
        );
    }

    #[test]
    fn lowers_string_method_alias_to_str_namespace() {
        // `s.replaceAll("foo", "X")` deve virar Call(_, "str.replace_all", [s, "foo", "X"])
        // sem que o usuario precise importar ou chamar o namespace explicitamente.
        let hir = build_simple_module(vec![
            r#"const s = "foo bar foo";"#,
            r#"s.replaceAll("foo", "X");"#,
        ]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;

        assert!(
            instructions.iter().any(|i| matches!(
                i,
                MirInstruction::Call(_, callee, args)
                    if callee == "str.replace_all" && args.len() == 3
            )),
            "s.replaceAll deve ser reescrito para str.replace_all com 3 args (receiver + 2)"
        );
    }

    #[test]
    fn lowers_string_method_slice_alias() {
        let hir = build_simple_module(vec![
            r#"const s = "hello";"#,
            r#"s.slice(0, 3);"#,
        ]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;

        assert!(
            instructions.iter().any(|i| matches!(
                i,
                MirInstruction::Call(_, callee, args)
                    if callee == "str.slice" && args.len() == 3
            )),
            "s.slice(a, b) deve virar str.slice(s, a, b)"
        );
    }

    #[test]
    fn lowers_instance_method_call_to_qualified_call() {
        // c.inc() deve virar Call(_, "Counter::inc", [c_handle]).
        let source = r#"
            class Counter {
                count: number;
                inc(): number {
                    this.count = this.count + 1;
                    return this.count;
                }
            }
            const c = new Counter();
            c.inc();
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);
        let mir = typed_build(&hir);
        let main = mir
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("main sintetica para statements top-level");
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions.iter().any(|i| matches!(
                i,
                MirInstruction::Call(_, callee, args) if callee == "Counter::inc" && args.len() == 1
            )),
            "c.inc() deve virar Call(_, \"Counter::inc\", [obj])"
        );
    }

    #[test]
    fn lowers_this_expression_to_load_binding_this() {
        // `this` fora de método ainda produz LoadBinding — só vira UB
        // em runtime (undefined). O lowering acima do HIR é quem injeta
        // `this` como parâmetro 0 em métodos de instância.
        let hir = build_simple_module(vec!["const x = this;"]);
        let mir = typed_build(&hir);
        let main = &mir.functions[0];
        let instructions = &main.blocks[0].instructions;
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::LoadBinding(_, name) if name == "this")),
            "Expr::This deve emitir LoadBinding(_, \"this\")"
        );
    }

    #[test]
    fn lowers_class_constructor_body_via_parser() {
        // Constructor body também precisa chegar ao MIR agora.
        let source = r#"
            class Counter {
                constructor(initial: number) {
                    let start = initial;
                }
            }
        "#;
        let program = crate::parser::parse_source(source).expect("parse ok");
        let resolver = crate::type_system::resolver::TypeResolver::default();
        let hir = crate::hir::lower::lower(&program, &resolver);

        let ctor = hir
            .functions
            .iter()
            .find(|f| f.name == "Counter::constructor")
            .expect("Counter::constructor deve aparecer em hir.functions");
        assert!(
            !ctor.body.is_empty(),
            "body do constructor deve ter snippets"
        );

        let mir = typed_build(&hir);
        let typed = mir
            .functions
            .iter()
            .find(|f| f.name == "Counter::constructor")
            .expect("Counter::constructor deve aparecer no MIR tipado");

        let instructions = &typed.blocks[0].instructions;
        // `let start = initial;` deve gerar ao menos um Bind("start", ...).
        assert!(
            instructions
                .iter()
                .any(|i| matches!(i, MirInstruction::Bind(name, _, _) if name == "start")),
            "constructor deve emitir Bind para a variável local `start`"
        );
    }
}
