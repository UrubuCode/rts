// Cross-runtime: a derived constructor may return an explicit object without calling super.
class Base { marker = "base"; }
class Child extends Base {
  constructor() {
    return { marker: "replacement", own: true } as any;
  }
}
const value: any = new Child();
console.log(value.marker, value.own);
console.log(value instanceof Child, value instanceof Base);

