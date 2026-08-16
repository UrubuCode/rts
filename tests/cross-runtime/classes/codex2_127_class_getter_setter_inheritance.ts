// Cross-runtime: inherited accessors operate on derived instance fields.
class Base {
  get doubled() { return (this as any).value * 2; }
  set doubled(v: number) { (this as any).value = v / 2; }
}
class Child extends Base { value = 3; }
const c = new Child();
console.log(c.doubled);
c.doubled = 20;
console.log(c.value, c.doubled);

