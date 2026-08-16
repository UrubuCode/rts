// Cross-runtime: calling RegExp.prototype[Symbol.replace] / [Symbol.search] /
// [Symbol.split] / [Symbol.match] DIRECTLY, which is what the String methods do
// underneath. Pins that the subject is coerced there (not before), that
// Symbol.search saves and restores lastIndex while Symbol.replace resets it, and
// that Symbol.split builds its own sticky clone.

// --- Symbol.replace, with a string and with a function replacement ---
console.log("rep=" + /b/[Symbol.replace]("abc", "X"));
console.log("rep-g=" + /b/g[Symbol.replace]("abcb", "X"));
console.log("rep-amp=" + /b/[Symbol.replace]("abc", "[$&]"));
console.log("rep-group=" + /(b)(c)/[Symbol.replace]("abc", "$2$1"));
console.log("rep-named=" + /(?<x>b)/[Symbol.replace]("abc", "[$<x>]"));
console.log("rep-fn=" + /(b)/[Symbol.replace]("abc", (...a: any[]) => "<" + a.length + ":" + a[1] + ":" + a[2] + ">"));
console.log("rep-nomatch=" + /z/[Symbol.replace]("abc", "X"));

// --- the subject is coerced inside, so a number works ---
console.log("rep-number=" + /2/[Symbol.replace](123 as any, "-"));
console.log("rep-array=" + /,/g[Symbol.replace]([1, 2, 3] as any, ";"));
console.log("rep-undef=" + /n/[Symbol.replace](undefined as any, "-"));

// --- a GLOBAL regex has lastIndex forced to 0 before, and left at 0 after ---
const g = /a/g;
g.lastIndex = 2;
console.log("rep-g-lastIndex=" + g[Symbol.replace]("aaa", "-") + ":" + g.lastIndex);

// --- a NON-global regex reads lastIndex only if it is sticky ---
const plain = /a/;
plain.lastIndex = 2;
console.log("rep-plain=" + plain[Symbol.replace]("aaa", "-") + ":" + plain.lastIndex);
const sticky = /a/y;
sticky.lastIndex = 1;
console.log("rep-sticky=" + sticky[Symbol.replace]("baa", "-") + ":" + sticky.lastIndex);
const stickyMiss = /a/y;
stickyMiss.lastIndex = 0;
console.log("rep-sticky-miss=" + stickyMiss[Symbol.replace]("baa", "-") + ":" + stickyMiss.lastIndex);

// --- Symbol.search returns an index and RESTORES lastIndex exactly ---
console.log("search=" + /c/[Symbol.search]("abc"));
console.log("search-miss=" + /z/[Symbol.search]("abc"));
const sg = /c/g;
sg.lastIndex = 9;
console.log("search-g=" + sg[Symbol.search]("abc") + ":" + sg.lastIndex);
const sy = /a/y;
sy.lastIndex = 5;
console.log("search-y=" + sy[Symbol.search]("xa") + ":" + sy.lastIndex);
console.log("search-coerce=" + /2/[Symbol.search](123 as any));

// --- Symbol.match: an array for /g, an exec result otherwise ---
const mOne: any = /b/[Symbol.match]("abc");
console.log("match-one=" + mOne[0] + ":" + mOne.index + ":" + mOne.input);
const mAll: any = /[ab]/g[Symbol.match]("abc");
console.log("match-g=" + mAll.join(",") + ":" + String(mAll.index));
console.log("match-miss=" + String(/z/[Symbol.match]("abc")));
console.log("match-g-miss=" + String(/z/g[Symbol.match]("abc")));
const mg = /a/g;
mg.lastIndex = 2;
console.log("match-g-lastIndex=" + (mg[Symbol.match]("aaa") as any).length + ":" + mg.lastIndex);

// --- Symbol.split ignores the global flag and never touches lastIndex ---
console.log("split=" + /,/[Symbol.split]("a,b,c").join("|"));
console.log("split-g=" + /,/g[Symbol.split]("a,b,c").join("|"));
console.log("split-lim=" + /,/[Symbol.split]("a,b,c", 2).join("|"));
console.log("split-cap=" + JSON.stringify(/(\d)/[Symbol.split]("a1b")));
const spg = /,/g;
spg.lastIndex = 4;
console.log("split-lastIndex=" + spg[Symbol.split]("a,b").join("|") + ":" + spg.lastIndex);
console.log("split-empty-subject=" + JSON.stringify(/x/[Symbol.split]("")));
console.log("split-empty-both=" + JSON.stringify(/(?:)/[Symbol.split]("")));
console.log("split-sticky=" + /,/y[Symbol.split]("a,b,c").join("|"));

// --- Symbol.matchAll returns an iterator, not an array ---
const it: any = /b/g[Symbol.matchAll]("abcb");
console.log("matchAll-typeof=" + typeof it.next);
console.log("matchAll-count=" + [...it].length);
try {
  console.log("matchAll-nong=" + [.../b/[Symbol.matchAll]("ab") as any].length);
} catch (e: any) {
  console.log("matchAll-nong!" + e.constructor.name);
}

// --- the methods live on the PROTOTYPE, shared by every regex ---
console.log("shared=" + (/a/[Symbol.replace] === /b/[Symbol.replace]));
console.log("on-proto=" + Object.prototype.hasOwnProperty.call(RegExp.prototype, Symbol.replace));
console.log("not-own=" + Object.prototype.hasOwnProperty.call(/a/, Symbol.replace));
console.log("types=" + [typeof RegExp.prototype[Symbol.replace], typeof RegExp.prototype[Symbol.search],
  typeof RegExp.prototype[Symbol.split], typeof RegExp.prototype[Symbol.match]].join(","));
