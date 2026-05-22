// Cross-runtime compatibility: structuredClone with richer values.
const original = {
  date: new Date(Date.UTC(2024, 4, 10)),
  regex: /ab+/gi,
  error: new Error("boom"),
};
(original as any).self = original;

const copy = structuredClone(original);
console.log("date=" + copy.date.toISOString());
console.log("regex=" + copy.regex.source + ":" + copy.regex.flags);
console.log("error=" + copy.error.name + ":" + copy.error.message);
console.log("self=" + String(copy.self === copy));
