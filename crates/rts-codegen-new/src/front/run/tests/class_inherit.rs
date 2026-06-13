//! P5.1: class INHERITANCE (`extends` / `super`), getters/setters, and static
//! members — flattened instance fields, parent-first super dispatch, accessor
//! reads/writes, static method/field resolution. Out-of-subset shapes still BAIL.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// extends + super + inherited/overridden methods
// ===========================================================================

#[test]
fn extends_inherited_method() {
    assert_stdout(
        "class A { x:number; constructor(x:number){this.x=x;} getX(){return this.x;} } \
         class B extends A { constructor(x:number){ super(x); } } \
         console.log(new B(7).getX());",
        "7\n",
    );
}

#[test]
fn super_plus_own_field_override() {
    assert_stdout(
        r#"class Animal { name:string; constructor(n:string){this.name=n;} speak(){return this.name+" makes a sound";} } class Dog extends Animal { constructor(n:string){ super(n); } speak(){ return this.name+" barks"; } } console.log(new Dog("Rex").speak());"#,
        "Rex barks\n",
    );
}

#[test]
fn super_method_call() {
    assert_stdout(
        r#"class A { greet(){ return "A"; } } class B extends A { greet(){ return super.greet() + "B"; } } console.log(new B().greet());"#,
        "AB\n",
    );
}

#[test]
fn inherited_plus_own_fields() {
    assert_stdout(
        "class A { a:number; constructor(a:number){this.a=a;} } \
         class B extends A { b:number; constructor(a:number,b:number){ super(a); this.b=b; } \
         sum(){ return this.a+this.b; } } \
         console.log(new B(3,4).sum());",
        "7\n",
    );
}

#[test]
fn super_method_with_args() {
    assert_stdout(
        "class A { add(x:number){ return x + 1; } } \
         class B extends A { add(x:number){ return super.add(x) * 10; } } \
         console.log(new B().add(2));",
        "30\n",
    );
}

#[test]
fn implicit_subclass_ctor_forwards_super() {
    // B declares NO constructor: a forwarding one is synthesized that calls
    // super(...sameArgs).
    assert_stdout(
        "class A { v:number; constructor(v:number){this.v=v;} get(){return this.v;} } \
         class B extends A {} \
         console.log(new B(9).get());",
        "9\n",
    );
}

#[test]
fn three_level_chain() {
    assert_stdout(
        "class A { a:number; constructor(){this.a=1;} } \
         class B extends A { b:number; constructor(){ super(); this.b=2; } } \
         class C extends B { c:number; constructor(){ super(); this.c=3; } \
         total(){ return this.a + this.b + this.c; } } \
         console.log(new C().total());",
        "6\n",
    );
}

#[test]
fn inherited_field_console_log() {
    // Instance inspect renders FLATTENED fields parent-first.
    assert_stdout(
        "class A { a:number; constructor(){this.a=1;} } \
         class B extends A { b:number; constructor(){ super(); this.b=2; } } \
         console.log(new B());",
        "{ a: 1, b: 2 }\n",
    );
}

// ===========================================================================
// getters / setters
// ===========================================================================

#[test]
fn getter() {
    assert_stdout(
        "class C { _v:number; constructor(v:number){this._v=v;} get v(){ return this._v*10; } } \
         console.log(new C(5).v);",
        "50\n",
    );
}

#[test]
fn setter_and_getter() {
    assert_stdout(
        "class C { _v:number; constructor(){this._v=0;} set v(x:number){ this._v=x; } get v(){ return this._v; } } \
         let c=new C(); c.v=9; console.log(c.v);",
        "9\n",
    );
}

#[test]
fn inherited_getter() {
    assert_stdout(
        "class A { _n:number; constructor(n:number){this._n=n;} get n(){ return this._n+100; } } \
         class B extends A { constructor(n:number){ super(n); } } \
         console.log(new B(5).n);",
        "105\n",
    );
}

// ===========================================================================
// static members
// ===========================================================================

#[test]
fn static_method() {
    assert_stdout(
        "class M { static add(a:number,b:number){ return a+b; } } console.log(M.add(3,4));",
        "7\n",
    );
}

#[test]
fn static_field_read() {
    assert_stdout("class K { static count:number = 5; } console.log(K.count);", "5\n");
}

#[test]
fn static_method_calls_static() {
    assert_stdout(
        "class M { static base(){ return 10; } static doubled(){ return M.base()*2; } } \
         console.log(M.doubled());",
        "20\n",
    );
}

// ===========================================================================
// negative: still-unmodeled shapes BAIL (never a wrong value)
// ===========================================================================

#[test]
fn unknown_parent_bails() {
    assert_bails("class D extends UnknownBase {} let d = new D(); console.log(d);");
}

#[test]
fn abstract_class_bails() {
    assert_bails("abstract class A { foo(){ return 1; } } class B extends A {} console.log(new B().foo());");
}

#[test]
fn private_field_bails() {
    assert_bails("class C { #secret:number; constructor(){this.#secret=1;} } let c = new C(); console.log(c);");
}

#[test]
fn field_and_accessor_clash_bails() {
    // A class cannot have both a field `x` and an accessor `x`.
    assert_bails(
        "class C { x:number; constructor(){this.x=1;} get x(){ return 2; } } console.log(new C().x);",
    );
}

#[test]
fn static_field_write_bails() {
    // A static-field WRITE is a later increment (read-only getter synthesis).
    assert_bails("class C { static n:number = 0; } C.n = 5; console.log(C.n);");
}

#[test]
fn this_in_static_method_bails() {
    assert_bails("class C { static go(){ return this.x; } } console.log(C.go());");
}
