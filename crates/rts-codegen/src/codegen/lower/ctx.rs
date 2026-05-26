//! Per-function compilation context.
//!
//! `FnCtx` wraps a Cranelift `FunctionBuilder` and carries the bookkeeping
//! needed to compile one function: local/global variables, extern cache,
//! string literal data, and loop control targets.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{
    Block, FuncRef, InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind, Value,
    types as cl,
};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{DataId, Module};

use crate::abi::types::AbiType;

/// Cranelift type tag of a compiled value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValTy {
    I32,
    I64,
    F64,
    Bool,
    /// GC handle (string pool, etc.). Backed by I64. `+` faz string concat.
    Handle,
    /// Bits opacos de 64 bits (ex.: ptr/handle de namespace nao-string como
    /// `buffer.ptr`, `atomic.*`). Backed by I64. `+` faz aritmetica inteira.
    U64,
    /// Narrow signed integers — ABI ainda viaja em i64 (sextend/ireduce no
    /// boundary), mas aritmetica precisa mascarar pra preservar overflow.
    /// Hoje usados apenas em rotas que passam pelo MIR (que tem o pass
    /// `narrow` automatico). Callsite AST trata como I64 via `is_i64_like`.
    I8,
    I16,
    /// Narrow unsigned integers — mesmo modelo, com mask em vez de sextend.
    U8,
    U16,
}

impl ValTy {
    pub fn cl_type(self) -> cranelift_codegen::ir::Type {
        match self {
            ValTy::I32 => cl::I32,
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => cl::I64,
            ValTy::F64 => cl::F64,
            // Narrow ints ABI-compativel com I64; banner mascara em ops.
            ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => cl::I64,
        }
    }

    /// Largura em bits do valor logico (nao do ABI). Usado pra escolher mask
    /// de overflow. Retorna 64 pra tipos que nao precisam de mask.
    pub fn narrow_width(self) -> Option<u8> {
        match self {
            ValTy::I8 | ValTy::U8 => Some(8),
            ValTy::I16 | ValTy::U16 => Some(16),
            _ => None,
        }
    }

    /// True quando o tipo eh um inteiro narrow signed.
    pub fn is_signed_narrow(self) -> bool {
        matches!(self, ValTy::I8 | ValTy::I16)
    }

    pub fn from_annotation(ann: &str) -> Self {
        let trimmed = ann.trim();
        match trimmed {
            "i32" | "I32" => return ValTy::I32,
            "f64" | "F64" | "number" => return ValTy::F64,
            "bool" | "boolean" => return ValTy::Bool,
            "string" | "str" => return ValTy::Handle,
            "i8" | "I8" => return ValTy::I8,
            "i16" | "I16" => return ValTy::I16,
            "u8" | "U8" => return ValTy::U8,
            "u16" | "U16" => return ValTy::U16,
            _ => {}
        }
        // (cross-runtime followup) Type predicate `param is Type` ou
        // `asserts param is Type` — semantica de retorno eh boolean.
        // Sem isso, console.log(fn(...)) imprime 0/1 em vez de true/false.
        if trimmed.contains(" is ") || trimmed.starts_with("asserts ") {
            return ValTy::Bool;
        }
        // Tipos de array (`T[]`, `Array<T>`, `ReadonlyArray<T>`) sao
        // representados como handles GC (Vec<i64>) — `.length`, `.push`,
        // `for...of` dispatcham via gc.handle_len/vec_*.
        if trimmed.ends_with("[]") {
            return ValTy::Handle;
        }
        if trimmed.starts_with("Array<")
            || trimmed.starts_with("ReadonlyArray<")
            || trimmed.starts_with("readonly ")
        {
            return ValTy::Handle;
        }
        // \`Promise<X>\` (resultado tipico de async fn): RTS nao tem
        // Promise propria — async vira sync stripped, entao o tipo
        // efetivo eh X. Cobre \`Promise<string>\`, \`Promise<number>\`, etc.
        if let Some(rest) = trimmed.strip_prefix("Promise<") {
            if let Some(inner) = rest.strip_suffix('>') {
                return ValTy::from_annotation(inner);
            }
        }
        // Union types raw da source: tenta resolver para um tipo unico
        // se todos os ramos sao do mesmo. Usa parsing textual simples.
        if trimmed.contains('|') {
            let parts: Vec<&str> = trimmed
                .split('|')
                .map(|s| s.trim().trim_start_matches('(').trim_end_matches(')').trim())
                .filter(|s| !s.is_empty() && *s != "null" && *s != "undefined")
                .collect();
            if !parts.is_empty() {
                let mut acc: Option<ValTy> = None;
                for p in parts {
                    // Cada parte: pode ser keyword, string literal "...",
                    // numero literal, etc.
                    let cur = if p.starts_with('"') && p.ends_with('"') {
                        ValTy::Handle
                    } else if p.starts_with('\'') && p.ends_with('\'') {
                        ValTy::Handle
                    } else if p.parse::<f64>().is_ok() {
                        ValTy::I64
                    } else {
                        ValTy::from_annotation(p)
                    };
                    match acc {
                        None => acc = Some(cur),
                        Some(prev) if prev == cur => {}
                        _ => return ValTy::I64,
                    }
                }
                if let Some(t) = acc {
                    return t;
                }
            }
        }
        ValTy::I64
    }

    pub fn from_abi(abi: AbiType) -> Self {
        match abi {
            AbiType::I32 => ValTy::I32,
            AbiType::F64 => ValTy::F64,
            AbiType::Bool => ValTy::Bool,
            AbiType::Handle => ValTy::Handle,
            AbiType::U64 => ValTy::U64,
            _ => ValTy::I64,
        }
    }
}

/// A typed Cranelift value.
#[derive(Debug, Clone, Copy)]
pub struct TypedVal {
    pub val: Value,
    pub ty: ValTy,
}

impl TypedVal {
    pub fn new(val: Value, ty: ValTy) -> Self {
        Self { val, ty }
    }
}

/// Slot for a local variable.
#[derive(Debug, Clone)]
pub struct LocalVar {
    pub var: Variable,
    pub ty: ValTy,
    /// True when declared with `const` — reassignment must be rejected.
    pub is_const: bool,
}

/// Module-scope global lowered to a data symbol.
#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub data_id: DataId,
    pub ty: ValTy,
}

/// Callable user function signature visible while lowering calls.
#[derive(Debug, Clone)]
pub struct UserFnAbi {
    pub params: Vec<ValTy>,
    pub ret: Option<ValTy>,
    /// Class name when the function's declared return type is a known class.
    /// Populated by `compile_program` after class declarations are collected.
    pub ret_class: Option<String>,
    /// (cross-runtime #799) `true` quando o primeiro parametro declarado eh
    /// `this` (ex: `function Point(this: any, x, y)`). Indica que invocacoes
    /// via Reflect.construct / fn.call devem prepender o thisArg como
    /// primeiro argumento Cranelift (em vez de empilhar no slot
    /// thread-local de THIS_PUSH).
    pub has_this_param: bool,
}

/// Decide se o codegen deve usar layout nativo (flat) para uma classe.
///
/// Mecanismo opt-in para a fase de coexistencia da #147 — fora dos casos
/// abaixo o codegen continua bit-identico ao caminho `Map`-based.
///
/// Gatilhos (qualquer um ativa):
/// - Nome da classe comeca com `__Flat` — permite teste sem env vars
///   (`rts:test` nao propaga env). Documentado no helper para que classes
///   de proucao nao sejam afetadas acidentalmente.
/// - Variavel de ambiente `RTS_FLAT_CLASSES` lista o nome (csv).
///
/// `RTS_FLAT_CLASSES=Point,Color cargo run -- run x.ts` ativa flat para
/// `Point` e `Color`. Vazio/ausente = nenhuma classe flat por env.
pub fn is_class_flat_enabled(name: &str) -> bool {
    if name.starts_with("__Flat") {
        return true;
    }
    let Ok(list) = std::env::var("RTS_FLAT_CLASSES") else {
        return false;
    };
    list.split(',').any(|n| n.trim() == name)
}

/// Slot de um campo numa classe com layout nativo.
///
/// Cada campo ocupa 8 bytes (alinhamento simples). `is_handle` marca os
/// slots que carregam GC handles (strings, instancias) — necessario para
/// trace futuro do GC.
#[derive(Debug, Clone)]
pub struct FieldSlot {
    pub name: String,
    pub offset: u32,
    pub ty: ValTy,
    pub is_handle: bool,
}

/// Layout nativo computado para uma classe elegivel.
///
/// Slot 0 (`offset=0`) sempre reservado para o tag `__rts_class` (handle
/// de string), por isso `parent_size` minimo é 8 quando nao ha super.
/// Quando ha super, os campos desta classe comecam em `parent_size`
/// (que ja inclui o slot do tag).
#[derive(Debug, Clone)]
pub struct ClassLayout {
    pub fields: Vec<FieldSlot>,
    pub size_bytes: u32,
    pub parent_size: u32,
}

/// Metadata estatica de uma classe TS/JS, consumida pelo codegen para
/// resolver `new C(...)`, `obj.method(...)` e dispatch de `super`.
///
/// Usado em compile-time apenas — runtime nao conhece classes, apenas
/// handles de map/vec.
#[derive(Debug, Clone)]
pub struct ClassMeta {
    pub name: String,
    pub super_class: Option<String>,
    /// Nomes de metodos definidos diretamente nesta classe (instance).
    pub methods: Vec<String>,
    /// Tipo declarado de cada field (via `class { x: string }`). Usado
    /// para tipar o resultado de `obj.field` quando a classe da var e
    /// conhecida.
    pub field_types: HashMap<String, ValTy>,
    /// Nome textual cru do tipo de cada field (ex: `"Vec3"`, `"i32"`).
    /// Permite descobrir quando o field e instancia de outra classe
    /// registrada — habilita overload em `this.field + x`.
    pub field_class_names: HashMap<String, String>,
    /// (#nested-this) Tipos dos sub-fields quando o field eh assigned
    /// com object literal no constructor (`this.cfg = { server: {...} }`).
    /// Chave eh field_name (do this), valor eh map de sub-keys → ValTy.
    /// Permite resolver `this.cfg.server` → Handle (vs lixo de URL.host).
    pub field_obj_types: HashMap<String, HashMap<String, ValTy>>,
    /// (#nested-this) Tipos nested para fields — chave (field_name, sub_key),
    /// valor eh map de sub-sub-keys → ValTy. Permite `this.cfg.server.host`.
    pub field_nested_obj_types: HashMap<(String, String), HashMap<String, ValTy>>,
    /// Nomes de static methods (chamados via `C.method()`).
    pub static_methods: Vec<String>,
    /// Nomes de static fields (acessados via `C.field`).
    pub static_fields: Vec<String>,
    /// Nomes de getters definidos diretamente — ler `obj.x` chama
    /// __class_C_get_x(this).
    pub getters: Vec<String>,
    /// Nomes de setters definidos diretamente — `obj.x = v` chama
    /// __class_C_set_x(this, v).
    pub setters: Vec<String>,
    /// True quando a classe tem constructor proprio (mesmo se vazio).
    pub has_constructor: bool,
    /// Nomes de fields marcados `readonly` — só podem ser atribuídos
    /// dentro do constructor. Reassign em outros métodos é erro.
    pub readonly_fields: std::collections::HashSet<String>,
    /// Visibility declarada de cada membro (field ou método) que não
    /// é \`public\`. \`public\` é default e não é registrado pra economizar.
    /// Acessos a \`obj.x\` quando \`x\` é \`private\`/\`protected\` são
    /// validados em compile-time conforme regras TS.
    pub member_visibility: std::collections::HashMap<String, crate::parser::ast::Visibility>,
    /// `abstract class C` — não pode ser instanciada via `new C()`.
    pub is_abstract: bool,
    /// Nomes de métodos abstract declarados nesta classe (sem implementação).
    /// Subclasses concretas devem implementar todos os abstract methods
    /// herdados (incluindo de ancestrais).
    pub abstract_methods: std::collections::HashSet<String>,
    /// Layout nativo computado, quando a classe é elegível (todos os
    /// fields anotados, sem getters/setters). `None` para classes que
    /// continuam usando o caminho `Map`-based atual. Populado em segundo
    /// pass de `compile_program` apos coletar todas as ClassMeta.
    pub layout: Option<ClassLayout>,
}

/// Per-function compilation context.
///
/// `module` is stored as `&mut dyn Module` so the same codegen plumbing
/// serves both `ObjectModule` (AOT via `rts compile`) and `JITModule`
/// (in-memory via `rts run`). The Module trait is object-safe — every
/// method we need dispatches through the vtable without Self-bounds.
pub struct FnCtx<'m, 'fb> {
    pub builder: &'fb mut FunctionBuilder<'m>,
    pub module: &'m mut dyn Module,
    pub extern_cache: &'fb mut HashMap<String, cranelift_module::FuncId>,
    pub data_counter: &'fb mut u32,

    /// Stack of scopes. The first entry is the function scope (where `var`
    /// declarations live); subsequent entries are block scopes for `let`/`const`.
    pub locals: Vec<HashMap<String, LocalVar>>,
    /// (#155 fase 1) Handles owned por escopo, candidatos a string_free
    /// no pop_scope. Paralelo a `locals`. Cada entrada e' o nome da var
    /// + Variable Cranelift; ao popar, o codegen pode emitir
    /// `string_free(var)` se feature flag estiver ativa.
    pub owned_handles_per_scope: Vec<Vec<(String, Variable)>>,
    /// (#155 fase 1) Handles temporarios (sem nome de var) produzidos
    /// dentro do escopo corrente. Cada entry tem o Value + o Block onde
    /// foi criado, pra que pop_scope so' libere os que continuam
    /// dominando o ponto de free (mesmo block ou ancestor direto sem
    /// branches no meio).
    pub temp_handles_per_scope: Vec<Vec<(Value, Block)>>,
    /// Nomes que escapam do bloco corrente (return/atribuicao external/
    /// captura por closure). Populado pre-lower via escape analysis em
    /// `lower_block`. Vars listadas aqui nao sao auto-freed.
    pub escaped_in_block: Vec<std::collections::HashSet<String>>,
    /// Module-scope globals visible from functions.
    pub globals: &'fb HashMap<String, GlobalVar>,
    /// User-defined function signatures by source name.
    pub user_fns: &'fb HashMap<String, UserFnAbi>,
    /// Classe retornada por user functions cujo return_type bate com
    /// uma classe registrada. Permite `const x: V = makeV()` e
    /// `this.doubled() + this.doubled()` detectarem o tipo.
    pub fn_class_returns: &'fb HashMap<String, String>,
    /// Classes registradas no programa, indexadas pelo nome da classe.
    /// Permite resolver `new C(args)`, `super(args)` e `super.method(args)`
    /// em compile-time sem vtable.
    pub classes: &'fb HashMap<String, ClassMeta>,
    /// Tipo estatico declarado de cada local conhecido como instancia
    /// de classe — povoado quando a anotacao do bind e `: ClassName`.
    /// Permite dispatch estatico de `obj.method(...)`.
    pub local_class_ty: HashMap<String, String>,
    /// Quando o local e array tipado `: ClassName[]`, guarda o nome
    /// da classe dos elementos. Usado para inferir tipo de bind em
    /// for-of e em `arr[i]`.
    pub local_array_class_ty: HashMap<String, String>,

    /// (#592) Field types do elemento quando array contem object literals.
    /// \`const users = [{ name: \"Alice\" }]\` registra users -> {name: Handle}.
    /// Permite \`users[0].name\` e \`for (const u of users) u.name\` retornar
    /// tipo correto.
    pub local_array_obj_field_types: HashMap<String, HashMap<String, ValTy>>,
    /// Tipo dos campos de uma var que e object literal (ex: enum string,
    /// `const E = { Red: "red" }`). Permite que `E.Red` retorne Handle
    /// em vez de I64 anonimo.
    pub local_obj_field_types: HashMap<String, HashMap<String, ValTy>>,
    /// Tipo estatico de globais module-scope que sao instancias de
    /// classe. Populado uma vez em compile_program e compartilhado
    /// entre todos os FnCtx — permite dispatch de overload em funcoes
    /// top-level que referenciam `const a: V = new V(...)` global.
    pub global_class_ty: &'fb HashMap<String, String>,
    /// (#330) Tipos de fields de globais object-literal (ex: enum string
    /// \`enum E { A = \"a\" }\` desugara em \`const E = {A:\"a\"}\`. Sem isso,
    /// fn user fazendo \`E.A\` retornava I64 anonimo, e \`x == E.A\` comparava
    /// como int em vez de string.
    pub global_obj_field_types: &'fb HashMap<String, HashMap<String, ValTy>>,
    /// (#nested-chain) Tipos de fields aninhados de globais. Chave eh
    /// (root_global, key_no_root) — analogo a local_nested_obj_field_types.
    /// Permite `cfg.server.host` em user fn quando cfg eh global.
    pub global_nested_obj_field_types:
        &'fb HashMap<(String, String), HashMap<String, ValTy>>,
    /// Nome da classe atualmente sendo lowered (quando dentro de um
    /// metodo ou constructor). Usado para resolver `super`.
    pub current_class: Option<String>,
    /// True quando a função atual é um constructor de classe
    /// (`__class_C__init`). Usado pra permitir assign em readonly fields.
    pub current_is_ctor: bool,
    /// (#303) True quando \`super(...)\` ja foi chamado neste constructor.
    /// JS lanca ReferenceError no segundo super(). Reseta a cada
    /// constructor lowered.
    pub super_already_called: bool,
    /// True when lowering top-level statements in `main`.
    pub module_scope: bool,
    /// Declared return type of the surrounding function, used to coerce
    /// `return <expr>` to the correct Cranelift type.
    pub return_ty: Option<ValTy>,
    /// True while lowering the expression of a `return` statement — enables
    /// tail-call optimisation in `lower_user_call` when the surrounding
    /// function uses the Tail calling convention.
    pub in_tail_position: bool,
    /// True when the function being lowered uses `CallConv::Tail`. Only
    /// tail-conv callers can emit `return_call` to tail-conv callees.
    /// `__RTS_MAIN` stays on the platform default and keeps this false.
    pub is_tail_conv: bool,

    /// Cranelift variable counter.
    var_counter: u32,

    /// Stack of (break_block, continue_block, optional_label) for nested
    /// loops. Label permite \`break LABEL\` saltar para um loop externo
    /// específico em vez do mais interno.
    pub loop_stack: Vec<(Block, Block, Option<String>)>,
    /// Quando \`Stmt::Labeled\` envolve o próximo loop, registra aqui o
    /// nome do label. O loop seguinte consome (via \`take()\`) ao fazer
    /// push no \`loop_stack\`.
    pub pending_label: Option<String>,

    /// Warnings coletados durante o lower (#205 unreachable code, etc).
    /// Drenados por compile_program antes de retornar.
    pub warnings: Vec<String>,

    /// Maps local import name → codegen qualified name for `node:*` imports.
    /// Sourced from `Program::node_import_map`. See `nodespace::mod` for encoding.
    pub node_import_map: &'fb HashMap<String, String>,

    /// Maps local alias → original name for user-module imports
    /// (`import { add as plus } from "./lib"` → `"plus" -> "add"`).
    /// Codegen consults this before resolving identifiers; if a name is here,
    /// it dereferences to the underlying source name. Sourced from
    /// `Program::local_alias_map`.
    pub local_alias_map: &'fb HashMap<String, String>,

    /// Cache de DataId pra simbolos de data global declarados via
    /// declare_data. Evita declarar duas vezes o mesmo simbolo —
    /// cada declare_data cria um novo gv distinto que Cranelift nao
    /// deduplica, gerando \`global_value + load + store\` repetidos
    /// no body do loop.
    pub data_cache: HashMap<&'static str, cranelift_module::DataId>,
    /// Cache do GlobalValue resultante de declare_data_in_func na fn
    /// atual. Mesmo data_id chamado 2x em declare_data_in_func produz
    /// 2 gvs distintos — Cranelift nao deduplica, e em loops com varias
    /// calls do mesmo intrinsic isso emite \`global_value\` e \`load\`
    /// redundantes.
    pub gv_cache: HashMap<&'static str, cranelift_codegen::ir::GlobalValue>,
    /// Cache de GlobalValue por DataId — usado em read_local/write_local
    /// pra dedup quando o mesmo global e' lido/escrito multiplas vezes
    /// na mesma fn (caso tipico: contador top-level usado em hot loop).
    pub gv_data_cache:
        HashMap<cranelift_module::DataId, cranelift_codegen::ir::GlobalValue>,
    /// Cache de FuncRef por simbolo extern declarado na fn atual.
    /// Cada declare_func_in_func cria um FuncRef distinto mesmo pra
    /// mesma FuncId — em hot loops com varias calls do mesmo extern
    /// (ex: gc.string_ptr/string_len 4-6x por iter de console.log),
    /// isso gerava 4-6 \`call\` instrucoes que Cranelift nao deduplica.
    pub fn_ref_cache: HashMap<&'static str, cranelift_codegen::ir::FuncRef>,
    /// Cache de FuncRef por FuncId — usado em chamadas de user fns
    /// e qualquer site que ja' tem FuncId em mao (sem passar por
    /// fn_ref_cache que e' string-keyed). Em fns recursivas como
    /// \`fib\`, sem isso cada chamada (\`fib(n-1)\` e \`fib(n-2)\`)
    /// emitia FuncRef distintos pra mesma fn.
    pub fn_ref_by_id_cache:
        HashMap<cranelift_module::FuncId, cranelift_codegen::ir::FuncRef>,

    /// SSA Values que são handles freshly alocados (não variáveis nomeadas).
    /// Populado por emit_str_handle e coerce_to_handle. Removido quando
    /// o valor é atribuído a uma variável nomeada via declare_local_kind.
    /// Usado por lower_add para liberar operandos temporários após concat.
    pub fresh_handle_set: std::collections::HashSet<cranelift_codegen::ir::Value>,

    /// (#432) SSA Values que sao resultado de optional chain (`?.`) e devem
    /// ser tratados como `undefined` quando 0. Lido por lower_tpl para
    /// emitir "undefined" em vez de "0" no template literal.
    pub optional_chain_values: std::collections::HashSet<cranelift_codegen::ir::Value>,

    /// (#proto-method) SSA Values que sao resultado de var_member_call
    /// (instance.method() em handle generico). Em template literal,
    /// tentar interpretar como Handle se valor parece handle valido
    /// (consulta entry table via runtime fn).
    pub var_member_call_values: std::collections::HashSet<cranelift_codegen::ir::Value>,

    /// (cross-runtime #45/#94) SSA Values vindos de `arr[i]` em Vec/Array.
    /// Em template literal, value=0 representa numero literal `0`, nao null.
    /// Roteia para TPL_COERCE_VEC_SLOT em vez de TPL_COERCE_AUTO.
    pub var_vec_slot_values: std::collections::HashSet<cranelift_codegen::ir::Value>,

    /// (#627) Var names cujo init veio de obj.x ambiguo (member access
    /// sem tipo declarado). Cada read_local da var marca o Value carregado
    /// como var_member_call_values, propagando a flag de ambiguidade
    /// atraves da var. Sem isso, destructuring de param de fn perde info.
    pub local_ambiguous_vars: std::collections::HashSet<String>,

    /// (372) Vars declaradas com tipo de funcao que retorna `number`
    /// (`f: () => number`). Quando chamadas via invoke, o resultado i64
    /// carrega os BITS de um f64 (ex: arrow `() => this.campoF64`) e deve ser
    /// reinterpretado via bitcast, nao tratado como inteiro.
    pub local_fn_ret_f64: std::collections::HashSet<String>,

    /// Dedup de data sections por conteúdo: bytes → DataId já declarado.
    /// Evita criar múltiplos .Lrts_str_N com bytes idênticos quando o mesmo
    /// literal aparece mais de uma vez (dentro ou entre funções do módulo).
    pub str_data_cache: HashMap<Vec<u8>, cranelift_module::DataId>,

    /// Dedup de GC handles por conteúdo: (bytes, block) → Value emitido neste bloco.
    /// Cache com chave de bloco evita reutilizar Values de blocos não-dominadores
    /// (que causam stack slots não-inicializados em ternários encadeados).
    pub str_handle_cache: HashMap<(Vec<u8>, cranelift_codegen::ir::Block), cranelift_codegen::ir::Value>,

    /// Dedup de SSA Values para constantes numéricas por bloco.
    /// Keyed por (bits, is_float, block) — per-block como str_handle_cache,
    /// para evitar usar Values de blocos não-dominadores.
    pub num_val_cache: HashMap<(u64, u8, cranelift_codegen::ir::Block), cranelift_codegen::ir::Value>,

    /// (#210) Para nested object literals: armazena tipos dos campos
    /// dos sub-objetos. Chave: (var_name, field_name) → tipos do
    /// sub-objeto. Permite que `const { db: { host } } = cfg` (apos
    /// desugaring) consiga inferir tipo de `host` quando cfg foi
    /// declarado como `const cfg = { db: { host: "x" } }`.
    pub local_nested_obj_field_types:
        HashMap<(String, String), HashMap<String, ValTy>>,

    /// (#208) Vars declaradas como `T[]` / `Array<T>` (qualquer T).
    /// Usado em `lower_var_member_call` para preferir `lower_array_builtin`
    /// antes de `lower_string_builtin` quando o tipo estatico indica array.
    /// Sem isso, `arr.indexOf(x)` em `let arr: number[] = [...]` cai em
    /// string builtin (`__RTS_FN_GL_STRING_INDEX_OF`) e retorna lixo.
    pub local_array_vars: std::collections::HashSet<String>,

    /// (generators) Vars inicializadas com chamada a uma generator fn
    /// (`const it = g()`). `it.next()` roteia para GENERATOR_NEXT (cursor
    /// lateral sobre o Vec); outras vars com `.next()` usam dispatch normal.
    pub generator_vars: std::collections::HashSet<String>,

    /// Counter para gerar nomes únicos de locals temporários em
    /// optional chain calls aninhadas (#481). Usado por
    /// `lower_opt_chain` quando o receiver é uma expressão complexa
    /// que não casa com Member.obj=Ident no path coberto de lower_call.
    pub opt_chain_temp_counter: u32,

    /// Nome da função atual (para stack traces em throw). Vazio em __RTS_MAIN.
    pub current_fn_name: String,
    /// Arquivo fonte atual (para stack traces em throw).
    pub current_file: String,
    /// True quando codegen emitiu push_frame no entry — pop_frame deve ser
    /// emitido antes de cada `return_` nesta função.
    pub trace_instrumented: bool,
}

impl<'m, 'fb> FnCtx<'m, 'fb> {
    pub fn new(
        builder: &'fb mut FunctionBuilder<'m>,
        module: &'m mut dyn Module,
        extern_cache: &'fb mut HashMap<String, cranelift_module::FuncId>,
        data_counter: &'fb mut u32,
        globals: &'fb HashMap<String, GlobalVar>,
        user_fns: &'fb HashMap<String, UserFnAbi>,
        classes: &'fb HashMap<String, ClassMeta>,
        global_class_ty: &'fb HashMap<String, String>,
        global_obj_field_types: &'fb HashMap<String, HashMap<String, ValTy>>,
        global_nested_obj_field_types: &'fb HashMap<
            (String, String),
            HashMap<String, ValTy>,
        >,
        fn_class_returns: &'fb HashMap<String, String>,
        node_import_map: &'fb HashMap<String, String>,
        local_alias_map: &'fb HashMap<String, String>,
        module_scope: bool,
    ) -> Self {
        Self {
            builder,
            module,
            extern_cache,
            data_counter,
            locals: vec![HashMap::new()],
            owned_handles_per_scope: vec![Vec::new()],
            temp_handles_per_scope: vec![Vec::new()],
            escaped_in_block: vec![std::collections::HashSet::new()],
            globals,
            user_fns,
            fn_class_returns,
            classes,
            local_class_ty: HashMap::new(),
            local_array_class_ty: HashMap::new(),
            local_array_obj_field_types: HashMap::new(),
            local_obj_field_types: HashMap::new(),
            global_class_ty,
            global_obj_field_types,
            global_nested_obj_field_types,
            current_class: None,
            current_is_ctor: false,
            super_already_called: false,
            module_scope,
            return_ty: None,
            in_tail_position: false,
            is_tail_conv: false,
            var_counter: 0,
            loop_stack: Vec::new(),
            pending_label: None,
            warnings: Vec::new(),
            node_import_map,
            local_alias_map,
            data_cache: HashMap::new(),
            gv_cache: HashMap::new(),
            gv_data_cache: HashMap::new(),
            fn_ref_cache: HashMap::new(),
            fn_ref_by_id_cache: HashMap::new(),
            fresh_handle_set: std::collections::HashSet::new(),
            optional_chain_values: std::collections::HashSet::new(),
            var_member_call_values: std::collections::HashSet::new(),
            var_vec_slot_values: std::collections::HashSet::new(),
            local_ambiguous_vars: std::collections::HashSet::new(),
            local_fn_ret_f64: std::collections::HashSet::new(),
            str_data_cache: HashMap::new(),
            str_handle_cache: HashMap::new(),
            num_val_cache: HashMap::new(),
            local_array_vars: std::collections::HashSet::new(),
            generator_vars: std::collections::HashSet::new(),
            local_nested_obj_field_types: HashMap::new(),
            opt_chain_temp_counter: 0,
            current_fn_name: String::new(),
            current_file: String::new(),
            trace_instrumented: false,
        }
    }

    /// Próximo id único pra nome temporário em optional chain (#481).
    pub fn next_opt_chain_temp_id(&mut self) -> u32 {
        let id = self.opt_chain_temp_counter;
        self.opt_chain_temp_counter += 1;
        id
    }

    /// Resolve `fn_id` a um FuncRef na fn atual, cacheando.
    /// Cada declare_func_in_func cria FuncRef distinto pro mesmo
    /// FuncId — em recursao ou multiplas chamadas, isso fragmentava
    /// o IR sem necessidade.
    pub fn fref_for_id(
        &mut self,
        fn_id: cranelift_module::FuncId,
    ) -> cranelift_codegen::ir::FuncRef {
        if let Some(f) = self.fn_ref_by_id_cache.get(&fn_id).copied() {
            return f;
        }
        let f = self.module.declare_func_in_func(fn_id, self.builder.func);
        self.fn_ref_by_id_cache.insert(fn_id, f);
        f
    }

    /// Resolve `data_id` a um GlobalValue na fn atual, cacheando o
    /// resultado. Sem o cache, leituras/escritas repetidas do mesmo
    /// global em hot loop emitiam `global_value` distintos.
    fn gv_for_data(&mut self, data_id: cranelift_module::DataId) -> cranelift_codegen::ir::GlobalValue {
        if let Some(g) = self.gv_data_cache.get(&data_id).copied() {
            return g;
        }
        let g = self.module.declare_data_in_func(data_id, self.builder.func);
        self.gv_data_cache.insert(data_id, g);
        g
    }

    /// Allocates a new Cranelift variable slot.
    pub fn new_var(&mut self, ty: ValTy) -> Variable {
        self.var_counter += 1;
        self.builder.declare_var(ty.cl_type())
    }

    /// Allocates an explicit stack slot of `size` bytes with the given
    /// log2 alignment. Returns the slot handle and a pointer to its base.
    ///
    /// Use when a value needs a stable address — scenarios that
    /// [`Variable`] (pure SSA) cannot model:
    ///
    /// - **Mutable captures in closures** (#97 — `() => x++` needs `&mut x`)
    /// - **Aggregate return values** (tuple/struct returns, future)
    /// - **Fixed-size local arrays** (`const buf = [0, 0, 0]`)
    ///
    /// Loads and stores through the returned pointer should use
    /// [`MemFlags::trusted()`] — stack slots are always aligned and
    /// addressable. Prefer [`new_var`](Self::new_var) for plain scalars
    /// that do not need an address: `Variable` is cheaper and lowers to
    /// a single Cranelift value without going through memory.
    ///
    /// Until #97 lands there is no consumer inside this crate; the helper
    /// is kept `pub` so the closure lowering work has a small surface to
    /// plug into.
    pub fn alloc_stack_slot(&mut self, size: u32, align_log2: u8) -> (StackSlot, Value) {
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            align_log2,
        ));
        let ptr_ty = self.module.isa().pointer_type();
        let addr = self.builder.ins().stack_addr(ptr_ty, slot, 0);
        (slot, addr)
    }

    /// Pushes a new block scope. Variables declared with `let`/`const` go here.
    pub fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
        self.owned_handles_per_scope.push(Vec::new());
        self.temp_handles_per_scope.push(Vec::new());
        self.escaped_in_block.push(std::collections::HashSet::new());
    }

    /// Pops the current block scope. Antes de descartar, emite
    /// `string_free` para handles owned que nao escaparam (#155 fase 1).
    /// So' age se feature flag `RTS_AUTO_FREE_HANDLES=1` estiver ativa
    /// e o IR atual ainda for alcancavel (nao-terminator).
    pub fn pop_scope(&mut self) {
        if self.locals.len() > 1 {
            self.try_emit_scope_frees();
            self.locals.pop();
            self.owned_handles_per_scope.pop();
            self.temp_handles_per_scope.pop();
            self.escaped_in_block.pop();
        }
    }

    fn try_emit_scope_frees(&mut self) {
        // Feature flag: opt-in via env var enquanto a analise de escape
        // amadurece. Off por padrao para evitar regressoes.
        if std::env::var("RTS_AUTO_FREE_HANDLES").ok().as_deref() != Some("1") {
            return;
        }
        if self.builder.is_unreachable() {
            return;
        }
        // Cranelift verifier rejeita instrucoes apos terminator no
        // mesmo block. Se o block atual ja' fechou com return/jump/
        // brif, nao da' pra adicionar mais calls.
        if let Some(b) = self.builder.current_block() {
            if let Some(last) = self.builder.func.layout.last_inst(b) {
                use cranelift_codegen::ir::InstructionData;
                let dfg = &self.builder.func.dfg;
                if matches!(
                    dfg.insts[last],
                    InstructionData::Jump { .. }
                        | InstructionData::Brif { .. }
                        | InstructionData::BranchTable { .. }
                        | InstructionData::MultiAry { .. } // return / return_call
                        | InstructionData::Trap { .. }
                ) {
                    return;
                }
            }
        }
        let owned = self.owned_handles_per_scope.last().cloned().unwrap_or_default();
        let temps = self.temp_handles_per_scope.last().cloned().unwrap_or_default();
        if owned.is_empty() && temps.is_empty() {
            return;
        }
        let escaped = self.escaped_in_block.last().cloned().unwrap_or_default();
        // Sentinel "*" significa que houve closure no bloco — todas
        // as vars potencialmente escaparam, nao libera nada.
        if escaped.contains("*") {
            return;
        }
        let free_fn = match self.get_extern(
            "__RTS_FN_NS_GC_STRING_FREE",
            &[cl::I64],
            Some(cl::I64),
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        for (name, var) in owned {
            if escaped.contains(&name) {
                continue;
            }
            let h = self.builder.use_var(var);
            self.builder.ins().call(free_fn, &[h]);
        }
        // Temp so' eh seguro liberar quando o block onde foi criado
        // domina o ponto atual. Heuristica simples: so' libera temps
        // cujo block bate com o block corrente (sem branches no meio).
        let cur_block = self.builder.current_block();
        for (h, block) in temps {
            if Some(block) == cur_block {
                self.builder.ins().call(free_fn, &[h]);
            }
        }
    }

    /// Registra um handle temporario (sem nome) produzido no escopo
    /// corrente para auto-free no pop_scope. Chamadores: emit_str_handle
    /// (string_from_static), coerce_to_handle (string_from_i64/f64),
    /// concat de strings.
    pub fn register_temp_handle(&mut self, h: Value) {
        if std::env::var("RTS_AUTO_FREE_HANDLES").ok().as_deref() != Some("1") {
            return;
        }
        if self.locals.len() <= 1 {
            return; // function scope — let main lifetime cuidar.
        }
        let cur_block = match self.builder.current_block() {
            Some(b) => b,
            None => return,
        };
        if let Some(top) = self.temp_handles_per_scope.last_mut() {
            top.push((h, cur_block));
        }
    }

    /// Declares a named local in the current (top-most) scope.
    pub fn declare_local(&mut self, name: &str, ty: ValTy, init: Value) {
        self.declare_local_kind(name, ty, init, false, false);
    }

    /// Declares a named local with control over mutability and scope target.
    ///
    /// `function_scope = true` places the binding in the function-level scope
    /// (used for `var`); otherwise it lands in the current block scope.
    pub fn declare_local_kind(
        &mut self,
        name: &str,
        ty: ValTy,
        init: Value,
        is_const: bool,
        function_scope: bool,
    ) {
        let var = self.new_var(ty);
        // Bool agora pode ser i8 (resultado direto de icmp). Variable
        // de Bool tem cl_type i64 — precisa uextend antes de def_var.
        let init = self.normalize_to_var_ty(init, ty);
        self.builder.def_var(var, init);
        let slot = LocalVar { var, ty, is_const };
        let idx = if function_scope {
            0
        } else {
            self.locals.len() - 1
        };
        self.locals[idx].insert(name.to_string(), slot);
        // Handle atribuído a variável nomeada — não é mais "fresh" (pode ser referenciado
        // por nome; lower_add não deve liberar automaticamente).
        if matches!(ty, ValTy::Handle) {
            self.fresh_handle_set.remove(&init);
            // Declarar a Variable para stack-map tracking — propaga para todos os
            // SSA values da var, incluindo phi nodes em loop headers.
            self.declare_gc_var(var);
        }
        // (#155 fase 1) Registra handles `const` em escopo de bloco
        // como candidatos a string_free no pop_scope. So' const Handle
        // em block scope (nao function/var scope).
        if is_const && matches!(ty, ValTy::Handle) && !function_scope && self.locals.len() > 1 {
            self.owned_handles_per_scope[idx].push((name.to_string(), var));
            // O `init` do declare ja' foi registrado como temp no
            // emit_str_handle/coerce_to_handle/concat. Remove esse
            // temp pra evitar double-free (var ja' vai liberar).
            if let Some(top) = self.temp_handles_per_scope.last_mut() {
                if top.last().map(|(v, _)| *v) == Some(init) {
                    top.pop();
                }
            }
        }
    }

    /// Garante que `val` tem o cl_type esperado pela Variable de tipo `ty`.
    /// Hoje so' Bool precisa: pode chegar como i8 (icmp result) e variable
    /// e' i64 — uextend implicito.
    fn normalize_to_var_ty(&mut self, val: Value, ty: ValTy) -> Value {
        let v_ty = self.builder.func.dfg.value_type(val);
        let expected = ty.cl_type();
        if v_ty == expected {
            // Caso comum: tipos batem (incl. I64/Handle/U64 todos cl::I64).
            return val;
        }
        if matches!(ty, ValTy::Bool) && v_ty == cl::I8 {
            return self.builder.ins().uextend(cl::I64, val);
        }
        // (cross-runtime #372) Var declarada F64 mas RHS eh int (ex: literal
        // `b = 20` em `let b: f64`). Converte via fcvt.
        if expected == cl::F64 && v_ty == cl::I64 {
            return self.builder.ins().fcvt_from_sint(cl::F64, val);
        }
        if expected == cl::F64 && v_ty == cl::I32 {
            let as_i64 = self.builder.ins().sextend(cl::I64, val);
            return self.builder.ins().fcvt_from_sint(cl::F64, as_i64);
        }
        // F64 -> I64 (truncate): caso reverso, atribuicao de F64 a var int.
        if expected == cl::I64 && v_ty == cl::F64 {
            return self.builder.ins().fcvt_to_sint_sat(cl::I64, val);
        }
        if expected == cl::I32 && v_ty == cl::I64 {
            return self.builder.ins().ireduce(cl::I32, val);
        }
        if expected == cl::I64 && v_ty == cl::I32 {
            return self.builder.ins().sextend(cl::I64, val);
        }
        val
    }

    /// Olha o LocalVar de uma var sem emitir IR — caller pode decidir
    /// se vai usar (ex: branchless if-to-select).
    pub fn read_local_info(&self, name: &str) -> Option<LocalVar> {
        self.find_local(name)
    }

    /// Resolve um identificador atraves do alias map de imports user-module.
    /// `import { x as y } from "./m"` registra `y -> x`. Quando codegen
    /// procura `y` em algum lookup top-level (user_fns/globals/classes),
    /// esta funcao retorna `x` se nao houver local `y` mascarando o alias.
    /// Em scope plano (locals first), o local sempre vence.
    pub fn resolve_alias<'a>(&'a self, name: &'a str) -> &'a str {
        if self.find_local(name).is_some() {
            return name;
        }
        match self.local_alias_map.get(name) {
            Some(orig) => orig.as_str(),
            None => name,
        }
    }

    fn find_local(&self, name: &str) -> Option<LocalVar> {
        for scope in self.locals.iter().rev() {
            if let Some(slot) = scope.get(name) {
                return Some(slot.clone());
            }
        }
        None
    }

    /// Reads a named local or module global.
    pub fn read_local(&mut self, name: &str) -> Option<TypedVal> {
        if let Some(local) = self.find_local(name) {
            let val = self.builder.use_var(local.var);
            return Some(TypedVal::new(val, local.ty));
        }

        let global = self.globals.get(name).cloned()?;
        let gv = self.gv_for_data(global.data_id);
        let ptr = self
            .builder
            .ins()
            .global_value(self.module.isa().pointer_type(), gv);
        // Module globals are declared with natural alignment (4/8 bytes)
        // and always initialised before first read (top-level emits the
        // initialiser before any reader). `trusted()` = aligned + notrap
        // lets the backend pick the widest load instruction.
        let val = self
            .builder
            .ins()
            .load(global.ty.cl_type(), MemFlags::trusted(), ptr, 0);
        Some(TypedVal::new(val, global.ty))
    }

    /// Writes to a named local or module global.
    pub fn write_local(&mut self, name: &str, val: Value) -> Result<()> {
        if let Some(local) = self.find_local(name) {
            if local.is_const {
                return Err(anyhow!("assignment to const variable `{name}`"));
            }
            let val = self.normalize_to_var_ty(val, local.ty);
            self.builder.def_var(local.var, val);
            return Ok(());
        }

        if let Some(global) = self.globals.get(name).cloned() {
            let gv = self.gv_for_data(global.data_id);
            let ptr = self
                .builder
                .ins()
                .global_value(self.module.isa().pointer_type(), gv);
            let casted = self.coerce_value_to_ty(val, global.ty);
            self.builder
                .ins()
                .store(MemFlags::trusted(), casted, ptr, 0);
            return Ok(());
        }

        Err(anyhow!("assignment to undeclared variable `{name}`"))
    }

    /// Returns the declared type of a local/global variable.
    pub fn var_ty(&self, name: &str) -> Option<ValTy> {
        self.find_local(name)
            .map(|local| local.ty)
            .or_else(|| self.globals.get(name).map(|global| global.ty))
    }

    /// Returns true if `name` is a module global.
    pub fn has_global(&self, name: &str) -> bool {
        self.globals.contains_key(name)
    }

    fn coerce_value_to_ty(&mut self, value: Value, ty: ValTy) -> Value {
        let expected = ty.cl_type();
        let actual = self.builder.func.dfg.value_type(value);
        if actual == expected {
            return value;
        }

        match (actual, expected) {
            (cl::I32, cl::I64) => self.builder.ins().sextend(cl::I64, value),
            (cl::I64, cl::I32) => self.builder.ins().ireduce(cl::I32, value),
            (cl::I32, cl::F64) => self.builder.ins().fcvt_from_sint(cl::F64, value),
            (cl::I64, cl::F64) => self.builder.ins().fcvt_from_sint(cl::F64, value),
            (cl::F64, cl::I64) => self.builder.ins().fcvt_to_sint_sat(cl::I64, value),
            (cl::F64, cl::I32) => {
                let as_i64 = self.builder.ins().fcvt_to_sint_sat(cl::I64, value);
                self.builder.ins().ireduce(cl::I32, as_i64)
            }
            _ => value,
        }
    }

    /// Ensures an extern symbol is declared and returns a FuncRef.
    pub fn get_extern(
        &mut self,
        symbol: &'static str,
        params: &[cranelift_codegen::ir::Type],
        ret: Option<cranelift_codegen::ir::Type>,
    ) -> Result<FuncRef> {
        // Cache duplo: FuncId e' por modulo (compartilhado entre fns),
        // FuncRef e' por funcao em compilacao. Cranelift nao deduplica
        // FuncRefs distintos pro mesmo FuncId no mesmo modulo, entao
        // sem o segundo cache hot loops emitiam varios \`fn3 = ...\`
        // distintos pra mesma signature/symbol.
        if let Some(fref) = self.fn_ref_cache.get(symbol).copied() {
            return Ok(fref);
        }
        if !self.extern_cache.contains_key(symbol) {
            use cranelift_codegen::ir::{AbiParam, Signature};
            use cranelift_module::Linkage;

            let mut sig = Signature::new(self.module.isa().default_call_conv());
            for &p in params {
                sig.params.push(AbiParam::new(p));
            }
            if let Some(r) = ret {
                sig.returns.push(AbiParam::new(r));
            }
            let id = self
                .module
                .declare_function(symbol, Linkage::Import, &sig)
                .map_err(|e| anyhow!("failed to declare extern {symbol}: {e}"))?;
            self.extern_cache.insert(symbol.to_string(), id);
        }
        let id = *self.extern_cache.get(symbol).expect("extern declared");
        let fref = self.module.declare_func_in_func(id, self.builder.func);
        self.fn_ref_cache.insert(symbol, fref);
        Ok(fref)
    }

    /// Variant of [`get_extern`] that accepts `AbiType` slices instead of
    /// Cranelift `Type`s. Converts using `scalar_to_cl` from `abi::signature`.
    pub fn get_extern_abi(
        &mut self,
        symbol: &'static str,
        params: &[AbiType],
        ret: Option<AbiType>,
    ) -> Result<cranelift_codegen::ir::FuncRef> {
        use crate::abi::signature::scalar_to_cl;
        let cl_params: Vec<_> = params.iter().copied().map(scalar_to_cl).collect();
        let cl_ret = ret.map(scalar_to_cl);
        self.get_extern(symbol, &cl_params, cl_ret)
    }

    /// Emits a rodata string literal and returns (ptr: i64, len: i64).
    pub fn emit_str_literal(&mut self, bytes: &[u8]) -> Result<(Value, Value)> {
        use cranelift_module::{DataDescription, Linkage};

        // Reuse existing data section for identical byte content.
        let data_id = if let Some(&cached) = self.str_data_cache.get(bytes) {
            cached
        } else {
            let name = format!(".Lrts_str_{}", self.data_counter);
            *self.data_counter += 1;

            let id = self
                .module
                .declare_data(&name, Linkage::Local, false, false)
                .map_err(|e| anyhow!("failed to declare data {name}: {e}"))?;

            let mut desc = DataDescription::new();
            desc.define(bytes.to_vec().into_boxed_slice());
            self.module
                .define_data(id, &desc)
                .map_err(|e| anyhow!("failed to define data {name}: {e}"))?;

            self.str_data_cache.insert(bytes.to_vec(), id);
            id
        };

        let gv = self.module.declare_data_in_func(data_id, self.builder.func);
        let ptr_ty = self.module.isa().pointer_type();
        let ptr = self.builder.ins().global_value(ptr_ty, gv);
        let ptr = if ptr_ty == cl::I64 {
            ptr
        } else {
            self.builder.ins().uextend(cl::I64, ptr)
        };
        let len = self.builder.ins().iconst(cl::I64, bytes.len() as i64);
        Ok((ptr, len))
    }

    /// Emits a call to `__RTS_FN_NS_TRACE_PUSH_FRAME` with current fn/file/line/col.
    /// Sets `self.trace_instrumented = true` so pop is emitted before every return.
    pub fn emit_trace_push(&mut self, line: u32, col: u32) -> Result<()> {
        let file_bytes = self.current_file.clone().into_bytes();
        let fn_bytes = self.current_fn_name.clone().into_bytes();
        let (file_ptr, file_len) = self.emit_str_literal(&file_bytes)?;
        let (fn_ptr, fn_len) = self.emit_str_literal(&fn_bytes)?;
        let line_v = self.builder.ins().iconst(cl::I64, line as i64);
        let col_v = self.builder.ins().iconst(cl::I64, col as i64);
        let push = self.get_extern(
            "__RTS_FN_NS_TRACE_PUSH_FRAME",
            &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
            None,
        )?;
        self.builder.ins().call(push, &[file_ptr, file_len, fn_ptr, fn_len, line_v, col_v]);
        self.trace_instrumented = true;
        Ok(())
    }

    /// Emits a call to `__RTS_FN_NS_TRACE_POP_FRAME`. No-op if not instrumented.
    pub fn emit_trace_pop(&mut self) -> Result<()> {
        if !self.trace_instrumented {
            return Ok(());
        }
        let pop = self.get_extern("__RTS_FN_NS_TRACE_POP_FRAME", &[], None)?;
        self.builder.ins().call(pop, &[]);
        Ok(())
    }

    /// Promotes a static string literal to a GC handle.
    /// Declare `val` (a GC handle) to Cranelift's stack-map tracking.
    /// Cranelift will spill this value at every non-tail-call safepoint where
    /// it is live, and record it in the `UserStackMap` for that call.
    /// The GC collector reads these maps during `finish_cycle()` to find roots.
    ///
    /// Prefer `declare_gc_var` when the handle will be stored in a Variable —
    /// Variables span loop iterations (phi nodes) while Values are single SSA defs.
    pub fn declare_gc_handle(&mut self, val: Value) {
        self.builder.declare_value_needs_stack_map(val);
    }

    /// Declare a Variable (SSA variable holding a GC handle) for stack-map tracking.
    /// Unlike `declare_gc_handle`, this propagates to ALL SSA values associated
    /// with the variable, including phi nodes created at loop headers. Use this
    /// for any `Variable` whose current value is a live GC handle.
    pub fn declare_gc_var(&mut self, var: cranelift_frontend::Variable) {
        self.builder.declare_var_needs_stack_map(var);
    }

    pub fn emit_str_handle(&mut self, bytes: &[u8]) -> Result<TypedVal> {
        // Dedup per-block: reuse SSA Value only within the same basic block.
        // Cross-block reuse of raw Values causes Cranelift to spill them into
        // stack slots before GC calls, but if the defining block wasn't executed
        // the slot is uninitialized — breaking ternary chains (issue #359).
        // (#1024) Não cachear o handle: o str_handle_cache reusava o mesmo
        // SSA Value para múltiplos usos da mesma string literal no bloco,
        // mas o handle pode ter sido movido para dentro de Vec/Map cujo
        // dono libera a string antes do uso seguinte (ex: arr.map callback
        // consome). Cada uso lexicográfico precisa alocar handle próprio.
        let (ptr, len) = self.emit_str_literal(bytes)?;
        let fref = self.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = self.builder.ins().call(fref, &[ptr, len]);
        let val = self.builder.inst_results(inst)[0];
        self.declare_gc_handle(val);
        self.fresh_handle_set.insert(val);
        self.register_temp_handle(val);
        Ok(TypedVal::new(val, ValTy::Handle))
    }

    /// Passa um valor adiante como Handle SEM stringificar.
    ///
    /// `coerce_to_handle` significa "produza uma string GC" — apropriado em
    /// contexto de template/concat, mas destrutivo quando o valor ja eh um
    /// handle de objeto/array/Map que deve ser propagado cru (ex: `return
    /// (globalThis as any)[key]`, `const x: any = obj; return x`). Nestes
    /// casos um valor i64/U64/Handle ja carrega os bits do handle e so'
    /// precisa ser reinterpretado como Handle. F64/Bool caem no caminho de
    /// coercao normal (sao genuinamente escalares e viram string).
    pub fn pass_as_handle(&mut self, tv: TypedVal) -> Result<TypedVal> {
        match tv.ty {
            ValTy::Handle => Ok(tv),
            ValTy::U64 | ValTy::I64 | ValTy::I32
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
                let v = self.coerce_to_i64(tv).val;
                Ok(TypedVal::new(v, ValTy::Handle))
            }
            // F64/Bool: nao sao handles — delega pra coercao que stringifica.
            _ => self.coerce_to_handle(tv),
        }
    }

    /// Coerces any value to a GC string handle.
    pub fn coerce_to_handle(&mut self, tv: TypedVal) -> Result<TypedVal> {
        match tv.ty {
            ValTy::Handle => Ok(tv),
            ValTy::U64 => Ok(TypedVal::new(tv.val, ValTy::Handle)),
            ValTy::Bool => {
                // Bool em string vira "true"/"false" (semantica JS), nao
                // "1"/"0". Sem isso `${x instanceof C}` saia como `1`.
                //
                // Usa __RTS_FN_GL_BOOLEAN_TO_STRING para evitar bug de
                // ordering de free: quando os 2 handles "true"/"false" eram
                // alocados separadamente e selecionados, o handle nao
                // selecionado ficava em str_handle_cache para reuso na
                // proxima expressao bool, mas o selecionado era freed via
                // register_temp_handle. Em segunda print, select pegava
                // handle ja freed -> string vazia. (#XXX)
                let cond = self.coerce_to_i64(tv).val;
                let fref = self.get_extern(
                    "__RTS_FN_GL_BOOLEAN_TO_STRING",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = self.builder.ins().call(fref, &[cond]);
                let val = self.builder.inst_results(inst)[0];
                self.declare_gc_handle(val);
                self.fresh_handle_set.insert(val);
                self.register_temp_handle(val);
                Ok(TypedVal::new(val, ValTy::Handle))
            }
            ValTy::I64 | ValTy::I32
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
                let as_i64 = self.coerce_to_i64(tv);
                // Se o valor e' ambiguo (pode ser handle ou i64), usa TPL_COERCE_AUTO
                // para decidir em runtime (evita formatar handle como numero decimal).
                if self.var_vec_slot_values.contains(&as_i64.val) {
                    let fref = self.get_extern("__RTS_FN_RT_TPL_COERCE_VEC_SLOT", &[cl::I64], Some(cl::I64))?;
                    let inst = self.builder.ins().call(fref, &[as_i64.val]);
                    let val = self.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(val, ValTy::Handle));
                }
                if self.var_member_call_values.contains(&as_i64.val) {
                    let fref = self.get_extern("__RTS_FN_RT_TPL_COERCE_AUTO", &[cl::I64], Some(cl::I64))?;
                    let inst = self.builder.ins().call(fref, &[as_i64.val]);
                    let val = self.builder.inst_results(inst)[0];
                    return Ok(TypedVal::new(val, ValTy::Handle));
                }
                // (cross-runtime) STRING_FROM_I64_TPL filtra sentinelas JS
                // (MIN..MIN+4) para gerar "false"/"true"/"undefined"/"null"
                // em template literal contexts. gc.string_from_i64 puro
                // (API direta) nao filtra.
                let fref =
                    self.get_extern("__RTS_FN_NS_GC_STRING_FROM_I64_TPL", &[cl::I64], Some(cl::I64))?;
                let inst = self.builder.ins().call(fref, &[as_i64.val]);
                let val = self.builder.inst_results(inst)[0];
                self.declare_gc_handle(val);
                self.fresh_handle_set.insert(val);
                self.register_temp_handle(val);
                Ok(TypedVal::new(val, ValTy::Handle))
            }
            ValTy::F64 => {
                let fref =
                    self.get_extern("__RTS_FN_NS_GC_STRING_FROM_F64", &[cl::F64], Some(cl::I64))?;
                let inst = self.builder.ins().call(fref, &[tv.val]);
                let val = self.builder.inst_results(inst)[0];
                self.declare_gc_handle(val);
                self.fresh_handle_set.insert(val);
                self.register_temp_handle(val);
                Ok(TypedVal::new(val, ValTy::Handle))
            }
        }
    }

    /// Reduz um valor a uma condicao branch-ready (i8 ou i64 != 0).
    /// Quando o valor ja eh resultado de icmp (i.e., um Bool i64 produzido
    /// por lower_icmp), evita re-comparar contra zero — passa direto.
    /// Reduz emissao de \`uextend + iconst 0 + icmp ne\` em loops/ifs.
    pub fn to_branch_cond(
        &mut self,
        tv: TypedVal,
    ) -> cranelift_codegen::ir::Value {
        match tv.ty {
            // Bool ja tem semantica de "branch on non-zero" — Cranelift
            // brif aceita qualquer valor inteiro != 0.
            ValTy::Bool => tv.val,
            _ => {
                let i64v = self.coerce_to_i64(tv).val;
                let zero = self.builder.ins().iconst(cl::I64, 0);
                // (cross-runtime #1133) Sentinel undefined (i64::MIN+2) deve
                // ser falsy em branch context. Sem isso `while (cur?.next)`
                // / `if (x?.missing)` sao sempre truthy quando o lado direito
                // eh undefined. JS spec: undefined eh falsy.
                let undef = self.builder.ins().iconst(cl::I64, i64::MIN + 2);
                let not_zero = self.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    i64v,
                    zero,
                );
                let not_undef = self.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    i64v,
                    undef,
                );
                self.builder.ins().band(not_zero, not_undef)
            }
        }
    }

    /// Coerces a value to I64.
    pub fn coerce_to_i64(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::I64 | ValTy::Handle | ValTy::U64 => tv,
            // Narrow ints viajam em I64 no ABI; mask de overflow eh
            // responsabilidade do MIR (`narrow` pass).
            ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
                TypedVal::new(tv.val, ValTy::I64)
            }
            ValTy::Bool => {
                // Bool e' i8 nativo Cranelift (resultado de icmp). Quando
                // precisar i64 (ex: `const flag = (x < y)`), faz uextend.
                // Se ja estiver em i64 (vindo de outro lugar — ex: literal
                // boolean), passa direto. cl_type(Bool) retorna i64 entao
                // o tipo do Value pode ser qualquer um — checamos em runtime.
                let v_ty = self.builder.func.dfg.value_type(tv.val);
                if v_ty == cl::I8 {
                    TypedVal::new(self.builder.ins().uextend(cl::I64, tv.val), ValTy::I64)
                } else {
                    tv
                }
            }
            ValTy::I32 => TypedVal::new(self.builder.ins().sextend(cl::I64, tv.val), ValTy::I64),
            ValTy::F64 => TypedVal::new(
                self.builder.ins().fcvt_to_sint_sat(cl::I64, tv.val),
                ValTy::I64,
            ),
        }
    }

    /// Coerces a value to I32.
    pub fn coerce_to_i32(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::I32 => tv,
            ValTy::I64 | ValTy::Handle | ValTy::U64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => {
                TypedVal::new(self.builder.ins().ireduce(cl::I32, tv.val), ValTy::I32)
            }
            ValTy::Bool => {
                let v_ty = self.builder.func.dfg.value_type(tv.val);
                let as_i64 = if v_ty == cl::I8 {
                    self.builder.ins().uextend(cl::I64, tv.val)
                } else {
                    tv.val
                };
                TypedVal::new(self.builder.ins().ireduce(cl::I32, as_i64), ValTy::I32)
            }
            ValTy::F64 => {
                let as_i64 = self.builder.ins().fcvt_to_sint_sat(cl::I64, tv.val);
                TypedVal::new(self.builder.ins().ireduce(cl::I32, as_i64), ValTy::I32)
            }
        }
    }

    /// Coerces a value to F64.
    pub fn coerce_to_f64(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::F64 => tv,
            ValTy::I32 => TypedVal::new(
                self.builder.ins().fcvt_from_sint(cl::F64, tv.val),
                ValTy::F64,
            ),
            ValTy::I64 | ValTy::Handle | ValTy::U64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => TypedVal::new(
                self.builder.ins().fcvt_from_sint(cl::F64, tv.val),
                ValTy::F64,
            ),
            ValTy::Bool => {
                let v_ty = self.builder.func.dfg.value_type(tv.val);
                let as_i64 = if v_ty == cl::I8 {
                    self.builder.ins().uextend(cl::I64, tv.val)
                } else {
                    tv.val
                };
                TypedVal::new(
                    self.builder.ins().fcvt_from_sint(cl::F64, as_i64),
                    ValTy::F64,
                )
            }
        }
    }

    /// Returns the current loop break target, if any. Sem label,
    /// usa o loop mais interno; com label, busca pelo label.
    pub fn break_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(brk, _, _)| *brk)
    }

    /// Returns the current loop continue target, if any.
    pub fn continue_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(_, cont, _)| *cont)
    }

    /// Resolve break para um label específico — busca no stack do
    /// topo até a base. Returns None se não encontrar.
    pub fn break_block_for_label(&self, label: &str) -> Option<Block> {
        for (brk, _, lbl) in self.loop_stack.iter().rev() {
            if lbl.as_deref() == Some(label) {
                return Some(*brk);
            }
        }
        None
    }

    /// Resolve continue para um label específico.
    pub fn continue_block_for_label(&self, label: &str) -> Option<Block> {
        for (_, cont, lbl) in self.loop_stack.iter().rev() {
            if lbl.as_deref() == Some(label) {
                return Some(*cont);
            }
        }
        None
    }
}
