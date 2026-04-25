//! Integration tests: compile each `.ts` fixture and compare stdout to `.out`.
//!
//! Each test compiles the fixture via the RTS pipeline, links against the
//! runtime support objects, runs the resulting binary, and asserts its stdout
//! matches the adjacent `.out` file byte-for-byte.
//!
//! Requires a pre-built `target/release/rts` binary (or `target/debug/rts`
//! as fallback). Tests are skipped if no binary is found.

use std::path::{Path, PathBuf};
use std::process::Command;

fn rts_binary() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = manifest.join("target/release/rts.exe");
    let debug = manifest.join("target/debug/rts.exe");
    // Non-Windows paths
    let release_nix = manifest.join("target/release/rts");
    let debug_nix = manifest.join("target/debug/rts");

    for p in [release, debug, release_nix, debug_nix] {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn run_fixture(name: &str) {
    let Some(rts) = rts_binary() else {
        eprintln!("skipping {name}: no rts binary found");
        return;
    };

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ts_path = manifest.join(format!("tests/fixtures/{name}.ts"));
    let expected_path = manifest.join(format!("tests/fixtures/{name}.out"));

    let expected = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|_| panic!("missing expected output file: {}", expected_path.display()));

    let output = Command::new(&rts)
        .args(["run", ts_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("failed to run rts for {name}: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "fixture `{name}` exited with status {}\nstderr: {stderr}",
        output.status,
    );

    assert_eq!(
        stdout.as_ref(),
        expected,
        "fixture `{name}` stdout mismatch\nstderr: {stderr}",
    );
}

/// Roda uma fixture esperando falha de compile/run, e exige que o stderr
/// contenha um substring específico. Use para validar mensagens de erro.
fn run_fixture_expect_error(name: &str, expected_substr: &str) {
    let Some(rts) = rts_binary() else {
        eprintln!("skipping {name}: no rts binary found");
        return;
    };

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ts_path = manifest.join(format!("tests/fixtures/{name}.ts"));

    let output = Command::new(&rts)
        .args(["run", ts_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("failed to run rts for {name}: {e}"));

    assert!(
        !output.status.success(),
        "fixture `{name}` deveria ter falhado mas saiu com status {}",
        output.status,
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected_substr),
        "fixture `{name}` falhou mas mensagem de erro não contém `{expected_substr}`\nstderr completo:\n{stderr}",
    );
}

#[test]
fn fixture_empty() {
    run_fixture("empty");
}

#[test]
fn fixture_print() {
    run_fixture("print");
}

#[test]
fn fixture_arithmetic() {
    run_fixture("arithmetic");
}

#[test]
fn fixture_if_else() {
    run_fixture("if_else");
}

#[test]
fn fixture_while_loop() {
    run_fixture("while_loop");
}

#[test]
fn fixture_functions() {
    run_fixture("functions");
}

#[test]
fn fixture_string_concat() {
    run_fixture("string_concat");
}

#[test]
fn fixture_template_literals() {
    run_fixture("template_literals");
}

#[test]
fn fixture_let_const_var() {
    run_fixture("let_const_var");
}

#[test]
fn fixture_function_expressions() {
    run_fixture("function_expressions");
}

#[test]
fn fixture_arrow_functions() {
    run_fixture("arrow_functions");
}

#[test]
fn fixture_bitwise_ops() {
    run_fixture("bitwise_ops");
}

#[test]
fn fixture_ternary() {
    run_fixture("ternary");
}

#[test]
fn fixture_f64_modulo() {
    run_fixture("f64_modulo");
}

#[test]
fn fixture_switch_jump_table() {
    run_fixture("switch_jump_table");
}

#[test]
fn fixture_exponentiation() {
    run_fixture("exponentiation");
}

#[test]
fn fixture_compound_assign() {
    run_fixture("compound_assign");
}

#[test]
fn fixture_typeof_void_delete() {
    run_fixture("typeof_void_delete");
}

#[test]
fn fixture_nullish_optional() {
    run_fixture("nullish_optional");
}

#[test]
fn fixture_try_catch() {
    run_fixture("try_catch");
}

#[test]
fn fixture_tail_call() {
    run_fixture("tail_call");
}

#[test]
fn fixture_first_class_functions() {
    run_fixture("first_class_functions");
}

#[test]
fn fixture_object_array_literals() {
    run_fixture("object_array_literals");
}

#[test]
fn fixture_for_of() {
    run_fixture("for_of");
}

#[test]
fn fixture_string_eq() {
    run_fixture("string_eq");
}

#[test]
fn fixture_class_basic() {
    run_fixture("class_basic");
}

#[test]
fn fixture_class_inheritance() {
    run_fixture("class_inheritance");
}

#[test]
fn fixture_operator_overload() {
    run_fixture("operator_overload");
}

#[test]
fn fixture_class_virtual_dispatch() {
    run_fixture("class_virtual_dispatch");
}

#[test]
fn fixture_class_extras() {
    run_fixture("class_extras");
}


#[test]
#[ignore = "requires a display; run manually with: cargo test fixture_ui_window -- --ignored"]
fn fixture_ui_window() {
    run_fixture("ui_window");
}

#[test]
fn fixture_property_init_basic() {
    run_fixture("property_init_basic");
}

#[test]
fn fixture_property_init_no_ctor() {
    run_fixture("property_init_no_ctor");
}

#[test]
fn fixture_property_init_override() {
    run_fixture("property_init_override");
}

#[test]
fn fixture_property_init_extends() {
    run_fixture("property_init_extends");
}

#[test]
fn fixture_property_init_expr() {
    run_fixture("property_init_expr");
}

#[test]
fn fixture_property_init_self_ref() {
    run_fixture("property_init_self_ref");
}

#[test]
fn fixture_property_init_types() {
    run_fixture("property_init_types");
}

#[test]
fn fixture_property_init_many() {
    run_fixture("property_init_many");
}

#[test]
fn fixture_property_init_fn_call() {
    run_fixture("property_init_fn_call");
}

#[test]
fn fixture_property_init_multi_instance() {
    run_fixture("property_init_multi_instance");
}

#[test]
fn fixture_property_init_inherited_access() {
    run_fixture("property_init_inherited_access");
}

#[test]
fn fixture_property_init_no_super_call() {
    run_fixture("property_init_no_super_call");
}

#[test]
fn fixture_super_field_read() {
    run_fixture("super_field_read");
}

#[test]
fn fixture_super_field_write() {
    run_fixture("super_field_write");
}

#[test]
fn fixture_super_field_getter() {
    run_fixture("super_field_getter");
}

#[test]
fn fixture_super_field_setter() {
    run_fixture("super_field_setter");
}

#[test]
fn fixture_private_field_basic() {
    run_fixture("private_field_basic");
}

#[test]
fn fixture_private_field_no_collision() {
    run_fixture("private_field_no_collision");
}

#[test]
fn fixture_private_field_other_instance() {
    run_fixture("private_field_other_instance");
}

#[test]
fn fixture_readonly_field_ctor() {
    run_fixture("readonly_field_ctor");
}

#[test]
fn fixture_private_field_cross_class_err() {
    run_fixture_expect_error(
        "private_field_cross_class_err",
        "private `#x` nao e visivel em `A`",
    );
}

#[test]
fn fixture_readonly_field_reassign_err() {
    run_fixture_expect_error(
        "readonly_field_reassign_err",
        "readonly `C.x` so pode ser atribuido",
    );
}

#[test]
fn fixture_computed_method_str() {
    run_fixture("computed_method_str");
}

#[test]
fn fixture_computed_property_str() {
    run_fixture("computed_property_str");
}

#[test]
fn fixture_computed_template() {
    run_fixture("computed_template");
}

#[test]
fn fixture_computed_accessor() {
    run_fixture("computed_accessor");
}

#[test]
fn fixture_abstract_class_basic() {
    run_fixture("abstract_class_basic");
}

#[test]
fn fixture_abstract_chain() {
    run_fixture("abstract_chain");
}

#[test]
fn fixture_abstract_class_no_new_err() {
    run_fixture_expect_error(
        "abstract_class_no_new_err",
        "classe abstract `Shape` nao pode ser instanciada",
    );
}

#[test]
fn fixture_abstract_missing_impl_err() {
    run_fixture_expect_error(
        "abstract_missing_impl_err",
        "classe concreta `Square` nao implementa",
    );
}

// Closure capturando `this`/`super` em callback de classe.
// Disparam o trampolim diretamente via `__class_C_lifted_arrow_N()` em
// vez de depender de evento UI — o nome mangled é estável porque o
// counter de arrows é resetado a cada compile (cada fixture é
// compilado isoladamente).
//
// Marcadas `#[ignore]` porque registrar callbacks via FLTK requer um
// display backend disponível (mesmo padrão de `fixture_ui_window`).
#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_this_field() {
    run_fixture("closure_this_field");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_this_method() {
    run_fixture("closure_this_method");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_super() {
    run_fixture("closure_super");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_nested() {
    run_fixture("closure_nested");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_per_instance() {
    run_fixture("closure_per_instance");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_local_capture() {
    run_fixture("closure_local_capture");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_local_multi() {
    run_fixture("closure_local_multi");
}

#[test]
#[ignore = "requires a display; run manually with: cargo test closure_ -- --ignored"]
fn fixture_closure_local_param() {
    run_fixture("closure_local_param");
}

#[test]
fn fixture_private_modifier_basic() {
    run_fixture("private_modifier_basic");
}

#[test]
fn fixture_protected_modifier_basic() {
    run_fixture("protected_modifier_basic");
}

#[test]
fn fixture_private_modifier_err() {
    run_fixture_expect_error(
        "private_modifier_err",
        "membro `secret` é private em `C`",
    );
}

#[test]
fn fixture_protected_modifier_err() {
    run_fixture_expect_error(
        "protected_modifier_err",
        "membro `y` é protected em `Base`",
    );
}

#[test]
fn fixture_private_method_err() {
    run_fixture_expect_error(
        "private_method_err",
        "membro `secret` é private em `C`",
    );
}

#[test]
fn fixture_default_param_basic() {
    run_fixture("default_param_basic");
}

#[test]
fn fixture_default_param_multi() {
    run_fixture("default_param_multi");
}

#[test]
fn fixture_default_param_expr() {
    run_fixture("default_param_expr");
}

#[test]
fn fixture_default_param_string() {
    run_fixture("default_param_string");
}

#[test]
fn fixture_type_assertion_basic() {
    run_fixture("type_assertion_basic");
}

#[test]
fn fixture_type_assertion_class() {
    run_fixture("type_assertion_class");
}

#[test]
fn fixture_type_assertion_misc() {
    run_fixture("type_assertion_misc");
}

#[test]
fn fixture_type_assertion_member() {
    run_fixture("type_assertion_member");
}

#[test]
fn fixture_enum_basic() {
    run_fixture("enum_basic");
}

#[test]
fn fixture_enum_explicit() {
    run_fixture("enum_explicit");
}

#[test]
fn fixture_enum_compare() {
    run_fixture("enum_compare");
}

#[test]
fn fixture_try_catch_no_binding() {
    run_fixture("try_catch_no_binding");
}

#[test]
fn fixture_try_catch_propagation() {
    run_fixture("try_catch_propagation");
}

#[test]
fn fixture_try_catch_nested() {
    run_fixture("try_catch_nested");
}

#[test]
fn fixture_try_catch_rethrow() {
    run_fixture("try_catch_rethrow");
}

#[test]
fn fixture_union_param() {
    run_fixture("union_param");
}

#[test]
fn fixture_intersection_type() {
    run_fixture("intersection_type");
}

#[test]
fn fixture_union_var() {
    run_fixture("union_var");
}

#[test]
fn fixture_union_null() {
    run_fixture("union_null");
}

#[test]
fn fixture_for_in_basic() {
    run_fixture("for_in_basic");
}

#[test]
fn fixture_for_in_values() {
    run_fixture("for_in_values");
}

#[test]
fn fixture_for_in_empty() {
    run_fixture("for_in_empty");
}

#[test]
fn fixture_for_in_break() {
    run_fixture("for_in_break");
}

#[test]
fn fixture_rest_param_basic() {
    run_fixture("rest_param_basic");
}

#[test]
fn fixture_rest_param_mixed() {
    run_fixture("rest_param_mixed");
}

#[test]
fn fixture_rest_param_empty() {
    run_fixture("rest_param_empty");
}

#[test]
fn fixture_spread_call_basic() {
    run_fixture("spread_call_basic");
}

#[test]
fn fixture_spread_call_mixed() {
    run_fixture("spread_call_mixed");
}

#[test]
fn fixture_spread_with_rest() {
    run_fixture("spread_with_rest");
}

#[test]
fn fixture_labeled_break() {
    run_fixture("labeled_break");
}

#[test]
fn fixture_labeled_continue() {
    run_fixture("labeled_continue");
}

#[test]
fn fixture_labeled_nested() {
    run_fixture("labeled_nested");
}

#[test]
fn fixture_satisfies_op() {
    run_fixture("satisfies_op");
}

#[test]
fn fixture_spread_new() {
    run_fixture("spread_new");
}

#[test]
fn fixture_spread_super() {
    run_fixture("spread_super");
}

#[test]
fn fixture_destruct_array() {
    run_fixture("destruct_array");
}

#[test]
fn fixture_destruct_object() {
    run_fixture("destruct_object");
}

#[test]
fn fixture_destruct_alias() {
    run_fixture("destruct_alias");
}

#[test]
fn fixture_destruct_in_fn() {
    run_fixture("destruct_in_fn");
}

#[test]
fn fixture_async_basic() {
    run_fixture("async_basic");
}

#[test]
fn fixture_async_chain() {
    run_fixture("async_chain");
}

#[test]
fn fixture_async_arrow() {
    run_fixture("async_arrow");
}

#[test]
fn fixture_async_class() {
    run_fixture("async_class");
}
