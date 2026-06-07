let calls: string[] = [];
let api: any = {
  data: null,
  get: function() { calls.push("get"); return null; }
};
let r1 = api.data?.items?.[0];
console.log(r1);
let r2 = api.get()?.handle();
console.log(r2);
console.log(calls.join(","));
let deep: any = { a: { b: null } };
console.log(deep.a?.b?.c?.d);
let fnHolder: any = { run: undefined };
console.log(fnHolder.run?.(1, 2, 3));
let mix: any = { arr: [{ name: "x" }] };
console.log(mix.arr?.[0]?.name);
console.log(mix.arr?.[5]?.name);
let nu: any = null;
console.log(nu?.x.y.z?.());