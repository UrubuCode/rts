// Cross-runtime: code units vs code points for surrogate pairs.
const s = "A\u{1F642}B";
console.log("len=" + s.length);
console.log("chars=" + Array.from(s).join("|"));
console.log("units=" + [s.charCodeAt(0), s.charCodeAt(1), s.charCodeAt(2), s.charCodeAt(3)].join(","));
console.log("points=" + [s.codePointAt(0), s.codePointAt(1), s.codePointAt(2), s.codePointAt(3)].join(","));
console.log("slice=" + s.slice(1, 3));
