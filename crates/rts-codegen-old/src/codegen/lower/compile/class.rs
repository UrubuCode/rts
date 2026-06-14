//! Compilacao de classes para FunctionDecls sintetizadas.
//!
//! `synthesize_class_fns` percorre uma `ClassDecl` e produz:
//! - `__class_C__init` — inicializa fields da instancia (chamado por
//!   `new C()` antes do constructor).
//! - `__class_C_<method>` — cada metodo de instancia.
//! - `__class_C_static_<method>` — cada metodo estatico.
//! - `__class_C_get_<prop>` / `__class_C_set_<prop>` — getters/setters.
//! - `__class_C_lifted_arrow_N` — arrows lifted que capturam `this`.
//!
//! `validate_abstract_method_implementations` checa que classes nao-abstract
//! implementam todos os abstract methods herdados.
//!
//! Helpers `class_*_name` sao a fonte unica do mangle convention; outros
//! arquivos do codegen importam via re-export em `func`.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use swc_ecma_ast::{Callee, Expr, Lit, MemberProp, Stmt};

use crate::parser::ast::{
    ClassDecl, ClassMember, FunctionDecl, MemberModifiers, MethodRole, Parameter, RawStmt,
    Statement,
};
use crate::parser::span::Span;

use super::super::ctx::{ClassMeta, ValTy};

pub(crate) fn validate_abstract_method_implementations(
    classes: &HashMap<String, ClassMeta>,
) -> Result<()> {
    for (name, meta) in classes {
        if meta.is_abstract {
            continue; // abstract classes podem deixar abstracts pendentes
        }

        // Acumula abstracts da hierarquia.
        let mut required: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut cur = Some(name.clone());
        while let Some(c) = cur {
            if let Some(m) = classes.get(&c) {
                for am in &m.abstract_methods {
                    required.insert(am.clone());
                }
                cur = m.super_class.clone();
            } else {
                break;
            }
        }

        // Subtrai métodos concretos providos pela classe ou ancestrais.
        let mut cur = Some(name.clone());
        while let Some(c) = cur {
            if let Some(m) = classes.get(&c) {
                for method in &m.methods {
                    if !m.abstract_methods.contains(method) {
                        required.remove(method);
                    }
                }
                cur = m.super_class.clone();
            } else {
                break;
            }
        }

        if !required.is_empty() {
            let mut missing: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
            missing.sort();
            return Err(anyhow!(
                "classe concreta `{name}` nao implementa metodo(s) abstract: {}",
                missing.join(", ")
            ));
        }
    }
    Ok(())
}

/// (#nested-this) Escaneia um statement por padroes
/// `this.X = { k: v, ... }` ou `this.X = { k: { ... } }` e popula
/// field_obj_types / field_nested_obj_types. Inferencia simples
/// baseada em tipos de literais (Str→Handle, Num→I64, Bool→Bool).
fn scan_this_obj_assign(
    s: &Statement,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    let Statement::Raw(rs) = s;
    let Some(stmt) = rs.stmt.as_ref() else { return };
    scan_this_stmt(stmt, field_obj_types, field_nested_obj_types);
}

fn scan_this_stmt(
    stmt: &swc_ecma_ast::Stmt,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    use swc_ecma_ast::*;
    match stmt {
        Stmt::Expr(e) => scan_this_expr(&e.expr, field_obj_types, field_nested_obj_types),
        Stmt::Block(b) => {
            for s in &b.stmts {
                scan_this_stmt(s, field_obj_types, field_nested_obj_types);
            }
        }
        Stmt::If(i) => {
            scan_this_stmt(&i.cons, field_obj_types, field_nested_obj_types);
            if let Some(alt) = &i.alt {
                scan_this_stmt(alt, field_obj_types, field_nested_obj_types);
            }
        }
        _ => {}
    }
}

fn scan_this_expr(
    e: &swc_ecma_ast::Expr,
    field_obj_types: &mut HashMap<String, HashMap<String, ValTy>>,
    field_nested_obj_types: &mut HashMap<(String, String), HashMap<String, ValTy>>,
) {
    use swc_ecma_ast::*;
    let Expr::Assign(a) = e else { return };
    if !matches!(a.op, AssignOp::Assign) {
        return;
    }
    // LHS: this.X
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return;
    };
    if !matches!(m.obj.as_ref(), Expr::This(_)) {
        return;
    }
    let MemberProp::Ident(field_id) = &m.prop else {
        return;
    };
    let field_name = field_id.sym.as_str().to_string();
    // RHS: object literal
    let Expr::Object(obj) = a.right.as_ref() else {
        return;
    };
    let mut fts: HashMap<String, ValTy> = HashMap::new();
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = p.as_ref() {
                let key = match &kv.key {
                    PropName::Ident(i) => i.sym.as_str().to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    _ => continue,
                };
                match kv.value.as_ref() {
                    Expr::Lit(Lit::Str(_)) => {
                        fts.insert(key, ValTy::Handle);
                    }
                    Expr::Lit(Lit::Num(_)) => {
                        fts.insert(key, ValTy::I64);
                    }
                    Expr::Lit(Lit::Bool(_)) => {
                        fts.insert(key, ValTy::Bool);
                    }
                    Expr::Object(sub) => {
                        fts.insert(key.clone(), ValTy::Handle);
                        let mut sub_fts: HashMap<String, ValTy> = HashMap::new();
                        for sp in &sub.props {
                            if let PropOrSpread::Prop(spx) = sp {
                                if let Prop::KeyValue(skv) = spx.as_ref() {
                                    let sk = match &skv.key {
                                        PropName::Ident(i) => i.sym.as_str().to_string(),
                                        PropName::Str(s) => {
                                            s.value.to_string_lossy().to_string()
                                        }
                                        _ => continue,
                                    };
                                    match skv.value.as_ref() {
                                        Expr::Lit(Lit::Str(_)) => {
                                            sub_fts.insert(sk, ValTy::Handle);
                                        }
                                        Expr::Lit(Lit::Num(_)) => {
                                            sub_fts.insert(sk, ValTy::I64);
                                        }
                                        Expr::Lit(Lit::Bool(_)) => {
                                            sub_fts.insert(sk, ValTy::Bool);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if !sub_fts.is_empty() {
                            field_nested_obj_types
                                .insert((field_name.clone(), key), sub_fts);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if !fts.is_empty() {
        field_obj_types.insert(field_name, fts);
    }
}

pub(crate) fn synthesize_class_fns(
    class: &ClassDecl,
    classes_with_init: &std::collections::HashSet<String>,
    class_ctor_params: &HashMap<String, Vec<crate::parser::ast::Parameter>>,
) -> (ClassMeta, Vec<FunctionDecl>) {
    let mut methods: Vec<String> = Vec::new();
    let mut getters: Vec<String> = Vec::new();
    let mut setters: Vec<String> = Vec::new();
    let mut static_methods: Vec<String> = Vec::new();
    let mut static_fields: Vec<String> = Vec::new();
    let mut fns: Vec<FunctionDecl> = Vec::new();
    let mut field_types: HashMap<String, ValTy> = HashMap::new();
    let mut field_class_names: HashMap<String, String> = HashMap::new();
    let mut field_obj_types: HashMap<String, HashMap<String, ValTy>> = HashMap::new();
    let mut field_nested_obj_types: HashMap<(String, String), HashMap<String, ValTy>> =
        HashMap::new();
    let mut readonly_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut abstract_methods: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut member_visibility: std::collections::HashMap<String, crate::parser::ast::Visibility> =
        std::collections::HashMap::new();
    let mut has_constructor = false;

    // Coleta initializers de instância (`x = expr`) na ordem declarada.
    // Serão prependidos ao body do constructor (depois de `super()` se
    // houver). Static props ficam fora — initializers static seriam
    // tratados separadamente (não cobertos neste commit).
    let init_stmts: Vec<Statement> = class
        .members
        .iter()
        .filter_map(|m| match m {
            ClassMember::Property(prop)
                if !prop.modifiers.is_static && prop.initializer.is_some() =>
            {
                let init = prop.initializer.as_ref().unwrap().clone();
                // (cross-runtime #267) Privates escopados por declaring class.
                let mangled_name = if let Some(rest) = prop.name.strip_prefix('#') {
                    format!("#{}_{}", class.name, rest)
                } else {
                    prop.name.clone()
                };
                Some(make_field_init_stmt(&mangled_name, init, prop.span))
            }
            _ => None,
        })
        .collect();

    // (375) Pre-passada: coleta tipos de campos ANTES de processar metodos,
    // pra que getters sem anotacao (`get name() { return this.#name; }`)
    // possam inferir o return_type do tipo do campo retornado — mesmo quando
    // o getter aparece antes da propriedade no fonte. Cobre tanto a anotacao
    // explicita quanto a inferida do initializer.
    let mut prescan_field_ty: HashMap<String, String> = HashMap::new();
    for member in &class.members {
        match member {
            ClassMember::Property(prop) if !prop.modifiers.is_static => {
                if let Some(ann) = prop.type_annotation.as_deref() {
                    prescan_field_ty.insert(prop.name.clone(), ann.trim().to_string());
                } else if let Some(init) = prop.initializer.as_ref() {
                    // Infere tipo do initializer quando nao ha anotacao:
                    // string lit / template -> "string"; bool -> "boolean".
                    // Cobre `#v = "A-priv"` (private string sem `: string`).
                    if matches!(
                        init.as_ref(),
                        swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_))
                            | swc_ecma_ast::Expr::Tpl(_)
                    ) {
                        prescan_field_ty.insert(prop.name.clone(), "string".to_string());
                    } else {
                        let inferred = crate::codegen::lower::analysis::types::infer_expr_ty(
                            Some(init.as_ref()),
                        );
                        if matches!(inferred, ValTy::Bool) {
                            prescan_field_ty.insert(prop.name.clone(), "boolean".to_string());
                        }
                    }
                }
            }
            ClassMember::Constructor(ctor) => {
                for p in &ctor.parameters {
                    if let Some(ann) = p.type_annotation.as_deref() {
                        prescan_field_ty
                            .entry(p.name.clone())
                            .or_insert_with(|| ann.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    for member in &class.members {
        match member {
            ClassMember::Constructor(ctor) => {
                has_constructor = true;
                // (#303 parte 1) Detecta \`super(...); super(...)\` em
                // sequencia direta (mesmo bloco/escopo top-level do
                // constructor body). JS proibe — segundo super lanca
                // ReferenceError. So' rejeita o caso linear obvio; em
                // branches if/else mutuamente exclusivos (super em cons
                // E em alt), passa silenciosamente — runtime check seria
                // a fase 2 dessa issue.
                if count_super_calls_in_top_level(&ctor.body) > 1 {
                    // SyntaxError-like: rejeita o programa em compile time.
                    // Usar eprintln + std::process::exit(1) imita o caminho
                    // de erros existentes em outras validacoes (abstract,
                    // visibility). Sem Result aqui pra nao espalhar
                    // mudanca de tipo em todo synthesize_class_fns caller.
                    eprintln!(
                        "error: ReferenceError: super constructor may only be called once (em `{}`)",
                        class.name
                    );
                    std::process::exit(1);
                }
                for p in &ctor.parameters {
                    if let Some(ann) = p.type_annotation.as_deref() {
                        field_types
                            .entry(p.name.clone())
                            .or_insert(ValTy::from_annotation(ann));
                    }
                }
                let mut params = Vec::with_capacity(ctor.parameters.len() + 1);
                params.push(this_param(ctor.span));
                params.extend(ctor.parameters.iter().cloned());
                // Body = [super() se houver no inicio] + initializers + user code.
                // Detecta `super(...)` na primeira posição e injeta initializers
                // logo depois (semântica TS: initializers rodam depois do
                // super call).
                let body = weave_initializers(&ctor.body, &init_stmts, class.super_class.is_some());
                fns.push(FunctionDecl {
                    name: class_init_name(&class.name),
                    parameters: params,
                    return_type: None,
                    body,
                    span: ctor.span,
                    is_async: false,
                });
            }
            ClassMember::Method(method) => {
                // Visibility — registra apenas private/protected (public é default).
                if let Some(v) = method.modifiers.visibility {
                    if !matches!(v, crate::parser::ast::Visibility::Public) {
                        member_visibility.insert(method.name.clone(), v);
                    }
                }
                // Métodos abstract: gera um stub que faz `throw "abstract"`
                // (na prática, retorna 0). O stub permite que o codegen
                // resolva referências `__class_C_<m>` para checagem de
                // assinatura, e o dispatch virtual roteia para a impl
                // concreta da subclasse em runtime. Se chamado direto na
                // base abstract (não deveria acontecer porque `new` é
                // bloqueado), retorna o default da assinatura.
                if method.modifiers.is_abstract {
                    abstract_methods.insert(method.name.clone());
                    if matches!(method.role, MethodRole::Method) {
                        methods.push(method.name.clone());
                    }
                    let synth_name = match method.role {
                        MethodRole::Getter => class_getter_name(&class.name, &method.name),
                        MethodRole::Setter => class_setter_name(&class.name, &method.name),
                        MethodRole::Method => class_method_name(&class.name, &method.name),
                    };
                    let mut params = Vec::with_capacity(method.parameters.len() + 1);
                    params.push(this_param(method.span));
                    params.extend(method.parameters.iter().cloned());
                    // Body do stub: retorna o default do tipo declarado.
                    // Se return_type é "void", body vazio basta. Caso
                    // contrário, `return 0;` ou `return 0.0;`.
                    let body = synth_abstract_stub_body(method.return_type.as_deref());
                    fns.push(FunctionDecl {
                        name: synth_name,
                        parameters: params,
                        return_type: method.return_type.clone(),
                        body,
                        span: method.span,
                        is_async: false,
                    });
                    continue;
                }
                if method.modifiers.is_static {
                    static_methods.push(method.name.clone());
                    fns.push(FunctionDecl {
                        name: class_static_method_name(&class.name, &method.name),
                        parameters: method.parameters.clone(),
                        return_type: method.return_type.clone(),
                        body: method.body.clone(),
                        span: method.span,
                        is_async: false,
                    });
                } else {
                    let synth_name = match method.role {
                        MethodRole::Getter => {
                            getters.push(method.name.clone());
                            class_getter_name(&class.name, &method.name)
                        }
                        MethodRole::Setter => {
                            setters.push(method.name.clone());
                            class_setter_name(&class.name, &method.name)
                        }
                        MethodRole::Method => {
                            methods.push(method.name.clone());
                            class_method_name(&class.name, &method.name)
                        }
                    };
                    let mut params = Vec::with_capacity(method.parameters.len() + 1);
                    params.push(this_param(method.span));
                    params.extend(method.parameters.iter().cloned());
                    // (375) Getter OU metodo sem anotacao cujo corpo eh
                    // `return this.<field>`: herda o tipo do campo. Sem isso a
                    // inferencia caia em F64 e um campo string voltava como
                    // handle cru (`d.name`/`b.getV()` imprimiam o numero do
                    // handle). Cobre tanto `get name()` quanto `getV()`.
                    let return_type = if method.return_type.is_none()
                        && matches!(method.role, MethodRole::Getter | MethodRole::Method)
                    {
                        getter_field_return_ty(&method.body, &prescan_field_ty)
                            .or_else(|| method.return_type.clone())
                    } else {
                        method.return_type.clone()
                    };
                    fns.push(FunctionDecl {
                        name: synth_name,
                        parameters: params,
                        return_type,
                        body: method.body.clone(),
                        span: method.span,
                        is_async: false,
                    });
                }
            }
            ClassMember::Property(prop) => {
                // Visibility — registra apenas private/protected.
                if let Some(v) = prop.modifiers.visibility {
                    if !matches!(v, crate::parser::ast::Visibility::Public) {
                        member_visibility.insert(prop.name.clone(), v);
                    }
                }
                if prop.modifiers.is_static {
                    static_fields.push(prop.name.clone());
                } else {
                    if let Some(ann) = prop.type_annotation.as_deref() {
                        let ann = ann.trim();
                        field_types.insert(prop.name.clone(), ValTy::from_annotation(ann));
                        field_class_names.insert(prop.name.clone(), ann.to_string());
                    } else if let Some(init) = prop.initializer.as_ref() {
                        // (cross-runtime #341) Sem anotacao, inferir do init:
                        // `_v = true` => Bool. Sem isso, console.log(this._v)
                        // imprime 1 em vez de "true".
                        let inferred = crate::codegen::lower::analysis::types::infer_expr_ty(
                            Some(init.as_ref()),
                        );
                        if matches!(inferred, ValTy::Bool) {
                            field_types.insert(prop.name.clone(), ValTy::Bool);
                            field_class_names.insert(prop.name.clone(), "boolean".to_string());
                        }
                    }
                    if prop.modifiers.readonly {
                        readonly_fields.insert(prop.name.clone());
                    }
                    // Private fields sem anotação ainda precisam ser
                    // detectáveis na hierarquia para validação de escopo.
                    // Garantimos uma entrada em field_types (default I64).
                    if prop.name.starts_with('#') && !field_types.contains_key(&prop.name) {
                        field_types.insert(prop.name.clone(), ValTy::I64);
                    }
                }
            }
        }
    }

    // Se a classe não tem constructor explícito mas tem initializers,
    // sintetizamos um ctor implícito que apenas executa-os. Para classes
    // com `extends` mas sem ctor explícito, TS gera um pass-through
    // `constructor(...args) { super(...args); }` — não suportamos rest
    // args ainda (#58/#59), então damos erro claro nesse caso.
    // (cross-runtime #267) Mesmo sem initializers proprios, se super tem
    // init, precisa gerar __init que chama super.__init. Caso contrario,
    // privates declarados em ancestrais nao sao inicializados em
    // subclasses sem ctor explicito.
    let super_needs_init = class
        .super_class
        .as_deref()
        .map(|s| classes_with_init.contains(s))
        .unwrap_or(false);
    // (cross-runtime #336) Sempre gera `__init` sintetico para classes
    // sem ctor user — necessario para que `new ClassX()` em new_expr.rs
    // possa fazer `emit_user_fn_addr(__class_X__init)` e instalar
    // `__proto__` chain. O body fica vazio quando nao ha init_stmts nem
    // super_needs_init, mas o symbol existe.
    //
    // Verifica se super tem qualquer ctor real (com_constructor=true) —
    // nesse caso precisamos delegar pra super.__init mesmo se nao houver
    // field initializers proprios. Sem isso, `class Sub extends Holder {}`
    // falha em popular fields setados em Holder ctor (this.cfg = ...).
    let super_has_ctor = class
        .super_class
        .as_deref()
        .map(|s| class_ctor_params.contains_key(s))
        .unwrap_or(false);
    // Re-avalia super_needs_init englobando classes cuja inicializacao
    // depende de super.__init real (ctor user, nao apenas field inits).
    let super_needs_init = super_needs_init || super_has_ctor;
    if !has_constructor {
        // (cross-runtime #1057) Resolve params herdados do ctor da super class
        // (chain). Sem isso, `class Dog extends Animal {}` gera __init(this)
        // chamando Animal.__init(this) sem propagar args, e GuideDog que
        // chama `super(name)` falha em Dog.__init com "espera 0 args".
        let inherited_ctor_params: Vec<crate::parser::ast::Parameter> = {
            let cur = class.super_class.clone();
            let mut found: Vec<crate::parser::ast::Parameter> = Vec::new();
            // Sobe a chain ate achar uma classe com ctor explicito.
            while let Some(parent_name) = cur {
                if let Some(ps) = class_ctor_params.get(&parent_name) {
                    if !ps.is_empty() {
                        found = ps.clone();
                        break;
                    }
                }
                // Olha o grandparent — busca proxima classe na chain.
                let grand = class_ctor_params
                    .keys()
                    .find_map(|_| None::<String>); // placeholder; precisaria do super_class do parent
                let _ = grand;
                break; // simplifica para 1 nivel — Dog->Animal direto.
            }
            found
        };
        // Prepend chamada para `__class_<super>__init(this, ...args)` quando aplicavel.
        let mut prelude: Vec<Statement> = Vec::new();
        if super_needs_init {
            let parent = class.super_class.as_deref().unwrap();
            // Args do super_init: this + cada inherited param como ident.
            let mut super_args: Vec<swc_ecma_ast::ExprOrSpread> = Vec::new();
            super_args.push(swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
                    span: Default::default(),
                })),
            });
            for p in &inherited_ctor_params {
                super_args.push(swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: p.name.clone().into(),
                        optional: false,
                    })),
                });
            }
            let super_init_call = Stmt::Expr(swc_ecma_ast::ExprStmt {
                span: Default::default(),
                expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
                    span: Default::default(),
                    ctxt: Default::default(),
                    callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Ident(
                        swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: class_init_name(parent).into(),
                            optional: false,
                        },
                    ))),
                    args: super_args,
                    type_args: None,
                })),
            });
            prelude.push(Statement::Raw(
                RawStmt::new("<super-init>".to_string(), class.span).with_stmt(super_init_call),
            ));
        }
        let mut full_body = prelude;
        full_body.extend(weave_initializers(&[], &init_stmts, false));
        // Params do __init sintetico: this + params herdados (pass-through).
        let mut params = vec![this_param(class.span)];
        params.extend(inherited_ctor_params.iter().cloned());
        fns.push(FunctionDecl {
            name: class_init_name(&class.name),
            parameters: params,
            return_type: None,
            body: full_body,
            span: class.span,
            is_async: false,
        });
        has_constructor = true;
    }

    // (#nested-this) Escaneia o body do constructor (e initializers) por
    // `this.X = { sub: { ... } }` ou `this.X = { ... }` e popula
    // field_obj_types / field_nested_obj_types. Permite resolver
    // `this.cfg.server.host` em metodos de instancia.
    {
        // Stmts a escanear: ctor body (se houver) + init_stmts (initializers
        // de propriedade que serao inseridos no ctor sintetizado).
        let mut stmts_to_scan: Vec<&Statement> = Vec::new();
        for member in &class.members {
            if let ClassMember::Constructor(ctor) = member {
                for s in &ctor.body {
                    stmts_to_scan.push(s);
                }
            }
        }
        for s in &init_stmts {
            stmts_to_scan.push(s);
        }
        for s in stmts_to_scan {
            scan_this_obj_assign(
                s,
                &mut field_obj_types,
                &mut field_nested_obj_types,
            );
        }
    }

    let meta = ClassMeta {
        name: class.name.clone(),
        super_class: class.super_class.clone(),
        methods,
        field_types,
        field_class_names,
        field_obj_types,
        field_nested_obj_types,
        static_methods,
        static_fields,
        getters,
        setters,
        has_constructor,
        readonly_fields,
        is_abstract: class.is_abstract,
        abstract_methods,
        member_visibility,
        layout: None,
    };
    (meta, fns)
}

/// `this.<name> = <init>;` como Statement RTS.
/// (375) Para um getter sem anotacao cujo corpo retorna `this.<field>`,
/// devolve a anotacao de tipo do campo (de `prescan_field_ty`). Permite que a
/// inferencia de return_type herde o tipo do campo em vez de cair em F64.
/// Conservador: so' reconhece `return this.<ident>` / `return this.#<priv>`
/// como unico statement relevante; qualquer outra forma retorna None.
fn getter_field_return_ty(
    body: &[Statement],
    prescan_field_ty: &HashMap<String, String>,
) -> Option<String> {
    use swc_ecma_ast::{Expr, MemberProp, Stmt};
    fn field_of_return(e: &Expr) -> Option<String> {
        // Peel Paren/TsAs/TsNonNull.
        let inner = match e {
            Expr::Paren(p) => return field_of_return(&p.expr),
            Expr::TsAs(a) => return field_of_return(&a.expr),
            Expr::TsNonNull(n) => return field_of_return(&n.expr),
            other => other,
        };
        let Expr::Member(m) = inner else { return None };
        if !matches!(m.obj.as_ref(), Expr::This(_)) {
            return None;
        }
        match &m.prop {
            MemberProp::Ident(id) => Some(id.sym.to_string()),
            MemberProp::PrivateName(pn) => Some(format!("#{}", pn.name.as_ref())),
            MemberProp::Computed(_) => None,
        }
    }
    for s in body {
        let Statement::Raw(rs) = s;
        let Some(stmt) = rs.stmt.as_ref() else { continue };
        if let Stmt::Return(r) = stmt {
            if let Some(arg) = r.arg.as_deref() {
                if let Some(field) = field_of_return(arg) {
                    return prescan_field_ty.get(&field).cloned();
                }
            }
        }
    }
    None
}

fn make_field_init_stmt(
    name: &str,
    init: Box<swc_ecma_ast::Expr>,
    span: crate::parser::span::Span,
) -> Statement {
    let lhs = Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
            span: Default::default(),
        })),
        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
            span: Default::default(),
            sym: name.into(),
        }),
    });
    let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
        span: Default::default(),
        op: swc_ecma_ast::AssignOp::Assign,
        left: swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(
            swc_ecma_ast::MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
                    span: Default::default(),
                })),
                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                    span: Default::default(),
                    sym: name.into(),
                }),
            },
        )),
        right: init,
    });
    let _ = lhs; // não usamos; AssignTarget já carrega o lado esquerdo.
    let stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(assign),
    });
    Statement::Raw(RawStmt::new("<field-init>".to_string(), span).with_stmt(stmt))
}

/// Costura initializers no body do constructor, respeitando `super()`.
/// - Se `has_super` e o primeiro statement do user é `super(...)`,
///   coloca os initializers logo depois.
/// - Caso contrário, prepende.
/// (#303 parte 1) Conta \`super(...)\` no nivel top-level do body de um
/// constructor, sem descer em if/else/loops/blocks. Detect de duplicacao
/// linear evita o caso degenerate \`super(); super();\`.
pub(crate) fn count_super_calls_in_top_level(body: &[Statement]) -> usize {
    use swc_ecma_ast::{Callee, Expr, Stmt};
    let mut count = 0usize;
    for stmt in body {
        let Statement::Raw(raw) = stmt;
        let Some(s) = raw.stmt.as_ref() else { continue };
        if let Stmt::Expr(e) = s {
            if let Expr::Call(c) = e.expr.as_ref() {
                if matches!(c.callee, Callee::Super(_)) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn weave_initializers(
    user_body: &[Statement],
    init_stmts: &[Statement],
    has_super: bool,
) -> Vec<Statement> {
    if init_stmts.is_empty() {
        return user_body.to_vec();
    }

    let mut out: Vec<Statement> = Vec::with_capacity(user_body.len() + init_stmts.len());

    let super_at_start = has_super
        && user_body
            .first()
            .map(|s| is_super_call_stmt(s))
            .unwrap_or(false);

    if super_at_start {
        out.push(user_body[0].clone());
        out.extend(init_stmts.iter().cloned());
        out.extend(user_body.iter().skip(1).cloned());
    } else {
        out.extend(init_stmts.iter().cloned());
        out.extend(user_body.iter().cloned());
    }

    out
}

fn is_super_call_stmt(s: &Statement) -> bool {
    let Statement::Raw(raw) = s;
    let Some(Stmt::Expr(expr_stmt)) = raw.stmt.as_ref() else {
        return false;
    };
    let Expr::Call(call) = expr_stmt.expr.as_ref() else {
        return false;
    };
    matches!(call.callee, Callee::Super(_))
}

/// Body padrão para stub de método abstract: `return 0;` (ou nada se void).
fn synth_abstract_stub_body(return_type: Option<&str>) -> Vec<Statement> {
    let ret_type = return_type.map(|s| s.trim()).unwrap_or("void");
    if ret_type == "void" || ret_type.is_empty() {
        return Vec::new();
    }
    let zero_expr = if ret_type == "f64" || ret_type == "F64" {
        // f64 → 0.0
        Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: None,
        }))
    } else {
        // i32/i64/handle/bool: literal 0
        Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: Some("0".into()),
        }))
    };
    let stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
        span: Default::default(),
        arg: Some(Box::new(zero_expr)),
    });
    vec![Statement::Raw(
        RawStmt::new("<abstract-stub>".to_string(), Span::default()).with_stmt(stmt),
    )]
}

fn this_param(span: crate::parser::span::Span) -> Parameter {
    Parameter {
        name: "this".to_string(),
        type_annotation: None,
        modifiers: MemberModifiers::default(),
        variadic: false,
        optional: false,
        default: None,
        span,
    }
}

pub(crate) fn class_init_name(class: &str) -> String {
    format!("__class_{class}__init")
}

pub(crate) fn class_method_name(class: &str, method: &str) -> String {
    format!("__class_{class}_{method}")
}

pub(crate) fn class_static_method_name(class: &str, method: &str) -> String {
    format!("__class_{class}_static_{method}")
}

pub(crate) fn class_getter_name(class: &str, prop: &str) -> String {
    format!("__class_{class}_get_{prop}")
}

pub(crate) fn class_setter_name(class: &str, prop: &str) -> String {
    format!("__class_{class}_set_{prop}")
}
