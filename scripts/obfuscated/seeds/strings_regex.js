// String methods, template literals, regex, replace callbacks.
const out = [];
const s = "The quick Brown fox";
out.push(s.toUpperCase().slice(4, 9));
out.push(s.split(" ").map((w) => w.length).join("-"));
out.push(s.replace(/(\w+) (\w+)/, "$2 $1"));
out.push(s.replace(/o/g, (m, i) => "[" + i + "]"));
out.push(/(?<adj>\w+) fox/.exec(s).groups.adj);
out.push([...s.matchAll(/[A-Z]/g)].map((m) => m.index).join(","));
const tag = (parts, ...vals) => parts.raw.join("#") + "/" + vals.join("+");
out.push(tag`a${1}b${2}c`);
out.push("abc".padStart(6, "-") + "|" + "abc".at(-1));
out.push(JSON.stringify({ q: 'he said "hi"', n: "\n" }));
console.log(out.join("|"));
