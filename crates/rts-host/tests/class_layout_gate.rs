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

#[test]
fn asking_whether_something_is_one_does_not_cost_the_class_its_layout() {
    let source = format!(
        "{CLASSES}
const kept = [];
function f(n) {{ let s = 0; for (let i = 0; i < n; i++) {{ const o = new D(i, 2); s += o.sum(); kept.push(new B(i)); }} return s; }}
console.log(f(3), kept[0] instanceof B, kept[0] instanceof D);"
    );
    assert_eq!(calls_to(&source, "__rts_construct"), 0, "no construction");
    assert_eq!(calls_to(&source, "__rts_object_new_under"), 1, "the one that is kept");
}

#[test]
fn a_class_that_answers_instanceof_itself_is_constructed_as_it_was() {
    let source = "
class H { a; constructor(a) { this.a = a; } static [Symbol.hasInstance](x) { return true; } }
function f(n) { let s = 0; for (let i = 0; i < n; i++) { const o = new H(i); s += o.a; } return s; }
console.log(f(3), 1 instanceof H);";
    assert_eq!(calls_to(source, "__rts_construct"), 1, "the class is not this pass's");
}

#[test]
fn a_class_declared_inside_the_function_that_uses_it_has_a_layout_too() {
    let source = "
const kept = [];
function f(n) {
  class Inner { a; constructor(a) { this.a = a; } twice() { return this.a * 2; } }
  let s = 0;
  for (let i = 0; i < n; i++) { const o = new Inner(i); s += o.twice(); kept.push(new Inner(i)); }
  return s;
}
console.log(f(3), kept.length);";
    assert_eq!(calls_to(source, "__rts_construct"), 0, "no construction");
    assert_eq!(calls_to(source, "__rts_object_new_under"), 1, "the one that is kept");
}

#[test]
fn two_functions_that_each_declare_a_class_of_one_name_both_have_its_layout() {
    let source = "
function g(n) { class P { x = 1; } let s = 0; for (let i = 0; i < n; i++) { const o = new P(); s += o.x; } return s; }
function f(n) {
  class P { a; b; constructor(a, b) { this.a = a; this.b = b; } sum() { return this.a + this.b; } }
  let s = 0;
  for (let i = 0; i < n; i++) { const o = new P(i, 2); s += o.sum(); }
  return s;
}
console.log(f(3), g(3));";
    assert_eq!(calls_to(source, "__rts_construct"), 0, "no construction");
    assert_eq!(calls_to(source, "__rts_object_new_under"), 0, "and no object");
}

#[test]
fn a_class_in_an_inner_block_does_not_lend_its_layout_to_the_name_outside_it() {
    let source = "
class P { v; constructor(v) { this.v = v; } }
function f(n) {
  let s = 0;
  { class P { w = 9; } const inner = new P(); s += inner.w; }
  for (let i = 0; i < n; i++) { const o = new P(i); s += o.v; }
  return s;
}
console.log(f(3));";
    assert_eq!(calls_to(source, "__rts_construct"), 2, "both are constructed as written");
}

#[test]
fn a_method_that_writes_its_fields_and_one_that_asks_math_are_neither_a_call() {
    let source = "
class V { x; y; n = 0; constructor(x, y) { this.x = x; this.y = y; }
  length() { return Math.sqrt(this.x * this.x + this.y * this.y); }
  scale(k) { this.x = this.x * k; this.y *= k; }
  bump() { this.n += 1; return this.n; } }
function f(n) {
  let s = 0;
  for (let i = 0; i < n; i++) { const v = new V(i, 4); v.scale(2); s += v.length() + v.bump(); }
  return s;
}
console.log(f(3));";
    assert_eq!(calls_to(source, "__rts_construct"), 0, "no construction");
    assert_eq!(calls_to(source, "__rts_object_new_under"), 0, "no object");
    assert_eq!(calls_to(source, "__rts_call_counted"), 0, "and no call");
}

#[test]
fn a_class_declared_in_another_module_has_its_layout_where_it_is_constructed() {
    let dir = std::env::temp_dir().join(format!("rts-class-layout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    std::fs::write(
        dir.join("shape.ts"),
        "export class Shape { id; constructor(id) { this.id = id; } twice() { return this.id * 2; } }\n\
         export class Rect extends Shape { w; constructor(id, w) { super(id); this.w = w; } area() { return this.w * this.w; } }\n",
    )
    .expect("written");
    let entry = dir.join("main.ts");
    std::fs::write(
        &entry,
        "import { Rect } from \"./shape\";\n\
         const kept = [];\n\
         function f(n) { let s = 0; for (let i = 0; i < n; i++) { const r = new Rect(i, 2); s += r.area() + r.twice(); } kept.push(new Rect(1, 1)); return s; }\n\
         console.log(f(3), kept.length);\n",
    )
    .expect("written");
    let ir = rts_host::describe::describe_path(&entry).expect("compiles");
    let _ = std::fs::remove_dir_all(&dir);
    let id_of = |symbol: &str| {
        ir.lines()
            .find(|line| line.starts_with(';') && line.ends_with(symbol))
            .and_then(|line| line.split_whitespace().nth(1))
            .map(str::to_owned)
    };
    let in_f = |symbol: &str| -> usize {
        let Some(id) = id_of(symbol) else { return 0 };
        ir.lines()
            .skip_while(|line| !(line.starts_with("; FuncId(") && line.ends_with(" f")))
            .skip(1)
            .take_while(|line| !line.starts_with("; FuncId("))
            .filter_map(|line| line.split("Call { callee: ").nth(1)?.split(',').next())
            .filter(|callee| *callee == id)
            .count()
    };
    assert_eq!(in_f("__rts_construct"), 0, "no construction across the boundary");
    assert_eq!(in_f("__rts_object_new_under"), 1, "the one that is kept");
}
