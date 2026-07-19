// Cluster: collections (Map/Set), proxy, reflect, function, error, string.
const m = new Map<string, number>();
m.set("a", 1); m.set("b", 2);
console.log("map:" + m.get("a") + "," + m.get("b") + ",size=" + m.size);

const s = new Set<number>([1, 2, 2, 3]);
console.log("set-size:" + s.size);

const target = { x: 10 };
const p = new Proxy(target, {
  get(t: any, k: string) { return k in t ? t[k] : 42; },
});
console.log("proxy:" + p.x + "," + p.missing);

console.log("reflect-has:" + Reflect.has(target, "x"));
console.log("reflect-keys:" + Reflect.ownKeys(target).join(","));

function add(this: any, a: number, b: number): number { return a + b; }
const bound = add.bind(null, 5);
console.log("bind:" + bound(7));
console.log("call:" + add.call(null, 3, 4));

try { throw new TypeError("bad"); } catch (e: any) {
  console.log("err:" + e.name + ":" + e.message);
}

const str = "a,b,c,d";
console.log("split:" + str.split(",").length);
console.log("replace:" + str.replace("b", "X"));
console.log("upper:" + "hi".toUpperCase());
