// Cross-runtime: overridden methods dispatch dynamically through base calls.
class Base {
  value() { return 2; }
  describe() { return "base:" + this.value(); }
}
class Child extends Base {
  value() { return 5; }
}
const c = new Child();
console.log(c.value(), c.describe());
console.log(c instanceof Child, c instanceof Base);

