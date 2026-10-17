function foo() {}
class A {}
class B extends Array {}
class C extends A {}
const arrow = () => 1;
const named = function () {};
function withProps() {}
(withProps as any).k = 1;
const box = new Number(1);
const getset = {
  get a() { return 1; },
  set b(v) {},
  get c() { return 1; },
  set c(v) {},
  plain: 1,
};
const circ: any = { name: "x" };
circ.self = circ;
const sym = Symbol("desc");
const nul = Object.create(null);
const nul2 = Object.create(null);
nul2.x = 1;

console.log(/a/gi);
console.log([/a/gi, { r: /b\/c/ }]);
console.log(foo);
console.log(named);
console.log(arrow);
console.log(() => 1);
console.log(function () {});
console.log([foo, { f: foo }]);
console.log(A);
console.log(B);
console.log(C);
console.log([A, C]);
console.log(withProps);
console.log({ w: withProps });
console.log(sym);
console.log([sym]);
console.log({ [sym]: 1, a: 2 });
console.log({ k: sym });
console.log([1, , 3]);
console.log(new Array(3));
console.log([, , 1, , ,]);
console.log({ a: [1, , 3] });
console.log(nul);
console.log(nul2);
console.log({ n: nul });
console.log(new Number(1));
console.log(new String("s"));
console.log(new Boolean(true));
console.log(Object(1n));
console.log([new Number(2), new String("t")]);
console.log(getset);
console.log([getset]);
console.log(Promise.resolve(5));
console.log(new Promise(() => {}));
console.log([Promise.resolve("s")]);
console.log(new Map([[1, 2]]));
console.log(new Set([1, "a"]));
console.log({ m: new Map(), s: new Set() });
console.log(circ);
console.log([circ]);
console.log("top\nlevel");
console.log(["a\nb"]);
console.log(["it's"]);
console.log(['say "hi"']);
console.log(["it's \"both\""]);
console.log(["it's \"both\" `tick`"]);
console.log(["tab\t", "back\\slash"]);
console.log({ s: "x'y" });
console.log(1, "a", [1, "b"], { a: "c" });
console.log(null, undefined, true, 1.5, -0, 10n);
console.log([null, undefined, true, -0, 10n]);
console.log(new Date(0), [new Date(0)]);
console.log({ a: 1, b: { c: [1, 2] } });
const p3 = Promise.reject(3);
p3.catch(() => {});
console.log(p3);
console.log([p3, { p: Promise.resolve({ a: [1] }) }]);
console.log(new (class MyP extends Promise<number> {})(() => {}));
const o2: any = { a: {} };
o2.a.back = o2;
o2.a.self = o2.a;
console.log(o2);
const x: any = {}; const y: any = { x }; x.y = y; x.me = x;
console.log(x);
console.log({ $a: 1, "a-b": 2, 3: 4, _ok: 5 });
console.log(["\x01\x7f", "é", "\u0085"]);
console.log([undefined, , undefined]);
console.log(Object.assign(/x/g, { extra: 1 }));
console.log(Object.assign(new Number(3), { extra: 1 }));
console.log(new String("ab"), [new Boolean(false)]);
console.log([[[[/deep/]]]], [[[function z() {}]]]);
console.log(Object(Symbol("bx")));
console.log(-0, [-0], new Number(-0));
