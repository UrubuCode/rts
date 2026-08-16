// ONE thing: where sort puts undefined and holes. Both move to the end and the
// comparator NEVER sees them — undefined first, holes last, always.
const a: any[] = [3, undefined, 1, undefined, 2];
console.log("undef=" + JSON.stringify(a.sort()));

const seen: string[] = [];
const b: any[] = [3, undefined, 1];
b.sort((x, y) => { seen.push(String(x) + "/" + String(y)); return x - y; });
console.log("comparatorSaw=" + seen.join(" "));
console.log("result=" + JSON.stringify(b));

const holes: any[] = [5, , 1, , 3];
holes.sort();
console.log("holesLen=" + holes.length);
console.log("holesIn=" + [0, 1, 2, 3, 4].map((i) => (i in holes ? "y" : "n")).join(""));
console.log("holesVals=" + holes.map((v) => String(v)).join(","));

const mixed: any[] = [2, , undefined, 1];
mixed.sort();
console.log("mixedLen=" + mixed.length);
console.log("mixedIn=" + [0, 1, 2, 3].map((i) => (i in mixed ? "y" : "n")).join(""));
console.log("mixed0=" + String(mixed[0]) + " mixed2=" + String(mixed[2]));

const t = [5, , 1, undefined, 3].toSorted();
console.log("toSortedLen=" + t.length);
console.log("toSortedIn=" + [0, 1, 2, 3, 4].map((i) => (i in t ? "y" : "n")).join(""));
console.log("toSorted=" + t.map((v) => String(v)).join(","));
