// Cross-runtime: inherited static methods cannot read base private static state through derived this.
class Base {
  static #value = 7;
  static read() { return this.#value; }
}
class Child extends Base {}
console.log(Base.read());
let threw = false;
try { Child.read(); } catch (e) { threw = e instanceof TypeError; }
console.log(threw);

