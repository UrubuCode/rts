// Cross-runtime: derived constructors inherit static members through their prototype chain.
class Base {
  static value = 4;
  static read() { return this.value; }
}
class Child extends Base { static value = 9; }
console.log(Base.read(), Child.read());
console.log(Object.getPrototypeOf(Child) === Base);

