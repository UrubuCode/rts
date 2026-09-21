//! A non-numeric literal operand is not guarded, and a numeric one still is.
//!
//! `tests/literal_operand_needs_no_guard.test.ts` pins what these operators
//! ANSWER, and passes on a binary from before the change — which is what a
//! semantics test has to do, and also why it cannot see whether the guard was
//! emitted. A change that guarded everything again would pass it.
//!
//! So this counts `Guard` terminators in the emitted IR. Both directions
//! matter: the numeric row is what says the speculation was narrowed rather
//! than switched off, which is the way this would break and still look fixed.

/// How many `Guard` terminators the named function's IR holds.
///
/// By function, because the entry block of a program carries guards of its own
/// — reading `console`, calling it — and counting the whole dump would measure
/// those instead.
fn guards_in(source: &str, function: &str) -> usize {
    let ir = rts_host::describe::describe_source(source).expect("compiles");
    let head = format!("; FuncId(");
    let mut inside = false;
    let mut found = 0;
    for line in ir.lines() {
        if line.starts_with(&head) {
            inside = line.ends_with(function);
            continue;
        }
        if inside && line.trim_start().starts_with("Guard {") {
            found += 1;
        }
    }
    found
}

/// `<literal> + i`, where `i` is a parameter and therefore arrives tagged.
fn guards_for(literal: &str) -> usize {
    let source = format!("function f(i) {{ return {literal} + i; }}\nconsole.log(f(1));\n");
    guards_in(&source, "f")
}

#[test]
fn a_number_literal_still_guards_the_other_operand() {
    // The speculation this pass exists for: one operand is proven by being
    // written, the other is asked about. If this reaches zero the guarded form
    // was turned off rather than narrowed, and every arithmetic in the language
    // became a call.
    assert_eq!(guards_for("5"), 1);
    assert_eq!(guards_for("0.5"), 1);
}

#[test]
fn a_literal_that_is_never_a_double_is_not_asked_whether_it_is_one() {
    for literal in ["\"n\"", "true", "false", "null", "/x/", "1n"] {
        assert_eq!(
            guards_for(literal),
            0,
            "`{literal} + i` cannot take the numeric instruction, so neither operand needs narrowing"
        );
    }
}

#[test]
fn undefined_is_a_name_and_is_still_guarded() {
    // `null` is a keyword and parses as a literal; `undefined` is an ordinary
    // global binding that a parameter may shadow — `function f(undefined)` is
    // legal — so nothing syntactic settles what it holds. It is guarded like
    // any other name, and that is the correct answer rather than a gap.
    //
    // Written down because the two look alike in a source file, and the first
    // version of the row above expected zero for it.
    assert!(guards_for("undefined") >= 1);
}

#[test]
fn it_is_the_literal_and_not_the_operator_that_settles_it() {
    // Every operator the guarded form covers, with a string on one side.
    for op in [
        "+", "-", "*", "/", "%", "<", "<=", ">", ">=", "==", "!=", "===", "!==",
    ] {
        let source = format!("function f(i) {{ return \"n\" {op} i; }}\nconsole.log(f(1));\n");
        assert_eq!(guards_in(&source, "f"), 0, "`\"n\" {op} i`");
    }
    // And the same operators with two unknowns, which is what the speculation
    // is for and must be left alone.
    for op in ["+", "-", "*", "<", "==="] {
        let source = format!("function f(i, j) {{ return i {op} j; }}\nconsole.log(f(1, 2));\n");
        assert!(
            guards_in(&source, "f") >= 2,
            "`i {op} j` speculates on both operands"
        );
    }
}

#[test]
fn a_name_the_program_annotates_is_a_claim_and_not_a_literal() {
    // Rule 4: an annotation is evidence, and evidence may choose between two
    // legal emissions. It is NOT what this change rests on — `"n"` needs no
    // annotation to be a string — and the two must not be confused, because a
    // `string` that really holds a number is a program this still has to run.
    let claimed = "function f(s: string, i: number) { return s + i; }\nconsole.log(f(\"a\", 1));\n";
    assert!(
        guards_in(claimed, "f") >= 1,
        "a claimed string is still asked, because a claim can be wrong"
    );
}
