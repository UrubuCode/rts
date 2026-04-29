fn lower_class(cm: &Lrc<SourceMap>, name: &str, class: &SwcClass, span: SwcSpan) -> ClassDecl {
    let mut members = Vec::new();
    let mut static_init_body: Vec<Statement> = Vec::new();

    for member in &class.body {
        match member {
            SwcClassMember::StaticBlock(sb) => {
                let stmts = lower_block_body(cm, Some(&sb.body));
                static_init_body.extend(stmts);
                continue;
            }
            SwcClassMember::Constructor(constructor) => {
                let parameters = constructor
                    .params
                    .iter()
                    .filter_map(|parameter| lower_constructor_param(cm, parameter))
                    .collect::<Vec<_>>();

                let body = lower_block_body(cm, constructor.body.as_ref());

                members.push(ClassMember::Constructor(ConstructorDecl {
                    parameters,
                    body,
                    span: convert_span(cm, constructor.span),
                }));
            }
            SwcClassMember::Method(method) => {
                let name = prop_name_to_string(&method.key, cm);
                if name.is_empty() {
                    continue;
                }

                let parameters = method
                    .function
                    .params
                    .iter()
                    .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
                    .collect::<Vec<_>>();

                let body = if method.function.is_generator {
                    let desugared = method
                        .function
                        .body
                        .as_ref()
                        .map(|b| crate::parser::generator_desugar::desugar_generator_body(b));
                    lower_block_body(cm, desugared.as_ref())
                } else {
                    lower_block_body(cm, method.function.body.as_ref())
                };

                let role = match method.kind {
                    swc_ecma_ast::MethodKind::Method => MethodRole::Method,
                    swc_ecma_ast::MethodKind::Getter => MethodRole::Getter,
                    swc_ecma_ast::MethodKind::Setter => MethodRole::Setter,
                };

                let return_type = if method.function.is_generator {
                    Some("i64".to_string())
                } else {
                    method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation))
                };

                members.push(ClassMember::Method(MethodDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(method.accessibility),
                        readonly: false,
                        is_static: method.is_static,
                        is_abstract: method.is_abstract,
                    },
                    parameters,
                    return_type,
                    body,
                    role,
                    span: convert_span(cm, method.span),
                }));
            }
            SwcClassMember::PrivateMethod(method) => {
                let parameters = method
                    .function
                    .params
                    .iter()
                    .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
                    .collect::<Vec<_>>();

                let body = if method.function.is_generator {
                    let desugared = method
                        .function
                        .body
                        .as_ref()
                        .map(|b| crate::parser::generator_desugar::desugar_generator_body(b));
                    lower_block_body(cm, desugared.as_ref())
                } else {
                    lower_block_body(cm, method.function.body.as_ref())
                };

                let return_type = if method.function.is_generator {
                    Some("i64".to_string())
                } else {
                    method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation))
                };

                members.push(ClassMember::Method(MethodDecl {
                    name: format!("#{}", method.key.name),
                    modifiers: MemberModifiers {
                        visibility: Some(Visibility::Private),
                        readonly: false,
                        is_static: method.is_static,
                        is_abstract: false,
                    },
                    parameters,
                    return_type,
                    body,
                    role: MethodRole::Method,
                    span: convert_span(cm, method.span),
                }));
            }
            SwcClassMember::ClassProp(prop) => {
                let name = prop_name_to_string(&prop.key, cm);
                if name.is_empty() {
                    continue;
                }

                members.push(ClassMember::Property(PropertyDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(prop.accessibility),
                        readonly: prop.readonly,
                        is_static: prop.is_static,
                        is_abstract: prop.is_abstract,
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    initializer: prop.value.clone(),
                    span: convert_span(cm, prop.span),
                }));
            }
            SwcClassMember::PrivateProp(prop) => {
                members.push(ClassMember::Property(PropertyDecl {
                    name: format!("#{}", prop.key.name),
                    modifiers: MemberModifiers {
                        visibility: Some(Visibility::Private),
                        readonly: prop.readonly,
                        is_static: prop.is_static,
                        is_abstract: false,
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    initializer: prop.value.clone(),
                    span: convert_span(cm, prop.span),
                }));
            }
            SwcClassMember::AutoAccessor(accessor) => {
                let name = key_to_string(&accessor.key, cm);
                if name.is_empty() {
                    continue;
                }

                members.push(ClassMember::Property(PropertyDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(accessor.accessibility),
                        readonly: false,
                        is_static: accessor.is_static,
                        is_abstract: false,
                    },
                    type_annotation: accessor
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    initializer: None,
                    span: convert_span(cm, accessor.span),
                }));
            }
            _ => {}
        }
    }

    let super_class = class.super_class.as_ref().and_then(|expr| {
        if let swc_ecma_ast::Expr::Ident(id) = expr.as_ref() {
            Some(id.sym.as_str().to_string())
        } else {
            None
        }
    });

    ClassDecl {
        name: name.to_string(),
        super_class,
        members,
        is_abstract: class.is_abstract,
        static_init_body,
        span: convert_span(cm, span),
    }
}

fn lower_fn_decl(cm: &Lrc<SourceMap>, fn_decl: &SwcFnDecl) -> FunctionDecl {
    lower_function(
        cm,
        &fn_decl.ident.sym.to_string(),
        &fn_decl.function,
        fn_decl.function.span,
    )
}

fn lower_function(
    cm: &Lrc<SourceMap>,
    name: &str,
    function: &SwcFunction,
    span: SwcSpan,
) -> FunctionDecl {
    let parameters = function
        .params
        .iter()
        .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
        .collect::<Vec<_>>();

    let body = if function.is_generator {
        let desugared = function
            .body
            .as_ref()
            .map(|b| crate::parser::generator_desugar::desugar_generator_body(b));
        lower_block_body(cm, desugared.as_ref())
    } else {
        lower_block_body(cm, function.body.as_ref())
    };

    let return_type = if function.is_generator {
        // Generators sempre retornam Vec<i64> handle.
        Some("i64".to_string())
    } else {
        function
            .return_type
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation))
    };

    FunctionDecl {
        name: name.to_string(),
        parameters,
        return_type,
        body,
        span: convert_span(cm, span),
    }
}

fn lower_constructor_param(
    cm: &Lrc<SourceMap>,
    parameter: &ParamOrTsParamProp,
) -> Option<Parameter> {
    match parameter {
        ParamOrTsParamProp::Param(param) => lower_param(cm, param, MemberModifiers::default()),
        ParamOrTsParamProp::TsParamProp(param_prop) => lower_ts_param_prop(cm, param_prop),
    }
}

fn lower_ts_param_prop(cm: &Lrc<SourceMap>, param_prop: &TsParamProp) -> Option<Parameter> {
    let modifiers = MemberModifiers {
        visibility: map_accessibility(param_prop.accessibility),
        readonly: param_prop.readonly,
        is_static: false,
        is_abstract: false,
    };

    match &param_prop.param {
        TsParamPropParam::Ident(binding) => Some(Parameter {
            name: binding.id.sym.to_string(),
            type_annotation: binding
                .type_ann
                .as_ref()
                .map(|annotation| normalize_type_annotation(cm, annotation)),
            modifiers,
            variadic: false,
            default: None,
            span: convert_span(cm, param_prop.span),
        }),
        TsParamPropParam::Assign(assign) => Some(Parameter {
            name: pat_name(&assign.left, cm).unwrap_or_else(|| "param".to_string()),
            type_annotation: pat_type_annotation(cm, &assign.left),
            modifiers,
            variadic: false,
            default: Some(assign.right.clone()),
            span: convert_span(cm, param_prop.span),
        }),
    }
}

fn lower_param(
    cm: &Lrc<SourceMap>,
    param: &SwcParam,
    modifiers: MemberModifiers,
) -> Option<Parameter> {
    let name = pat_name(&param.pat, cm)?;
    let variadic = matches!(param.pat, Pat::Rest(_));
    let type_annotation = pat_type_annotation(cm, &param.pat);
    // Default param `(x = expr)` — SWC representa como Pat::Assign.
    let default = match &param.pat {
        Pat::Assign(assign) => Some(assign.right.clone()),
        _ => None,
    };

    Some(Parameter {
        name,
        type_annotation,
        modifiers,
        variadic,
        default,
        span: convert_span(cm, param.span),
    })
}

