// Cross-runtime: Array.from snapshots array-like length then reads indexed values in order.
const seen: string[] = [];
const source: any = {
  get length() { seen.push("length"); return 3; },
  get 0() { seen.push("0"); this[1] = "changed"; return "a"; },
  1: "b",
  get 2() { seen.push("2"); return "c"; },
};
const out = Array.from(source, (v, i) => i + ":" + v);
console.log(out.join("|"));
console.log(seen.join(","));

