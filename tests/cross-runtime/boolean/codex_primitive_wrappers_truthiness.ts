// Cross-runtime: primitive wrappers are truthy objects with valueOf primitives.
const values: any[] = [new Boolean(false), new Number(0), new String("")];
for (const v of values) {
  console.log((v ? "truthy" : "falsy") + ":" + typeof v + ":" + typeof v.valueOf() + ":" + String(v.valueOf()));
}

const b: any = new Boolean(false);
b.extra = 3;
console.log(Object.keys(b).join(",") + ":" + JSON.stringify(b));
