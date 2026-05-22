// Cross-runtime compatibility: Intl with fixed locale/options.
const nf = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
});
console.log("money=" + nf.format(1234.5));

const dtf = new Intl.DateTimeFormat("en-GB", {
  timeZone: "UTC",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});
console.log("date=" + dtf.format(new Date(Date.UTC(2024, 4, 10))));

const coll = new Intl.Collator("en", { sensitivity: "base" });
console.log("cmp=" + coll.compare("alpha", "ALPHA"));
