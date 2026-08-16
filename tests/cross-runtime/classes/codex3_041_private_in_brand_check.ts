// Cross-runtime: private-in checks an object's class brand without reading the field.
class Box {
  #value = 1;
  static has(value: any) { return #value in value; }
}
const box = new Box();
console.log(Box.has(box), Box.has({}), Box.has(Object.create(Box.prototype)));
console.log(Box.has(new Proxy(box, {})));

