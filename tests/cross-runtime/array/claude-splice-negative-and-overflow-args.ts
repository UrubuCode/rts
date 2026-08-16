// ONE thing: how splice clamps its two numeric arguments, and what it RETURNS.
function show(label: string, ...args: any[]) {
  const a = [0, 1, 2, 3, 4];
  const removed = (a.splice as any)(...args);
  console.log(label + " removed=[" + removed.map(String).join(",") + "] rest=[" + a.map(String).join(",") + "] len=" + a.length);
}
show("start2", 2);
show("startNeg2", -2);
show("startNeg99", -99);
show("start99", 99);
show("startNaN", NaN);
show("startUndef", undefined);
show("count_neg5", 1, -5);
show("count_99", 1, 99);
show("count_NaN", 1, NaN);
show("count0_ins", 1, 0, "a", "b");
show("neg1_1_ins", -1, 1, "x");
show("none");
show("frac", 1.9);
show("strings", "2", "1");

const g = [1, 2];
const ret = g.splice(1, 0, "a", "b", "c");
console.log("grow=" + g.join(",") + " retIsArray=" + Array.isArray(ret) + " retLen=" + ret.length);

const h: any[] = [1, , 3, , 5];
const rem = h.splice(1, 3);
console.log("remIn=" + [0, 1, 2].map((i) => (i in rem ? "y" : "n")).join("") + " rem=" + rem.map(String).join(","));
console.log("leftIn=" + [0, 1].map((i) => (i in h ? "y" : "n")).join("") + " left=" + h.map(String).join(","));
