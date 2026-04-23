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
    ints32: HashMap<i32, VReg>,
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
        // Default consulta HINT_STACK para que qualquer path que nao recebe
        // hint explicito herde o contexto corrente (ex: Expr::Ident resolvendo
        // `const X = literal` dentro de `let acc: i32 = X` pega Int).
        self.get_or_create_number_hinted(value, current_hint(), next_vreg)
    }

    fn get_or_create_number_hinted(
        &mut self,
        value: f64,
        hint: NumericHint,
        next_vreg: &mut u32,
    ) -> VReg {
        if hint == NumericHint::Int
            && value.fract() == 0.0
            && value >= i32::MIN as f64
            && value <= i32::MAX as f64
        {
            let int_key = value as i32;
            if let Some(&vreg) = self.ints32.get(&int_key) {
                return vreg;
            }
            let vreg = alloc(next_vreg);
            self.ints32.insert(int_key, vreg);
            self.hoisted_instructions
                .push(MirInstruction::ConstInt32(vreg, int_key));
            return vreg;
        }
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

/// Hint de tipo numerico propagado durante lowering de expressoes.
/// Permite que literais inteiros decidam entre `ConstInt32` (i32 nativo) e
/// `ConstNumber` (f64), baseado no contexto estatico do TS. Essencial para
/// performance: sem hint, `let x: i32 = 3` vira f64 e loops aritmeticos
/// pagam fadd/fmul/fmod ao inves de iadd/imul/srem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum NumericHint {
    #[default]
    None,
    Int,
    Float,
}

/// Resolve nome de anotacao TS em hint numerico.
pub(super) fn numeric_hint_from_type_name(name: &str) -> NumericHint {
    match name {
        "i32" | "u32" | "i16" | "u16" | "i8" | "u8" => NumericHint::Int,
        "number" | "f64" | "f32" => NumericHint::Float,
        // i64/u64 nao cabem em ConstInt32; mantemos None para preservar f64
        // ate haver suporte nativo i64.
        _ => NumericHint::None,
    }
}

pub(super) fn numeric_hint_from_ts_type_ann(ann: Option<&TsTypeAnn>) -> NumericHint {
    let Some(ann) = ann else {
        return NumericHint::None;
    };
    match &*ann.type_ann {
        TsType::TsKeywordType(k) => {
            if matches!(k.kind, TsKeywordTypeKind::TsNumberKeyword) {
                NumericHint::Float
            } else {
                NumericHint::None
            }
        }
        TsType::TsTypeRef(r) => {
            if let TsEntityName::Ident(i) = &r.type_name {
                numeric_hint_from_type_name(i.sym.as_ref())
            } else {
                NumericHint::None
            }
        }
        _ => NumericHint::None,
    }
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

    /// Hints de tipo por nome de binding, populado em VarDecl. AssignExpr
    /// consulta este mapa para propagar hint para o lowering do rhs.
    /// Resetado no inicio de cada funcao via `reset_binding_hints()`.
    static BINDING_HINTS: RefCell<HashMap<String, NumericHint>> = RefCell::new(HashMap::new());

    /// Stack de hints ativos durante lowering recursivo. `lower_expr_*` empilha
    /// o hint do contexto (VarDecl type ann / AssignExpr target binding) antes
    /// de descer; literais `Lit::Num` no leaf consultam o topo.
    static HINT_STACK: RefCell<Vec<NumericHint>> = RefCell::new(Vec::new());
}

pub(super) fn reset_binding_hints() {
    BINDING_HINTS.with(|map| map.borrow_mut().clear());
    HINT_STACK.with(|s| s.borrow_mut().clear());
}

pub(super) fn set_binding_hint(name: &str, hint: NumericHint) {
    if hint == NumericHint::None {
        return;
    }
    BINDING_HINTS.with(|map| {
        map.borrow_mut().insert(name.to_string(), hint);
    });
}

pub(super) fn lookup_binding_hint(name: &str) -> NumericHint {
    BINDING_HINTS.with(|map| map.borrow().get(name).copied().unwrap_or_default())
}

pub(super) fn with_hint<F, R>(hint: NumericHint, f: F) -> R
where
    F: FnOnce() -> R,
{
    HINT_STACK.with(|s| s.borrow_mut().push(hint));
    let r = f();
    HINT_STACK.with(|s| {
        let _ = s.borrow_mut().pop();
    });
    r
}

pub(super) fn current_hint() -> NumericHint {
    HINT_STACK.with(|s| s.borrow().last().copied().unwrap_or_default())
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
        let HirItem::Statement(hir_stmt) = item else {
            continue;
        };
        // Consome o Stmt estruturado se presente; senao faz re-parse do texto.
        let stmts: Vec<Stmt> = if let Some(stmt) = &hir_stmt.stmt {
            vec![stmt.clone()]
        } else {
            match try_parse_statement(hir_stmt.text.trim()) {
                Some(s) => s,
                None => continue,
            }
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

