// Cross-runtime: a metadata registry combines WeakMap identity with prototype-chain lookup.
const metadata = new WeakMap<object, Map<string, any>>();
function define(target: object, key: string, value: any) {
  let map = metadata.get(target);
  if (!map) { map = new Map(); metadata.set(target, map); }
  map.set(key, value);
}
function read(target: object, key: string): any {
  for (let cursor: any = target; cursor; cursor = Object.getPrototypeOf(cursor)) {
    const map = metadata.get(cursor);
    if (map?.has(key)) return map.get(key);
  }
}
class Base {}
class Child extends Base {}
define(Base.prototype, "role", "base");
define(Child.prototype, "own", 7);
console.log(read(Child.prototype, "role"), read(Child.prototype, "own"));
console.log(read(new Child(), "role"), read(new Base(), "own"));

