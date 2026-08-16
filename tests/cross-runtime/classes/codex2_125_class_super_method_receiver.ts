// Cross-runtime: super method calls retain the derived instance receiver.
class Base {
  prefix = "B";
  label(x: string) { return this.prefix + x; }
}
class Child extends Base {
  prefix = "C";
  label(x: string) { return super.label(x) + "!"; }
}
console.log(new Child().label("x"));

