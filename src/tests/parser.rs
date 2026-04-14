use crate::compile_options::FrontendMode;
use crate::parser::ast::Item;
use crate::parser::{parse_source, parse_source_with_mode};

    #[test]
    fn parses_typescript_module_items_into_internal_ast() {
        let source = r#"
            import { print } from "rts";

            interface Teste {
                valor: i32;
            }

            class A {
                private readonly x: i8;
                constructor(public value: i16) {}
                run(): void {}
            }

            function main(x: i8): i32 {
                return x;
            }

            const valor = 2 * 60 * 60 * 1000;
        "#;

        let program = parse_source(source).expect("parser should accept valid TS");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Import(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Interface(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Class(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Function(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Statement(_)))
        );
    }

    #[test]
    fn compat_mode_parses_plain_javascript() {
        let source = "const valor = 1 + 2;";
        let program = parse_source_with_mode(source, FrontendMode::Compat)
            .expect("compat mode should parse plain JS");
        assert!(!program.items.is_empty());
    }

    #[test]
    fn compat_mode_falls_back_to_typescript_when_needed() {
        let source = "const valor: i8 = 42;";
        let program = parse_source_with_mode(source, FrontendMode::Compat)
            .expect("compat mode should fallback to TS parser");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Statement(_)))
        );
    }
