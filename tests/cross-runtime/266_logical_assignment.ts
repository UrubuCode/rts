// Cross-runtime: logical assignment operators.
let a: any = 0;
a ||= 5;
let b: any = "x";
b &&= "y";
let c: any = null;
c ??= 9;
const obj: any = { v: 0, w: "ok", x: null };
obj.v ||= 2;
obj.w &&= "done";
obj["x"] ??= 3;
console.log("vars=" + [a, b, c].join(","));
console.log("obj=" + [obj.v, obj.w, obj.x].join(","));
