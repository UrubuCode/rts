#[cfg(test)]
mod tests {
    use crate::hir::nodes::{HirFunction, HirItem, HirModule, HirParameter};

    use crate::mir::typed::typed;
    use crate::mir::{MirBinOp, MirInstruction};

    fn build_simple_module(statements: Vec<&str>) -> HirModule {
        use crate::hir::nodes::HirStmt;
        HirModule {
            items: statements
                .into_iter()
                .map(|s| HirItem::Statement(HirStmt::new(s.to_string(), None)))
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
                body: vec![crate::hir::nodes::HirStmt::new(
                    "return a + b;".to_string(),
                    None,
                )],
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
                body: vec![crate::hir::nodes::HirStmt::new(
                    "return a + b;".to_string(),
                    None,
                )],
                loc: None,
            }],
            imports: Vec::new(),
            classes: Vec::new(),
            interfaces: Vec::new(),
        };
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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

        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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

        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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
        let mir = typed(&hir);
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

        let mir = typed(&hir);
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
