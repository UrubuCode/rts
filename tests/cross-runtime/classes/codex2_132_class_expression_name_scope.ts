// Cross-runtime: a class expression name is visible inside but not outside.
const Outer = class Inner {
  static ownName() { return Inner.name; }
  make() { return new Inner(); }
};
const value = new Outer();
console.log(Outer.name, Outer.ownName());
console.log(value.make() instanceof Outer);
console.log(typeof (globalThis as any).Inner);

