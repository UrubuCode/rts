//! A top-level function is registered for the pickle where it is made, not in
//! a pass of its own.
//!
//! Counted off the EMITTED IR, because the defect this pins is a property of
//! the order the instructions are emitted in and nothing else: a registration
//! pass that ran after the hoist read every hoisted closure back, so N SSA
//! values were live across N runtime calls and the machine's register
//! allocation went quadratic — 9.2 s to compile a script of 4 000 top-level
//! functions against 0.35 s once each closure is registered beside its own
//! creation (fast profile, 2026-09-18, medians of five). A timing test would
//! see that and also fail on a busy machine; the order cannot.

/// The calls of the whole program, in emission order, named by their callee.
fn calls(source: &str) -> Vec<&'static str> {
    let ir = rts_host::describe::describe_source(source).expect("compiles");
    let id_of = |symbol: &str| -> Option<String> {
        ir.lines()
            .find(|line| line.starts_with(';') && line.ends_with(symbol))
            .and_then(|line| line.split_whitespace().nth(1))
            .map(str::to_owned)
    };
    let closure = id_of("__rts_closure_new").expect("a program with functions makes closures");
    let declare = id_of("__rts_serde_declare").expect("a script registers its top-level functions");
    ir.lines()
        .filter_map(|line| {
            let callee = line.split("Call { callee: ").nth(1)?.split(',').next()?;
            match callee {
                _ if callee == closure => Some("closure"),
                _ if callee == declare => Some("declare"),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn each_top_level_function_is_registered_before_the_next_is_made() {
    // The import is what makes the program register at all — a program with
    // no route to the pickle emits no registration (`serde_declare_gate.rs`).
    let mut source = String::from("import { serialize } from \"rts:serde\";\n");
    source.extend((0..8).map(|i| format!("function f{i}(x) {{ return x + {i}; }}\n")));
    let sequence = calls(&source);
    assert_eq!(
        sequence.len(),
        16,
        "one closure and one registration per function: {sequence:?}"
    );
    for pair in sequence.chunks(2) {
        assert_eq!(
            pair,
            ["closure", "declare"],
            "a closure is registered where it is made, so no hoisted value stays live \
             across the registrations of the functions after it: {sequence:?}"
        );
    }
}
