// Cross-runtime: super in a static method uses the derived constructor receiver.
class Base {
  static value = 2;
  static calc(n: number) { return this.value * n; }
}
class Child extends Base {
  static value = 5;
  static calc(n: number) { return super.calc(n) + 1; }
}
console.log(Child.calc(3), Base.calc(3));

