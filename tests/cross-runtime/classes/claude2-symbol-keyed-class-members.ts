// Cross-runtime: symbol-keyed class members — methods, accessors, fields and
// statics under a symbol key. They live where a string key would, stay out of
// Object.keys and JSON, and are found only through getOwnPropertySymbols.
const KEY = Symbol("key");
const ACC = Symbol("acc");
const FIELD = Symbol("field");
const STATIC = Symbol("static");
const GEN = Symbol("gen");
const REG = Symbol.for("registered");

class Holder {
  [FIELD]: string = "field-value";
  plain: string = "plain";

  [KEY](): string {
    return "method:" + this.plain;
  }

  get [ACC](): string {
    return "get:" + this.plain;
  }
  set [ACC](v: string) {
    this.plain = "set(" + v + ")";
  }

  *[GEN](): Generator<number, void, undefined> {
    yield 1;
    yield 2;
  }

  static [STATIC](): string {
    return "static-method";
  }

  [REG]: number = 42;
}

const h: any = new Holder();
console.log("method=" + h[KEY]());
console.log("field=" + h[FIELD]);
console.log("registered=" + h[REG]);
console.log("accessor-get=" + h[ACC]);
h[ACC] = "x";
console.log("accessor-after-set=" + h.plain);
console.log("accessor-get2=" + h[ACC]);
console.log("generator=" + Array.from(h[GEN]()).join(","));
console.log("static=" + (Holder as any)[STATIC]());

// A symbol key never appears among string keys.
console.log("keys=" + Object.keys(h).join(","));
console.log("names=" + Object.getOwnPropertyNames(h).join(","));
console.log("json=" + JSON.stringify(h));

// Symbol-keyed FIELDS are own properties of the instance; symbol-keyed METHODS
// and ACCESSORS are on the prototype.
const instSyms = Object.getOwnPropertySymbols(h);
console.log("inst-symbol-count=" + instSyms.length);
console.log("inst-has-field=" + (instSyms.indexOf(FIELD) >= 0));
console.log("inst-has-registered=" + (instSyms.indexOf(REG) >= 0));
console.log("inst-has-method=" + (instSyms.indexOf(KEY) >= 0));

const protoSyms = Object.getOwnPropertySymbols(Holder.prototype);
console.log("proto-symbol-count=" + protoSyms.length);
console.log("proto-has-method=" + (protoSyms.indexOf(KEY) >= 0));
console.log("proto-has-accessor=" + (protoSyms.indexOf(ACC) >= 0));
console.log("proto-has-gen=" + (protoSyms.indexOf(GEN) >= 0));
console.log("class-symbol-count=" + Object.getOwnPropertySymbols(Holder).length);

// Descriptors match their string-keyed counterparts exactly.
const md: any = Object.getOwnPropertyDescriptor(Holder.prototype, KEY);
console.log("method-desc=w" + md.writable + ",e" + md.enumerable + ",c" + md.configurable);
const ad: any = Object.getOwnPropertyDescriptor(Holder.prototype, ACC);
console.log("accessor-desc=e" + ad.enumerable + ",c" + ad.configurable + ",g" + (typeof ad.get) + ",s" + (typeof ad.set));
const fd: any = Object.getOwnPropertyDescriptor(h, FIELD);
console.log("field-desc=w" + fd.writable + ",e" + fd.enumerable + ",c" + fd.configurable);

// The function NAME derived from a symbol key is the description in brackets.
console.log("method-name=" + JSON.stringify(md.value.name));
console.log("getter-name=" + JSON.stringify(ad.get.name));
console.log("setter-name=" + JSON.stringify(ad.set.name));
console.log("static-name=" + JSON.stringify(((Holder as any)[STATIC] as any).name));

// An anonymous symbol (no description) yields an empty bracket pair.
const ANON = Symbol();
class Anon {
  [ANON](): number {
    return 1;
  }
}
const anonDesc: any = Object.getOwnPropertyDescriptor(Anon.prototype, ANON);
console.log("anon-name=" + JSON.stringify(anonDesc.value.name));

// Inheritance carries symbol members like any other.
class SubHolder extends Holder {
  [KEY](): string {
    return "sub+" + super[KEY]();
  }
}
const s: any = new SubHolder();
console.log("sub-method=" + s[KEY]());
console.log("sub-field=" + s[FIELD]);
console.log("sub-in=" + (KEY in s) + "," + (FIELD in s));
console.log("sub-own-field=" + Object.prototype.hasOwnProperty.call(s, FIELD));
console.log("sub-static=" + (SubHolder as any)[STATIC]());

// Symbol.for keys are shared; Symbol("x") keys are not.
console.log("for-lookup=" + (h[Symbol.for("registered")] === 42));
console.log("fresh-lookup=" + String(h[Symbol("field")]));
