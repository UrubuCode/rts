// Cross-runtime: String.fromCharCode used AS A VALUE (aliased, passed, stored)
// rather than called directly on String. A static method is a first-class
// function object: aliasing it must not change what it does (it ignores `this`).

// --- baseline: the direct call ---
console.log("direct=" + String.fromCharCode(65));
console.log("direct-multi=" + String.fromCharCode(72, 73));

// --- it is a function value with the usual metadata ---
console.log("typeof=" + typeof String.fromCharCode);
console.log("name=" + String.fromCharCode.name);
console.log("length=" + String.fromCharCode.length);

// --- stored in a const, then called ---
const cc = String.fromCharCode;
console.log("alias=" + cc(66));
console.log("alias-multi=" + cc(67, 68, 69));
console.log("alias-none=[" + cc() + "]");

// --- stored in a let and reassigned through ---
let cc2 = String.fromCharCode;
console.log("let-alias=" + cc2(70));

// --- stored in an object property ---
const holder: any = { f: String.fromCharCode };
console.log("obj-prop=" + holder.f(71));

// --- stored in an array element ---
const arr: any[] = [String.fromCharCode];
console.log("arr-elem=" + arr[0](72));

// --- passed as a callback to map ---
console.log("map=" + [73, 74, 75].map((n) => String.fromCharCode(n)).join(""));
console.log("map-direct=" + [76, 77, 78].map(cc).join("|"));

// --- passed as an argument to a user function ---
function callWith(fn: any, code: number): string {
  return fn(code);
}
console.log("passed=" + callWith(String.fromCharCode, 79));
console.log("passed-alias=" + callWith(cc, 80));

// --- returned from a function ---
function getFn(): any {
  return String.fromCharCode;
}
console.log("returned=" + getFn()(81));

// --- captured by a closure ---
function makeCaller(): any {
  const inner = String.fromCharCode;
  return (n: number) => inner(n);
}
console.log("closure=" + makeCaller()(82));

// --- invoked via call / apply / bind ---
console.log("call=" + String.fromCharCode.call(null, 83));
console.log("apply=" + String.fromCharCode.apply(null, [84, 85]));
const bound = String.fromCharCode.bind(null);
console.log("bind=" + bound(86));
const boundArg = String.fromCharCode.bind(null, 87);
console.log("bind-partial=" + boundArg(88));

// --- the alias is the same function object ---
console.log("identity=" + (cc === String.fromCharCode));
console.log("obj-identity=" + (holder.f === String.fromCharCode));

// --- spread of an array of codes ---
const codes = [89, 90];
console.log("spread=" + String.fromCharCode(...codes));
console.log("alias-spread=" + cc(...codes));

// --- the same treatment for fromCodePoint ---
const fcp = String.fromCodePoint;
console.log("fcp-direct=" + String.fromCodePoint(97));
console.log("fcp-alias=" + fcp(98));
console.log("fcp-alias-astral-len=" + fcp(0x1f600).length);
