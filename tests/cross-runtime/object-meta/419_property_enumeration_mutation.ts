// Cross-runtime: for..in behavior while object/prototype are mutated.
const proto: any = { p1: 1, p2: 2 };
const obj: any = Object.create(proto);
obj.a = 1;
obj.b = 2;

const seen: string[] = [];
for (const k in obj) {
  seen.push(k);
  if (k === "a") {
    obj.c = 3;
    delete obj.b;
    proto.p3 = 3;
  }
}

console.log(seen.join(","));
console.log(Object.keys(obj).join(","));
console.log("p3" in obj);
