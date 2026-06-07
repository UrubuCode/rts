// Cross-runtime: Reflect.construct can choose a distinct newTarget prototype.
function Base(this: any, x: number) {
  this.x = x;
}
class Derived {}

const obj = Reflect.construct(Base, [7], Derived);
console.log(obj.x);
console.log(obj instanceof Base);
console.log(obj instanceof Derived);
console.log(Object.getPrototypeOf(obj) === Derived.prototype);
