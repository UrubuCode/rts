// Cross-runtime: computed assignment evaluates base/key/value in order.
const log: string[] = [];
const obj: any = {};
function base() { log.push("base"); return obj; }
function key() { log.push("key"); return "x"; }
function val() { log.push("val"); return 7; }

base()[key()] = val();
console.log(obj.x);
console.log(log.join(","));
