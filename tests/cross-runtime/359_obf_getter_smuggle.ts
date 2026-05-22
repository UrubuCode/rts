// Cross-runtime: getter-based value smuggling (obfuscator hides values in descriptors).

// Pattern 1: value hidden behind non-enumerable getter
const _vault: any = {};
const _secrets = [0x61, 0x62, 0x63, 0x64, 0x65];
_secrets.forEach((code, i) => {
  Object.defineProperty(_vault, "_" + i, {
    get() { return String.fromCharCode(code); },
    enumerable: false,
    configurable: false
  });
});
// Object.keys won't reveal these
console.log(Object.keys(_vault).length);  // 0
// but direct access works
console.log(_vault._0 + _vault._1 + _vault._2 + _vault._3 + _vault._4); // abcde

// Pattern 2: computed property name obfuscation
const _k1 = ["c","o","n","s","t","r","u","c","t","o","r"].join("");
const _k2 = ["t","o","S","t","r","i","n","g"].join("");
const _obj: any = { hello: 42 };
console.log(typeof _obj[_k1]);    // function
console.log(typeof _obj[_k2]);    // function
console.log(_obj["he"+"llo"]);    // 42

// Pattern 3: prototype pollution guard (obfuscated lib protection)
function _safeGet(obj: any, key: string): any {
  if (key === "__proto__" || key === "constructor" || key === "prototype") return undefined;
  return obj[key];
}
const _data = { a: 1, b: 2, __proto__: { evil: true } };
console.log(_safeGet(_data, "a"));
console.log(_safeGet(_data, "__proto__"));
console.log(_safeGet(_data, "b"));

// Pattern 4: toString override for anti-debug (obfuscator adds this to detect DevTools)
// Safe version — just verify toString can be overridden on functions
function _fn() { return 42; }
const _originalStr = Function.prototype.toString;
(_fn as any).toString = () => "function _fn() { [obfuscated] }";
console.log((_fn as any).toString());
console.log(_fn());  // still works
