// ONE thing: an assignment INSIDE a loop's condition. The new value must be
// visible on the next pass — an engine that evaluates the test against a copy
// that does not survive the back edge loops forever, so this file is kept apart
// from every other loop fixture: a hang here must not hide anything else.
//
// Every loop below is also bounded by a hard counter, so a broken engine stops
// at the guard instead of running until the harness kills it.
let guard = 0;
function tick(): boolean { return ++guard < 1000; }

// do-while with a compound assignment in the test.
let m = 0;
const doSaw: number[] = [];
do { doSaw.push(m); } while ((m += 2) < 6 && tick());
console.log("doWhile=" + doSaw.join(",") + "|m=" + m + "|guard=" + guard);

// while with a plain assignment in the test.
guard = 0;
let w = 0;
const whileSaw: number[] = [];
while ((w = w + 1) < 4 && tick()) whileSaw.push(w);
console.log("while=" + whileSaw.join(",") + "|w=" + w);

// while with an assignment whose VALUE is the test (the classic read-loop).
guard = 0;
const queue = [3, 2, 1, 0, "", null];
let item: any;
const drained: string[] = [];
let qi = 0;
while ((item = queue[qi++]) && tick()) drained.push(String(item));
console.log("truthyDrain=" + drained.join(",") + "|stoppedAt=" + qi);

// for with the assignment in the condition rather than the update.
guard = 0;
const forSaw: number[] = [];
for (let i = 0; (i = i + 1) <= 3 && tick(); ) forSaw.push(i);
console.log("forCond=" + forSaw.join(","));

// An increment operator in the condition.
guard = 0;
let p = 0;
const postSaw: number[] = [];
while (p++ < 3 && tick()) postSaw.push(p);
console.log("postInc=" + postSaw.join(",") + "|p=" + p);

guard = 0;
let q = 0;
const preSaw: number[] = [];
while (++q < 4 && tick()) preSaw.push(q);
console.log("preInc=" + preSaw.join(",") + "|q=" + q);

// A logical-assignment operator in the condition.
guard = 0;
let r: any = null;
let rounds = 0;
while ((r ||= "set") && rounds++ < 3 && tick()) { /* body */ }
console.log("logicalAssign=" + r + "|rounds=" + rounds);

// The condition mutates a CAPTURED variable that a closure reads.
guard = 0;
let shared = 0;
const readers: (() => number)[] = [];
do { readers.push(() => shared); } while ((shared += 3) < 9 && tick());
console.log("closures=" + readers.map((f) => f()).join(",") + "|shared=" + shared);

// The condition mutates a PROPERTY, not a local.
guard = 0;
const box = { n: 0 };
const boxSaw: number[] = [];
do { boxSaw.push(box.n); } while ((box.n += 2) < 6 && tick());
console.log("property=" + boxSaw.join(",") + "|n=" + box.n);

// The condition mutates an ARRAY ELEMENT.
guard = 0;
const cell = [0];
const cellSaw: number[] = [];
do { cellSaw.push(cell[0]); } while ((cell[0] += 2) < 6 && tick());
console.log("element=" + cellSaw.join(",") + "|v=" + cell[0]);

// A comma operator in the condition, assigning to two names.
guard = 0;
let x = 0, y = 10;
const pairs: string[] = [];
while ((x = x + 1, y = y - 1, x < y) && tick()) pairs.push(x + "/" + y);
console.log("comma=" + pairs.join(" ") + "|x=" + x + "|y=" + y);

console.log("finalGuard=" + guard);
