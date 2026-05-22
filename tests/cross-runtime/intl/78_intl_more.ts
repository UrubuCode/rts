// Cross-runtime compatibility: additional Intl constructors.
const pr = new Intl.PluralRules("en-US");
console.log("one=" + pr.select(1));
console.log("two=" + pr.select(2));

const lf = new Intl.ListFormat("en-US", { style: "long", type: "conjunction" });
console.log("list=" + lf.format(["a", "b", "c"]));

const rf = new Intl.RelativeTimeFormat("en-US", { numeric: "auto" });
console.log("rel=" + rf.format(-1, "day"));
