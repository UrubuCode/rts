// Cross-runtime: in operator for property existence checks.
const obj = { a: 1, b: undefined, c: null };
console.log("a" in obj);
console.log("b" in obj);   // true — key exists even if value is undefined
console.log("c" in obj);
console.log("d" in obj);

// in with arrays (checks index as key)
const arr = [10, 20, 30];
console.log(0 in arr);
console.log(2 in arr);
console.log(3 in arr);
console.log("length" in arr);

// in with class instances
class Foo {
  x = 1;
  method() {}
}
const f = new Foo();
console.log("x" in f);
console.log("method" in f);
console.log("toString" in f);  // inherited from Object.prototype

// private field in check (ES2022)
class HasPrivate {
  #value = 42;
  static check(o: any) { return #value in o; }
}
console.log(HasPrivate.check(new HasPrivate()));
console.log(HasPrivate.check({}));
