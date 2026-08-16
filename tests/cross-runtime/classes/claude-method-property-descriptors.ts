// Cross-runtime: class methods land on the prototype non-enumerable, writable
// and configurable; accessors carry "get "/"set " names; the constructor's own
// properties are prototype/length/name with fixed attributes.
class Shape {
  side: number = 3;

  area(a: number, b: number): number {
    return a * b;
  }
  get width(): number {
    return this.side;
  }
  set width(v: number) {
    this.side = v;
  }
  static make(n: number): Shape {
    const s = new Shape();
    s.side = n;
    return s;
  }
  *rows(): Generator<number> {
    yield this.side;
  }
}

function desc(o: any, k: string): string {
  const d = Object.getOwnPropertyDescriptor(o, k);
  if (d === undefined) return "none";
  const kind = "get" in d && (d.get !== undefined || d.set !== undefined) ? "accessor" : "data";
  return (
    kind +
    ",e=" + String(d.enumerable) +
    ",c=" + String(d.configurable) +
    (kind === "data" ? ",w=" + String(d.writable) : ",g=" + String(d.get !== undefined) + ",s=" + String(d.set !== undefined))
  );
}

const P: any = Shape.prototype;
console.log("proto-area=" + desc(P, "area"));
console.log("proto-width=" + desc(P, "width"));
console.log("proto-rows=" + desc(P, "rows"));
console.log("proto-ctor=" + desc(P, "constructor"));
console.log("static-make=" + desc(Shape, "make"));
console.log("ctor-prototype=" + desc(Shape, "prototype"));
console.log("ctor-length=" + desc(Shape, "length"));
console.log("ctor-name=" + desc(Shape, "name"));

console.log("proto-names=" + Object.getOwnPropertyNames(P).sort().join(","));
console.log("proto-keys=" + Object.keys(P).join(","));
console.log("ctor-names=" + Object.getOwnPropertyNames(Shape).sort().join(","));

const s = new Shape();
console.log("inst-keys=" + Object.keys(s).join(","));
console.log("inst-names=" + Object.getOwnPropertyNames(s).join(","));
console.log("inst-json=" + JSON.stringify(s));

console.log("area-name=" + Shape.prototype.area.name);
console.log("area-length=" + Shape.prototype.area.length);
console.log("make-name=" + Shape.make.name);
console.log("ctor-name-value=" + Shape.name);
console.log("ctor-length-value=" + Shape.length);

const wd: any = Object.getOwnPropertyDescriptor(P, "width");
console.log("getter-name=" + wd.get.name);
console.log("setter-name=" + wd.set.name);
console.log("getter-length=" + wd.get.length);
console.log("setter-length=" + wd.set.length);

// Methods are not constructors; generators are not either.
try {
  new (Shape.prototype.area as any)();
  console.log("new-method=no-throw");
} catch (e: any) {
  console.log("new-method=" + e.constructor.name);
}
try {
  new (wd.get as any)();
  console.log("new-getter=" + "no-throw");
} catch (e: any) {
  console.log("new-getter=" + e.constructor.name);
}
console.log("method-has-prototype=" + ("prototype" in (Shape.prototype.area as any)));
console.log("generator-has-prototype=" + ("prototype" in (Shape.prototype.rows as any)));

// The class constructor itself refuses a plain call.
try {
  (Shape as any)();
  console.log("call-class=no-throw");
} catch (e: any) {
  console.log("call-class=" + e.constructor.name);
}

// for-in walks the chain but sees no method, because none is enumerable.
const forIn: string[] = [];
for (const k in s) forIn.push(k);
console.log("for-in=" + forIn.join(","));

console.log("proto-of-static=" + (Object.getPrototypeOf(Shape) === Function.prototype));
console.log("proto-tostring=" + Object.prototype.toString.call(P));
