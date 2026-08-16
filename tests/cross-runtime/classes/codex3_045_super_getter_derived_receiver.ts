// Cross-runtime: super getter lookup uses the derived instance as receiver.
class Base {
  get total() { return (this as any).a + (this as any).b; }
}
class Child extends Base {
  a = 2;
  b = 5;
  read() { return super.total; }
}
console.log(new Child().read());

