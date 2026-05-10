// Cross-runtime: private methods and inheritance separation.
class A {
  #m(v: number) { return v + 1; }
  call(v: number) { return this.#m(v); }
}
class B extends A {
  #m(v: number) { return v + 10; }
  both(v: number) { return this.call(v) + ":" + this.#m(v); }
}
const b1 = new B();
let err = "";
try { eval("b1.#m(1)"); } catch (e: any) { err = e.name; }
console.log("both=" + b1.both(5));
console.log("err=" + err);
