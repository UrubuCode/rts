// ONE thing: includes uses SameValueZero and READS holes; indexOf uses strict
// equality and SKIPS them. The two disagree on exactly two kinds of input.
const holes: any[] = [1, , 3];
console.log("holesIncludesUndef=" + holes.includes(undefined));
console.log("holesIndexOfUndef=" + holes.indexOf(undefined));
console.log("holesLastIndexOfUndef=" + holes.lastIndexOf(undefined));

const nan = [1, NaN, 3];
console.log("nanIncludes=" + nan.includes(NaN));
console.log("nanIndexOf=" + nan.indexOf(NaN));
console.log("nanFindIndex=" + nan.findIndex((v) => Number.isNaN(v)));

console.log("includesNegZero=" + [0, -0].includes(-0));
console.log("indexOfNegZero=" + [0, -0].indexOf(-0));
console.log("includesPosInNeg=" + [-0].includes(0));
console.log("indexOfPosInNeg=" + [-0].indexOf(0));

const a = [1, 2, 3, 1, 2, 3];
console.log("from3=" + a.includes(1, 3));
console.log("from4=" + a.includes(1, 4));
console.log("fromNeg2=" + a.includes(3, -2));
console.log("fromNeg99=" + a.includes(1, -99));
console.log("from99=" + a.includes(1, 99));
console.log("fromNaN=" + a.includes(1, NaN));
console.log("fromUndef=" + a.includes(1, undefined));

const own: any[] = [1, undefined, 3];
console.log("ownIncludes=" + own.includes(undefined) + " ownIndexOf=" + own.indexOf(undefined));
console.log("arrOfStr=" + ["ab", "cd"].includes("b"));
