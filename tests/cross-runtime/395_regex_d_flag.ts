// Cross-runtime: regex d flag (indices) — ES2022.

// d flag provides start/end indices for each match
const re = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/d;
const m = re.exec("date: 2026-05-22 end")!;

console.log(m[0]);          // 2026-05-22
console.log(m.index);       // 6

// m.indices provides [start, end] for each group
console.log(m.indices![0].join(","));  // 6,16  (full match)
console.log(m.indices![1].join(","));  // 6,10  (year)
console.log(m.indices![2].join(","));  // 11,13 (month)
console.log(m.indices![3].join(","));  // 14,16 (day)

// named group indices
console.log(m.indices!.groups!.year.join(","));   // 6,10
console.log(m.indices!.groups!.month.join(","));  // 11,13
console.log(m.indices!.groups!.day.join(","));    // 14,16

// Verify by slicing original string
const str = "date: 2026-05-22 end";
const [yearStart, yearEnd] = m.indices![1];
console.log(str.slice(yearStart, yearEnd));   // 2026

// matchAll with d flag
const text = "foo bar baz";
const wordRe = /\b(\w+)\b/dg;
const positions: string[] = [];
for (const match of text.matchAll(wordRe)) {
  positions.push(match[1] + "@" + match.indices![1].join("-"));
}
console.log(positions.join("|"));  // foo@0-3|bar@4-7|baz@8-11

// optional group indices (non-participating group = undefined)
const optRe = /(\d+)(?:\.(\d+))?/d;
const m2 = optRe.exec("42")!;
console.log(m2[1]);              // 42
console.log(m2[2]);              // undefined
console.log(m2.indices![1].join(",")); // 0,2
console.log(m2.indices![2]);     // undefined
