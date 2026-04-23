fn lower_class(cm: &Lrc<SourceMap>, name: &str, class: &SwcClass, span: SwcSpan) -> ClassDecl {
    let mut members = Vec::new();

    for member in &class.body {
        match member {
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

                let body = lower_block_body(cm, method.function.body.as_ref());

                members.push(ClassMember::Method(MethodDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(method.accessibility),
                        readonly: false,
                        is_static: method.is_static,
                    },
                    parameters,
                    return_type: method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    body,
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

                let body = lower_block_body(cm, method.function.body.as_ref());

                members.push(ClassMember::Method(MethodDecl {
                    name: format!("#{}", method.key.name),
                    modifiers: MemberModifiers {
                        visibility: Some(Visibility::Private),
                        readonly: false,
                        is_static: method.is_static,
                    },
                    parameters,
                    return_type: method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    body,
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
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
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
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
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
                    },
                    type_annotation: accessor
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, accessor.span),
                }));
            }
            _ => {}
        }
    }

    ClassDecl {
        name: name.to_string(),
        members,
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

    let body = lower_block_body(cm, function.body.as_ref());

    FunctionDecl {
        name: name.to_string(),
        parameters,
        return_type: function
            .return_type
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
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
            span: convert_span(cm, param_prop.span),
        }),
        TsParamPropParam::Assign(assign) => Some(Parameter {
            name: pat_name(&assign.left, cm).unwrap_or_else(|| "param".to_string()),
            type_annotation: pat_type_annotation(cm, &assign.left),
            modifiers,
            variadic: false,
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

    Some(Parameter {
        name,
        type_annotation,
        modifiers,
        variadic,
        span: convert_span(cm, param.span),
    })
}

