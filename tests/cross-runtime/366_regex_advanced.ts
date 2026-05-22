// Cross-runtime: advanced regex patterns.

// named capture groups
const dateRe = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/;
const m = "2026-05-22".match(dateRe)!;
console.log(m.groups!.year);
console.log(m.groups!.month);
console.log(m.groups!.day);

// named groups in replace
const swapped = "2026-05-22".replace(
  /(?<y>\d{4})-(?<m>\d{2})-(?<d>\d{2})/,
  "$<d>/$<m>/$<y>"
);
console.log(swapped);  // 22/05/2026

// lookahead / lookbehind
const prices = ["$10", "$20", "€30", "$40"];
const usd = prices.filter(p => /(?<=\$)\d+/.test(p));
console.log(usd.join(","));  // $10,$20,$40

// negative lookahead
const words = ["foo", "foobar", "food", "bar", "baz"];
const fooOnly = words.filter(w => /^foo(?!bar)/.test(w));
console.log(fooOnly.join(","));  // foo,food

// matchAll (all occurrences with groups)
const text = "cat:5 dog:3 bird:8";
const matches = [...text.matchAll(/(\w+):(\d+)/g)];
matches.forEach(m => console.log(m[1] + "=" + m[2]));

// sticky flag
const stickyRe = /\d+/y;
stickyRe.lastIndex = 4;
const stickyResult = stickyRe.exec("abc 123 456");
console.log(stickyResult?.[0]);  // 123
console.log(stickyRe.lastIndex); // 7

// unicode flag
const emoji = "hello 🌍 world";
const noEmoji = emoji.replace(/\p{Emoji}/gu, "?");
console.log(noEmoji);

// split with groups
const parts = "a1b2c3".split(/(\d)/);
console.log(parts.join("|"));  // a|1|b|2|c|3|

// replace with function
const result = "hello world foo".replace(/\b\w+\b/g, w => w[0].toUpperCase() + w.slice(1));
console.log(result);  // Hello World Foo
