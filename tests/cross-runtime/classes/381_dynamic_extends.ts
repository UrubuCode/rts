// Cross-runtime: extends with expression (dynamic base class).

function Timestamped<T extends new (...a: any[]) => {}>(Base: T) {
  return class extends Base {
    readonly createdAt = new Date(0).toISOString();  // deterministic
  };
}

function Tagged<T extends new (...a: any[]) => {}>(tag: string) {
  return (Base: T) => class extends Base {
    readonly tag = tag;
  };
}

class User { constructor(public name: string) {} }

const TimestampedUser = Timestamped(User);
const u = new TimestampedUser("Alice");
console.log(u.name);
console.log(typeof u.createdAt);   // string
console.log(u instanceof User);    // true

// chained dynamic base
const AdminUser = Tagged<typeof TimestampedUser>("admin")(TimestampedUser);
const admin = new AdminUser("Bob");
console.log(admin.name);
console.log(admin.tag);
console.log(admin instanceof User);  // true

// base class chosen at runtime
function createClass(shape: "circle" | "square") {
  const Base = shape === "circle"
    ? class { describe() { return "circle"; } }
    : class { describe() { return "square"; } };
  return class extends Base {
    area(size: number) { return shape === "circle" ? Math.PI * size * size : size * size; }
  };
}
const Circle = createClass("circle");
const Square = createClass("square");
const c = new Circle();
const s = new Square();
console.log(c.describe());
console.log(Math.round(c.area(1) * 100) / 100);  // 3.14
console.log(s.describe());
console.log(s.area(4));  // 16

// null extends (Object.create(null)-like via class)
const NullBase = null as unknown as new () => {};
class PureDict extends (class {} as any) {
  #data = Object.create(null) as Record<string, string>;
  set(k: string, v: string) { this.#data[k] = v; }
  get(k: string) { return this.#data[k]; }
  has(k: string) { return k in this.#data; }
}
const d = new PureDict();
d.set("foo", "bar");
console.log(d.get("foo"));   // bar
console.log(d.has("foo"));   // true
console.log(d.has("toString"));  // false — no prototype chain
