// Cross-runtime: Symbol.toStringTag customizes Object.prototype.toString.
const obj: any = {};
console.log(Object.prototype.toString.call(obj));
obj[Symbol.toStringTag] = "Custom";
console.log(Object.prototype.toString.call(obj));

class Box {
  get [Symbol.toStringTag]() {
    return "Box";
  }
}
console.log(Object.prototype.toString.call(new Box()));
