// ONE thing: flat REMOVES holes at every level, and only flattens real arrays —
// an array-like or a string is left whole.
const h: any[] = [1, , 2, [3, , 4], , 5];
console.log("flat1=" + JSON.stringify(h.flat()));
console.log("flat1Len=" + h.flat().length);

const deep: any[] = [1, [2, [3, [4, [5]]]]];
console.log("d0=" + JSON.stringify(deep.flat(0)));
console.log("d1=" + JSON.stringify(deep.flat(1)));
console.log("d2=" + JSON.stringify(deep.flat(2)));
console.log("dInf=" + JSON.stringify(deep.flat(Infinity)));
console.log("dNeg=" + JSON.stringify(deep.flat(-1)));
console.log("dNaN=" + JSON.stringify(deep.flat(NaN)));
console.log("dStr=" + JSON.stringify(deep.flat("2" as any)));

const al: any[] = [[1], { length: 2, 0: "a", 1: "b" }, "xy"];
console.log("arrayLike=" + JSON.stringify(al.flat()));

const src: any[] = [1, , 2];
console.log("flatMap=" + JSON.stringify(src.flatMap((v) => [v, v])));
console.log("flatMapNested=" + JSON.stringify([1, 2].flatMap((v) => [[v]])));

class MyArr extends Array {}
const m: any = MyArr.from([1, [2]] as any);
console.log("speciesFlat=" + (m.flat() instanceof MyArr));

try { (src as any).flatMap(1); } catch (e: any) { console.log("badCb=" + e.constructor.name); }
