// Cross-runtime: a class has a non-writable name and length descriptor shape.
class Named {
  constructor(a: any, b: any = 2) {}
}
const name = Object.getOwnPropertyDescriptor(Named, "name")!;
const length = Object.getOwnPropertyDescriptor(Named, "length")!;
console.log(Named.name, Named.length);
console.log(name.writable, name.enumerable, name.configurable);
console.log(length.writable, length.enumerable, length.configurable);

