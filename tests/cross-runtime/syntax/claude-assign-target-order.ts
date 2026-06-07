let log: string[] = [];
function obj(tag: number): any { log.push("obj" + tag); return { v: 0 }; }
function key(tag: number): string { log.push("key" + tag); return "v"; }
function val(tag: number): number { log.push("val" + tag); return tag; }
let o = obj(1);
o[key(2)] = val(3);
console.log(o.v);
console.log(log.join(","));
let arr = [0, 0, 0];
let idx = 0;
arr[idx++] = idx;
console.log(arr.join(","));
console.log(idx);
let m = { x: 1 };
m.x += (m.x = 10);
console.log(m.x);