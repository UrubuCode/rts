// Cross-runtime: the full lastIndex matrix. A plain regex NEVER reads or writes
// lastIndex; /g reads and writes it; /y reads it, writes it on success and RESETS
// IT TO 0 on failure; and replace/match/split/search each set it differently.
// 171 and 240 test one flag on one method; this pins the whole grid in one place.

function grid(re: RegExp, subject: string, start: number): string {
  re.lastIndex = start;
  const ok = re.test(subject);
  return re.flags + " li" + start + " -> " + ok + " li" + re.lastIndex;
}

const S = "xaxa";

// --- test() across the three modes ---
console.log("t-plain-0=" + grid(/a/, S, 0));
console.log("t-plain-9=" + grid(/a/, S, 9));
console.log("t-g-0=" + grid(/a/g, S, 0));
console.log("t-g-2=" + grid(/a/g, S, 2));
console.log("t-g-4=" + grid(/a/g, S, 4));
console.log("t-y-0=" + grid(/a/y, S, 0));
console.log("t-y-1=" + grid(/a/y, S, 1));
console.log("t-y-3=" + grid(/a/y, S, 3));
console.log("t-gy-1=" + grid(/a/gy, S, 1));
console.log("t-gy-0=" + grid(/a/gy, S, 0));

// --- a /g regex walks the string across repeated calls, then wraps to 0 ---
const g = /a/g;
const walk: string[] = [];
for (let i = 0; i < 4; i++) walk.push(g.test(S) + ":" + g.lastIndex);
console.log("walk=" + walk.join(" "));

// --- exec() writes the same slot ---
const e = /a/g;
e.lastIndex = 2;
const m1: any = e.exec(S);
console.log("exec-hit=" + m1[0] + ":" + m1.index + ":" + e.lastIndex);
const m2: any = e.exec(S);
console.log("exec-miss=" + String(m2) + ":" + e.lastIndex);
const ePlain = /a/;
ePlain.lastIndex = 3;
console.log("exec-plain=" + (ePlain.exec(S) as any).index + ":" + ePlain.lastIndex);

// --- a sticky failure resets to 0 even when it started elsewhere ---
const y = /a/y;
y.lastIndex = 0;
console.log("y-fail=" + y.test(S) + ":" + y.lastIndex);
y.lastIndex = 3;
console.log("y-hit=" + y.test(S) + ":" + y.lastIndex);
y.lastIndex = 3;
console.log("y-exec-then-fail=" + (y.exec(S) as any)[0] + ":" + y.lastIndex + ":" + String(y.exec(S)) + ":" + y.lastIndex);

// --- String.replace: /g starts from 0 and finishes at 0; plain leaves it alone ---
const rg = /a/g;
rg.lastIndex = 3;
console.log("replace-g=" + "aaa".replace(rg, "-") + ":" + rg.lastIndex);
const rp = /a/;
rp.lastIndex = 3;
console.log("replace-plain=" + "aaa".replace(rp, "-") + ":" + rp.lastIndex);
const rgy = /a/gy;
rgy.lastIndex = 1;
console.log("replace-gy=" + "aab".replace(rgy, "-") + ":" + rgy.lastIndex);

// --- match: /g returns every match and ends at 0; plain returns one exec result ---
const mg = /a/g;
mg.lastIndex = 3;
console.log("match-g=" + (("aaa".match(mg) as any) || []).join(",") + ":" + mg.lastIndex);
const mp = /a/;
mp.lastIndex = 3;
console.log("match-plain=" + ("aaa".match(mp) as any).index + ":" + mp.lastIndex);

// --- search RESTORES lastIndex to whatever it was, even on a /g regex ---
const s1 = /a/g;
s1.lastIndex = 9;
console.log("search-g=" + "xa".search(s1) + ":" + s1.lastIndex);
const s2 = /a/y;
s2.lastIndex = 9;
console.log("search-y=" + "xa".search(s2) + ":" + s2.lastIndex);

// --- split ignores lastIndex entirely and leaves it untouched ---
const sp = /,/g;
sp.lastIndex = 5;
console.log("split-g=" + "a,b,c".split(sp).join("|") + ":" + sp.lastIndex);

// --- the property is writable but neither enumerable nor configurable ---
const d: any = Object.getOwnPropertyDescriptor(/a/, "lastIndex");
console.log("desc=" + d.value + "/" + d.writable + "/" + d.enumerable + "/" + d.configurable);
console.log("own=" + Object.getOwnPropertyNames(/a/).join(","));

// --- a frozen /g regex cannot have lastIndex written, so test() throws ---
const frozen = /a/g;
Object.freeze(frozen);
try {
  console.log("frozen-g=" + frozen.test("a"));
} catch (err: any) {
  console.log("frozen-g!" + err.constructor.name);
}
const frozenPlain = /a/;
Object.freeze(frozenPlain);
console.log("frozen-plain=" + frozenPlain.test("a"));

// --- a non-integer lastIndex goes through ToLength ---
const frac = /a/g;
frac.lastIndex = 1.7 as any;
console.log("frac=" + frac.test("xa") + ":" + frac.lastIndex);
const neg = /a/g;
neg.lastIndex = -5 as any;
console.log("neg=" + neg.test("xa") + ":" + neg.lastIndex);
const str = /a/g;
str.lastIndex = "1" as any;
console.log("str=" + str.test("xa") + ":" + str.lastIndex);
