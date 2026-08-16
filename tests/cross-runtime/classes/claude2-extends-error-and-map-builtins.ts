// Cross-runtime: extending the built-in Error and Map. The subclass instance
// gets its prototype from new.target, the exotic internals (message, the Map
// data list) still work, an overridden method is what the built-in calls back
// into, and `constructor` on the prototype names the subclass.
class AppError extends Error {
  code: number = 0;
  constructor(message: string, code: number) {
    super(message);
    this.name = "AppError";
    this.code = code;
  }
  describe(): string {
    return this.name + "/" + this.code + "/" + this.message;
  }
}

class FatalError extends AppError {
  constructor(message: string) {
    super(message, 99);
  }
}

const err = new AppError("boom", 7);
console.log("err-describe=" + err.describe());
console.log("err-message=" + err.message);
console.log("err-instanceof=" + (err instanceof AppError) + "," + (err instanceof Error));
console.log("err-proto=" + (Object.getPrototypeOf(err) === AppError.prototype));
console.log("err-proto-chain=" + (Object.getPrototypeOf(AppError.prototype) === Error.prototype));
console.log("err-static-chain=" + (Object.getPrototypeOf(AppError) === Error));
console.log("err-tag=" + Object.prototype.toString.call(err));
console.log("err-tostring=" + err.toString());
console.log("err-keys=" + Object.keys(err).sort().join(","));
console.log("err-own-message=" + Object.prototype.hasOwnProperty.call(err, "message"));
console.log("err-own-name=" + Object.prototype.hasOwnProperty.call(err, "name"));
console.log("err-proto-ctor=" + (AppError.prototype.constructor === AppError));

const fatal = new FatalError("down");
console.log("fatal-describe=" + fatal.describe());
console.log("fatal-code=" + fatal.code);
console.log("fatal-instanceof=" + (fatal instanceof AppError) + "," + (fatal instanceof Error));
console.log("fatal-proto=" + (Object.getPrototypeOf(fatal) === FatalError.prototype));

// The subclass survives being thrown and caught like any other error.
let caughtTag = "none";
try {
  throw new FatalError("thrown");
} catch (e: any) {
  caughtTag = e.constructor.name + ":" + e.code + ":" + (e instanceof Error);
}
console.log("caught=" + caughtTag);

// A Map subclass: the exotic data list belongs to the instance, and the
// built-in constructor's own iteration goes through the SUBCLASS `set`.
const setCalls: string[] = [];
class CountingMap<K, V> extends Map<K, V> {
  writes: number = 0;
  set(key: K, value: V): this {
    this.writes = (this.writes || 0) + 1;
    setCalls.push(String(key));
    return super.set(key, value);
  }
}

const cm = new CountingMap<string, number>([["a", 1], ["b", 2]]);
console.log("cm-size=" + cm.size);
console.log("cm-get=" + cm.get("a") + "," + cm.get("b"));
console.log("cm-ctor-calls=" + setCalls.join("|"));
cm.set("c", 3);
console.log("cm-size2=" + cm.size);
console.log("cm-calls=" + setCalls.join("|"));
console.log("cm-instanceof=" + (cm instanceof CountingMap) + "," + (cm instanceof Map));
console.log("cm-tag=" + Object.prototype.toString.call(cm));
console.log("cm-entries=" + Array.from(cm.entries()).map((p) => p[0] + ":" + p[1]).join(","));
console.log("cm-keys=" + Array.from(cm.keys()).join(","));
console.log("cm-own-keys=" + Object.keys(cm).join(","));
console.log("cm-proto-names=" + Object.getOwnPropertyNames(CountingMap.prototype).sort().join(","));

// Map has no species-driven method, so nothing derived comes back typed.
console.log("map-species=" + ((Map as any)[Symbol.species] === Map));
console.log("cm-species=" + ((CountingMap as any)[Symbol.species] === CountingMap));

// A brand check still passes through the subclass, and fails off it.
console.log("brand-ok=" + Map.prototype.has.call(cm, "a"));
let brand = "no-throw";
try {
  Map.prototype.has.call({ size: 0 } as any, "a");
} catch (e: any) {
  brand = e.constructor.name;
}
console.log("brand-off=" + brand);
