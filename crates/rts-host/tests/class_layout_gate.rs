//! A class with a layout is not constructed, and the emitted IR says so.
//!
//! Counted off the EMITTED IR and not timed, because what this pins is whether
//! a rewrite happened at all — and a rewrite that silently stops happening
//! passes every test that asserts an answer. It did once, on the day it was
//! written: a flag read after the value it described had been moved out made
//! `class_layout::rewritten` hand back the body as written, the corpus and the
//! fixture stayed green, and the only thing that said otherwise was a row
//! reading 80 ns where it had read 4.

/// How many calls the function `f` makes to the runtime symbol `symbol`.
///
/// Of `f` and not of the program: the classes are still DEFINED, and a derived
/// constructor constructs its parent whether or not anything calls it.
fn calls_to(source: &str, symbol: &str) -> usize {
    let ir = rts_host::describe::describe_source(source).expect("compiles");
    let Some(id) = ir
        .lines()
        .find(|line| line.starts_with(';') && line.ends_with(symbol))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_owned)
    else {
        return 0;
    };
    ir.lines()
        .skip_while(|line| !(line.starts_with("; FuncId(") && line.ends_with(" f")))
        .skip(1)
        .take_while(|line| !line.starts_with("; FuncId("))
        .filter_map(|line| line.split("Call { callee: ").nth(1)?.split(',').next())
        .filter(|callee| *callee == id)
        .count()
}

const CLASSES: &str = "
class B { a; constructor(a) { this.a = a; } ga() { return this.a; } }
class D extends B { b; constructor(a, b) { super(a); this.b = b; } sum() { return this.a + this.b; } }
";

#[test]
fn an_instance_nothing_sees_is_neither_constructed_nor_allocated() {
    let source = format!(
        "{CLASSES}
function f(n) {{ let s = 0; for (let i = 0; i < n; i++) {{ const o = new D(i, 2); s += o.sum() + o.ga() + o.b; }} return s; }}
console.log(f(3));"
    );
    assert_eq!(calls_to(&source, "__rts_construct"), 0, "no construction");
    assert_eq!(calls_to(&source, "__rts_object_new_under"), 0, "and no object");
}

#[test]
fn an_instance_that_is_seen_is_born_and_not_constructed() {
    let source = format!(
        "{CLASSES}
const kept = [];
function f(n) {{ for (let i = 0; i < n; i++) {{ kept.push(new D(i, 2)); const b = new B(i); kept.push(b); }} return kept.length; }}
console.log(f(3));"
    );
    assert_eq!(calls_to(&source, "__rts_construct"), 0, "no construction");
    assert_eq!(
        calls_to(&source, "__rts_object_new_under"),
        2,
        "one object per `new`, born under its prototype"
    );
}

#[test]
fn a_class_read_as_a_value_is_constructed_as_it_was() {
    let source = format!(
        "{CLASSES}
const held = D;
function f(n) {{ let s = 0; for (let i = 0; i < n; i++) {{ const o = new D(i, 2); s += o.b; }} return s; }}
console.log(f(3), held.name);"
    );
    assert_eq!(calls_to(&source, "__rts_construct"), 1, "the class is not this pass's");
    assert_eq!(calls_to(&source, "__rts_object_new_under"), 0);
}
