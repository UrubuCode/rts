// Cross-runtime: super setter writes through the derived instance receiver.
class Base {
  set doubled(v: number) { (this as any).stored = v * 2; }
}
class Child extends Base {
  write(v: number) { super.doubled = v; }
}
const c: any = new Child();
c.write(6);
console.log(c.stored);
console.log(Object.hasOwn(c, "doubled"), Object.hasOwn(c, "stored"));

