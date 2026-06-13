//! P4.9: user CLASSES without inheritance — fields, constructor, instance
//! methods, `new C(args)`, `this.field`, `instance.method(args)`, and the
//! out-of-subset bails (extends / static / unknown-class receiver).

use super::{assert_bails, assert_stdout};

#[test]
fn field_and_method() {
    assert_stdout(
        "class P { x: number; constructor(x: number){ this.x = x; } get2(){ return this.x * 2; } } \
         let p = new P(5); console.log(p.get2());",
        "10\n",
    );
}

#[test]
fn field_read() {
    assert_stdout(
        "class C { v: number; constructor(v: number){ this.v = v; } } \
         let c = new C(7); console.log(c.v);",
        "7\n",
    );
}

#[test]
fn multiple_fields_method() {
    assert_stdout(
        "class Pt { x: number; y: number; constructor(x:number,y:number){ this.x=x; this.y=y; } \
         sum(){ return this.x + this.y; } } let q = new Pt(3,4); console.log(q.sum());",
        "7\n",
    );
}

#[test]
fn method_calls_method_via_this() {
    assert_stdout(
        "class A { n: number; constructor(n:number){this.n=n;} dbl(){return this.n*2;} \
         quad(){return this.dbl()*2;} } console.log(new A(5).quad());",
        "20\n",
    );
}

#[test]
fn method_with_args() {
    assert_stdout(
        "class Acc { t: number; constructor(){this.t=0;} add(v:number){ this.t = this.t + v; return this.t; } } \
         let a=new Acc(); a.add(3); console.log(a.add(4));",
        "7\n",
    );
}

#[test]
fn string_field() {
    assert_stdout(
        r#"class N { name: string; constructor(n:string){this.name=n;} greet(){ return "hi " + this.name; } } console.log(new N("rts").greet());"#,
        "hi rts\n",
    );
}

#[test]
fn console_log_instance() {
    assert_stdout(
        "class B { a: number; b: number; constructor(){this.a=1;this.b=2;} } console.log(new B());",
        "{ a: 1, b: 2 }\n",
    );
}

#[test]
fn field_without_property_decl() {
    // Field declared ONLY via `this.x = …` in the constructor (no `x: T;` line) —
    // still becomes an instance slot in first-assignment order.
    assert_stdout(
        "class K { constructor(){ this.v = 42; } get(){ return this.v; } } \
         let k = new K(); console.log(k.get());",
        "42\n",
    );
}

#[test]
fn property_initializer() {
    // `count: number = 10;` becomes a `this.count = 10` constructor prologue.
    assert_stdout(
        "class C { count: number = 10; bump(){ this.count = this.count + 1; return this.count; } } \
         let c = new C(); console.log(c.bump());",
        "11\n",
    );
}

#[test]
fn instance_in_loop() {
    assert_stdout(
        "class Counter { n: number; constructor(){this.n=0;} inc(){ this.n = this.n + 1; return this.n; } } \
         let c = new Counter(); let i = 0; while (i < 3) { console.log(c.inc()); i = i + 1; }",
        "1\n2\n3\n",
    );
}

// ===========================================================================
// P4.9 negative: out-of-subset class features BAIL (never a wrong value).
// ===========================================================================

#[test]
fn extends_bails() {
    assert_bails("class B { a: number; constructor(){this.a=1;} } class D extends B {} let d = new D(); console.log(d.a);");
}

#[test]
fn static_member_bails() {
    assert_bails("class C { static n: number = 5; } console.log(C.n);");
}

#[test]
fn static_method_bails() {
    assert_bails("class C { static make(){ return 1; } } console.log(C.make());");
}

#[test]
fn getter_bails() {
    assert_bails("class C { x: number; constructor(){this.x=1;} get val(){ return this.x; } } let c = new C(); console.log(c.val);");
}

#[test]
fn method_on_unknown_class_param_bails() {
    // `o` is a param of unknown class — a method call on it needs dynamic dispatch
    // (a later increment). Must bail, not guess a class.
    assert_bails(
        "function f(o: any){ return o.get(); } \
         class C { v: number; constructor(){this.v=3;} get(){return this.v;} } \
         console.log(f(new C()));",
    );
}

#[test]
fn unknown_method_on_known_class_bails() {
    assert_bails(
        "class C { v: number; constructor(){this.v=1;} } let c = new C(); console.log(c.nope());",
    );
}

#[test]
fn new_of_unknown_class_bails() {
    // `Foo` is not a user class in the program.
    assert_bails("let x = new Foo(1); console.log(x);");
}
