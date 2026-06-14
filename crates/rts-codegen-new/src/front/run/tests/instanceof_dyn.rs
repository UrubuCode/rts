//! Dynamic `instanceof` on user classes — `x instanceof C` where `x` is an
//! OPAQUE value (a param / return / `any`) with no statically-known class. The
//! engine emits a runtime shape-id check (slot 0 of the instance), accepting `C`
//! plus every descendant of `C`. A non-object operand is `false` (never faults).

use super::assert_stdout;

#[test]
fn dynamic_instanceof_param() {
    assert_stdout(
        r#"class A {}
           class B {}
           function isA(x: any): boolean { return x instanceof A; }
           console.log(isA(new A()), isA(new B()), isA(5), isA("s"));"#,
        "true false false false\n",
    );
}

#[test]
fn dynamic_instanceof_subclass() {
    assert_stdout(
        r#"class Animal {}
           class Dog extends Animal {}
           function check(x: any): boolean { return x instanceof Animal; }
           console.log(check(new Dog()), check(new Animal()));"#,
        "true true\n",
    );
}

#[test]
fn dynamic_instanceof_not_subclass() {
    assert_stdout(
        r#"class Animal {}
           class Dog extends Animal {}
           function isDog(x: any): boolean { return x instanceof Dog; }
           console.log(isDog(new Animal()), isDog(new Dog()));"#,
        "false true\n",
    );
}
